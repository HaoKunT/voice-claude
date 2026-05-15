//! 豆包/火山引擎 SAUC ASR（WebSocket 流式，自定义二进制帧）。
//! 对应 Go 版的 volc_asr.go。

use crate::config::Config;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::handshake::client::Request as HandshakeRequest;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::Message;

/// 豆包双向流式热词直传上限:官方文档标称 100 tokens,保守按词数切;
/// 每个中文词约 1-2 tokens,英文专有名词也多在 1-2 tokens 之内,取 50 个词。
const MAX_INJECT_HOTWORDS: usize = 50;

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
/// key(用户自己记的错音)和 value(正确写法)都注入,去空去重后按字典序排列,
/// 超过 MAX_INJECT_HOTWORDS 截断(豆包 100 tokens 上限)。
fn collect_hotwords(hotwords: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for (k, v) in hotwords {
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() {
            set.insert(k.to_string());
        }
        if !v.is_empty() {
            set.insert(v.to_string());
        }
    }
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
        // 豆包 API 约定:corpus.context 的值是一个 JSON 字符串(内部 JSON 序列化)
        let ctx_inner = json!({
            "hotwords": hotwords.iter().map(|w| json!({ "word": w })).collect::<Vec<_>>(),
        });
        request["corpus"] = json!({ "context": ctx_inner.to_string() });
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
    serde_json::to_vec(&payload).unwrap_or_default()
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
        .send(Message::Binary(init_msg))
        .await
        .context("发送初始化请求失败")?;

    let _ = ready.send(());

    let recv_task = tokio::spawn(async move {
        let mut final_text = String::new();
        while let Some(msg) = read.next().await {
            let bytes = match msg {
                Ok(Message::Binary(b)) => b,
                Ok(Message::Text(t)) => t.into_bytes(),
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
            if write.send(Message::Binary(msg)).await.is_err() {
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
        let _ = write.send(Message::Binary(end)).await;
        write
    });

    let _ = send_task.await;

    let final_text = tokio::time::timeout(Duration::from_secs(10), recv_task)
        .await
        .map(|r| r.unwrap_or_default())
        .unwrap_or_default();
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
        let mut m = std::collections::HashMap::new();
        m.insert("克劳德".to_string(), "Claude".to_string());
        m.insert("吉他布".to_string(), "GitHub".to_string());
        m.insert("  ".to_string(), "API".to_string()); // 空 key 忽略,value "API" 仍加
        m.insert("艾皮爱".to_string(), "API".to_string()); // key 新增,value "API" 重复去重
        let words = collect_hotwords(&m);
        // key+value 合集去空去重:克劳德 Claude 吉他布 GitHub API 艾皮爱 = 6 个
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
        let hw = vec!["Claude".to_string(), "GitHub".to_string()];
        let bytes = build_client_request("uid-1", &hw);
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // corpus.context 必须是字符串而不是嵌套对象,API 特殊约定
        let ctx = v["request"]["corpus"]["context"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(ctx).unwrap();
        assert_eq!(parsed["hotwords"][0]["word"], "Claude");
        assert_eq!(parsed["hotwords"][1]["word"], "GitHub");
    }
}
