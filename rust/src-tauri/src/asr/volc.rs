//! 豆包/火山引擎 SAUC ASR（WebSocket 流式，自定义二进制帧）。
//! 对应 Go 版的 volc_asr.go。

use crate::config::Config;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::handshake::client::Request as HandshakeRequest;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::Message;

/// 防呆上限:cfg.hotwords 可能有上百上千词,完全无脑塞会让 init payload 巨大。
/// 文档标称双向流式 100 tokens,但实测 269 chars(15 词)server 不报错,说明
/// 火山可能只是软性建议或 server 端自己截。**不主动扔** —— 由 server 自己决定。
/// 这里的 50 词只是防极端情况(用户配了上千词时不一次发完)。
const MAX_INJECT_HOTWORDS: usize = 50;

// **endpoint 选择**:用优化版 `bigmodel_async`(rtf / 首尾字时延更优,只在
// 结果变化时返回数据包)。
//
// **热词的现状**(2026-05-19 验证):
// 极小词典(只 1 词 `Claude`,context 32 chars 远低于文档 100 tokens 上限)
// 实测 "用 Claude 写代码" 仍被识成 "用 Cloud Code 写代码",`all_matched_hotwords`
// 一直为空。说明 `bigmodel_async` 上 inline `corpus.context` 注入的 hotwords
// 在 server 端权重极低或根本不生效。
//
// 长期方案:火山控制台预建热词表 + `corpus.boosting_table_id` 引用(走自学习
// 平台路径)。要做需要用户手动操作控制台,目前先保留 inline 注入(对部分
// 不常见的中文术语 / 没同音替代品的英文词如 ClickHouse 仍有效),不强求 100%
// 命中。voice-claude 的热词另一条路 —— LLM 校正 prompt 注入 —— 在 LLM 层
// 仍能纠回写法(`Cloud code` → `Claude code`)。
const VOLC_ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";

// Protocol 常量
const VERSION: u8 = 0x01;
const HEADER_SIZE: u8 = 0x01;
const MSG_FULL_CLIENT_REQUEST: u8 = 0x01;
const MSG_AUDIO_ONLY_REQUEST: u8 = 0x02;
const MSG_SERVER_ERROR: u8 = 0x0F;

const FLAG_NO_SEQUENCE: u8 = 0x00;
const FLAG_LAST_PACKET: u8 = 0x02;
const FLAG_ASYNC_FINAL: u8 = 0x04;

const SER_NONE: u8 = 0x00;
const SER_JSON: u8 = 0x01;
const COMP_NONE: u8 = 0x00;

fn encode_header(msg_type: u8, flags: u8, ser: u8, comp: u8) -> [u8; 4] {
    [
        (VERSION << 4) | (HEADER_SIZE & 0x0F),
        (msg_type << 4) | (flags & 0x0F),
        (ser << 4) | (comp & 0x0F),
        0x00,
    ]
}

fn encode_message(msg_type: u8, flags: u8, ser: u8, comp: u8, payload: &[u8]) -> Vec<u8> {
    let header = encode_header(msg_type, flags, ser, comp);
    let size = (payload.len() as u32).to_be_bytes();
    let mut buf = Vec::with_capacity(4 + 4 + payload.len());
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&size);
    buf.extend_from_slice(payload);
    buf
}

/// 从 config.hotwords 抽出要注入 ASR 的词列表。
/// 去空 / 去重 / 字典序排列,超过 MAX_INJECT_HOTWORDS 截断(豆包 100 tokens 上限)。
fn collect_hotwords(hotwords: &[String]) -> Vec<String> {
    let set: BTreeSet<String> = hotwords
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    set.into_iter().take(MAX_INJECT_HOTWORDS).collect()
}

fn build_client_request(uid: &str, hotwords: &[String]) -> Vec<u8> {
    let mut request = json!({
        "model_name": "bigmodel",
        "enable_punc": true,
        "enable_ddc": true,
        "enable_nonstream": true,
        "show_utterances": true,
        "result_type": "full",
        "end_window_size": 3000,
    });
    if !hotwords.is_empty() {
        // 豆包大模型流式 ASR 的"上下文热词"约定:`request.corpus.context` 是
        // 一个**可被 unmarshal 的 JSON 字符串**(对,字符串里再包一层 JSON),
        // schema 是 `{"hotwords":[{"word":"X"}, ...]}`。详见火山文档
        // https://www.volcengine.com/docs/6561/1354869
        //
        // 文档说双向流式上限 100 tokens,但实测超过也不报错(server 静默截或
        // 直接接受);我们这边只保 collect_hotwords 的 50 词防呆,不主动按
        // char 数裁 —— 如果 server 真有问题会回 server error,届时再调。
        let ctx_inner = json!({
            "hotwords": hotwords.iter().map(|w| json!({ "word": w })).collect::<Vec<_>>(),
        });
        let ctx_str = ctx_inner.to_string();
        tracing::debug!(
            words = hotwords.len(),
            ctx_chars = ctx_str.chars().count(),
            "豆包 corpus.context 拼接"
        );
        request["corpus"] = json!({ "context": ctx_str });
    }
    let payload = json!({
        "user": { "uid": uid },
        "audio": {
            "format": "pcm",
            "codec": "raw",
            "rate": 16000,
            "bits": 16,
            "channel": 1,
        },
        "request": request,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    // dump 完整 init payload 到 debug 日志,方便用户对照官方文档验证 hotwords
    // 字段在不在对的位置;debug 级别避免污染默认日志。
    tracing::debug!(
        payload = %String::from_utf8_lossy(&bytes),
        "豆包 init payload"
    );
    bytes
}

#[derive(Deserialize, Default)]
struct VolcResult {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize, Default)]
struct VolcPayload {
    #[serde(default)]
    result: VolcResult,
}

/// 解码服务端响应，返回 (文本, 是否 final)。
fn decode_response(data: &[u8]) -> Result<(String, bool)> {
    if data.len() < 8 {
        anyhow::bail!("响应数据过短 len={}", data.len());
    }
    let msg_type = (data[1] >> 4) & 0x0F;
    let flags = data[1] & 0x0F;
    let payload_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    // 调试用:dump 每个收到的服务端帧的头信息 + payload 前 256 字节 hex,定位
    // 火山协议里 server 发的具体 msg_type(0x09=FullResponse / 0x0B=Ack / 0x0F=Error)。
    let payload_preview: String = data
        .iter()
        .skip(8)
        .take(256)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("");
    let payload_text: String = data
        .iter()
        .skip(8)
        .take(256)
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    tracing::debug!(
        msg_type = format_args!("0x{:02x}", msg_type),
        flags = format_args!("0x{:02x}", flags),
        total_len = data.len(),
        payload_size,
        hex = %payload_preview,
        text = %payload_text,
        "豆包响应帧"
    );
    if msg_type == MSG_SERVER_ERROR {
        anyhow::bail!("服务端错误 msg_type=0x{:02x}", msg_type);
    }
    // 火山协议:flags & 0x01 = 1 表示帧带 sequence number(4 字节,跟在 header 后),
    // 真正的 payload_size 在 sequence 之后。flags=0x00 时无 sequence,payload 紧跟 header。
    let has_sequence = (flags & 0x01) != 0;
    let size_offset = if has_sequence { 8 } else { 4 };
    let payload_offset = size_offset + 4;
    if data.len() < payload_offset {
        anyhow::bail!(
            "数据不足以读取 payload size has_sequence={} got={}",
            has_sequence,
            data.len()
        );
    }
    let real_payload_size = u32::from_be_bytes([
        data[size_offset],
        data[size_offset + 1],
        data[size_offset + 2],
        data[size_offset + 3],
    ]) as usize;
    if data.len() < payload_offset + real_payload_size {
        anyhow::bail!(
            "payload 不完整 expect={} got={}",
            payload_offset + real_payload_size,
            data.len()
        );
    }
    let payload = &data[payload_offset..payload_offset + real_payload_size];
    let parsed: VolcPayload = serde_json::from_slice(payload).map_err(|e| {
        anyhow::anyhow!(
            "JSON 解析失败 msg_type=0x{:02x} flags=0x{:02x} payload_size={}: {}",
            msg_type,
            flags,
            real_payload_size,
            e
        )
    })?;
    // final 判定:flags 的 bit 1 (0x02 = LAST_PACKET) 或 bit 2 (0x04 = ASYNC_FINAL,
    // 不同 endpoint 可能用不同位)。bigmodel_async 实测用 0x02,保留 0x04 兼容其他 endpoint。
    let is_final = (flags & 0x02) != 0 || (flags & FLAG_ASYNC_FINAL) != 0;
    Ok((parsed.result.text, is_final))
}

pub async fn transcribe_stream(
    cfg: &Config,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    on_partial: Box<dyn Fn(String) + Send + Sync>,
    ready: oneshot::Sender<()>,
) -> Result<String> {
    if cfg.volc_app_key.is_empty() || cfg.volc_access_token.is_empty() {
        anyhow::bail!("请配置豆包 App Key 和 Access Token");
    }

    // 构造带 header 的 WebSocket 握手
    let uri: Uri = VOLC_ENDPOINT.parse()?;
    let connect_id = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or(0)
        .to_string();
    let req = HandshakeRequest::builder()
        .uri(&uri)
        .header("Host", uri.host().unwrap_or(""))
        .header("X-Api-App-Key", &cfg.volc_app_key)
        .header("X-Api-Access-Key", &cfg.volc_access_token)
        .header("X-Api-Resource-Id", &cfg.volc_resource_id)
        .header("X-Api-Connect-Id", &connect_id)
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .body(())?;

    let (ws_stream, _) = match tokio_tungstenite::connect_async(req).await {
        Ok(v) => v,
        Err(e) => {
            // 握手失败时,把火山返回的 HTTP 状态码 / response body / 关键 header 完整打出来,
            // 方便定位 403 / 应用未开通 / resource_id 不匹配等业务错误码。
            // tokio-tungstenite 默认会丢掉 response body,这里显式抓取。
            if let tokio_tungstenite::tungstenite::Error::Http(response) = &e {
                let status = response.status();
                let body = response
                    .body()
                    .as_ref()
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default();
                let logid = response
                    .headers()
                    .get("X-Tt-Logid")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                tracing::error!(
                    status = %status,
                    body = %body,
                    logid = %logid,
                    "豆包 WebSocket 握手失败"
                );
            }
            return Err(anyhow::Error::new(e).context("豆包连接失败"));
        }
    };
    let (mut write, mut read) = ws_stream.split();

    // 发送初始化请求
    let uid = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or(0)
        .to_string();
    let hotwords = collect_hotwords(&cfg.hotwords);
    if !hotwords.is_empty() {
        tracing::info!(count = hotwords.len(), "豆包 ASR 注入热词");
    }
    let init_payload = build_client_request(&uid, &hotwords);
    let init_msg = encode_message(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_NO_SEQUENCE,
        SER_JSON,
        COMP_NONE,
        &init_payload,
    );
    write
        .send(Message::Binary(init_msg.into()))
        .await
        .context("发送初始化请求失败")?;

    let _ = ready.send(());

    // 跟踪最近一次 partial:豆包的 partial 是累积式(每次 partial 都是从录音
    // 开头到当前的完整文本),所以最后一次 partial 就是已识别的全部内容。
    // 服务端某些场景下不会发 is_final=true 帧(比如长录音),用它兜底当 final。
    let last_partial = Arc::new(Mutex::new(String::new()));
    let last_partial_for_recv = Arc::clone(&last_partial);
    let recv_task = tokio::spawn(async move {
        let mut final_text = String::new();
        while let Some(msg) = read.next().await {
            let bytes = match msg {
                Ok(Message::Binary(b)) => b.to_vec(),
                Ok(Message::Text(t)) => t.as_bytes().to_vec(),
                Ok(Message::Close(_)) => break,
                Ok(_) => continue,
                Err(_) => break,
            };
            match decode_response(&bytes) {
                Ok((text, is_final)) => {
                    if text.is_empty() {
                        continue;
                    }
                    if is_final {
                        final_text = text;
                        tracing::debug!(text = %final_text, "豆包最终结果");
                        break;
                    } else {
                        tracing::debug!(%text, "豆包中间结果");
                        *last_partial_for_recv.lock() = text.clone();
                        on_partial(text);
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "豆包响应解析失败");
                    continue;
                }
            }
        }
        final_text
    });

    // 发送 PCM
    let send_task = tokio::spawn(async move {
        while let Some(chunk) = pcm_rx.recv().await {
            let msg = encode_message(
                MSG_AUDIO_ONLY_REQUEST,
                FLAG_NO_SEQUENCE,
                SER_NONE,
                COMP_NONE,
                &chunk,
            );
            if write.send(Message::Binary(msg.into())).await.is_err() {
                return write;
            }
        }
        // 发送结束帧
        let end = encode_message(
            MSG_AUDIO_ONLY_REQUEST,
            FLAG_LAST_PACKET,
            SER_NONE,
            COMP_NONE,
            &[],
        );
        let _ = write.send(Message::Binary(end.into())).await;
        write
    });

    let _ = send_task.await;

    // 等服务端给 is_final=true 帧;长录音(>30s)经常没有,fallback 到 last_partial
    // (跟用户实际听到的中间结果一致,比交白卷强)。30s 覆盖正常场景的网络抖动 +
    // server 处理延迟;再不来就走 fallback。
    const FINAL_WAIT_SECS: u64 = 30;
    let final_text =
        match tokio::time::timeout(Duration::from_secs(FINAL_WAIT_SECS), recv_task).await {
            Ok(Ok(text)) if !text.is_empty() => text,
            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                let partial = last_partial.lock().clone();
                if partial.is_empty() {
                    tracing::warn!("豆包未拿到 final 也无 partial,识别结果为空");
                } else {
                    tracing::info!(
                        text = %partial,
                        "豆包未拿到 final 帧,fallback 用最近一次 partial 当结果"
                    );
                }
                partial
            }
        };
    Ok(final_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_format() {
        let h = encode_header(
            MSG_FULL_CLIENT_REQUEST,
            FLAG_NO_SEQUENCE,
            SER_JSON,
            COMP_NONE,
        );
        assert_eq!(h[0], (VERSION << 4) | HEADER_SIZE);
        assert_eq!(h[1] >> 4, MSG_FULL_CLIENT_REQUEST);
        assert_eq!(h[2] >> 4, SER_JSON);
        assert_eq!(h[3], 0);
    }

    #[test]
    fn decode_short_fails() {
        assert!(decode_response(&[0u8; 4]).is_err());
    }

    #[test]
    fn decode_final_result() {
        let payload = r#"{"result":{"text":"hello"}}"#.as_bytes();
        let msg = encode_message(0x09, FLAG_ASYNC_FINAL, SER_JSON, COMP_NONE, payload);
        let (text, is_final) = decode_response(&msg).unwrap();
        assert_eq!(text, "hello");
        assert!(is_final);
    }

    #[test]
    fn collect_hotwords_dedups_and_trims() {
        let v: Vec<String> = vec![
            "克劳德".into(),
            "Claude".into(),
            "吉他布".into(),
            "GitHub".into(),
            "  ".into(), // 空白忽略
            "API".into(),
            "API".into(), // 重复去重
            "艾皮爱".into(),
        ];
        let words = collect_hotwords(&v);
        // 去空 + 去重 = 6 个
        assert_eq!(words.len(), 6);
        assert!(words.contains(&"Claude".to_string()));
        assert!(words.contains(&"API".to_string()));
        assert!(words.contains(&"克劳德".to_string()));
    }

    #[test]
    fn build_request_without_hotwords_omits_corpus() {
        let bytes = build_client_request("uid-1", &[]);
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["request"]["corpus"].is_null());
    }

    #[test]
    fn build_request_with_hotwords_stringifies_context() {
        // corpus.context 必须是可 unmarshal 的 JSON 字符串(豆包协议),
        // 写纯文本会触发 server 端报 "fail to unmarshal corpusCtx"。
        let hw = vec![
            "Claude".to_string(),
            "GitHub".to_string(),
            "克劳德".to_string(),
        ];
        let bytes = build_client_request("uid-1", &hw);
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let ctx = v["request"]["corpus"]["context"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(ctx).unwrap();
        assert_eq!(parsed["hotwords"][0]["word"], "Claude");
        assert_eq!(parsed["hotwords"][1]["word"], "GitHub");
        assert_eq!(parsed["hotwords"][2]["word"], "克劳德");
    }
}
