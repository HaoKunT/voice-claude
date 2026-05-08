//! 豆包/火山引擎 SAUC ASR（WebSocket 流式，自定义二进制帧）。
//! 对应 Go 版的 volc_asr.go。

use crate::config::Config;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::handshake::client::Request as HandshakeRequest;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::Message;

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

fn build_client_request(uid: &str) -> Vec<u8> {
    let payload = json!({
        "user": { "uid": uid },
        "audio": {
            "format": "pcm",
            "codec": "raw",
            "rate": 16000,
            "bits": 16,
            "channel": 1,
        },
        "request": {
            "model_name": "bigmodel",
            "enable_punc": true,
            "enable_ddc": true,
            "enable_nonstream": true,
            "show_utterances": true,
            "result_type": "full",
            "end_window_size": 3000,
        },
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
        anyhow::bail!("响应数据过短");
    }
    let msg_type = (data[1] >> 4) & 0x0F;
    let flags = data[1] & 0x0F;
    if msg_type == MSG_SERVER_ERROR {
        anyhow::bail!("服务端错误");
    }
    let payload_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if data.len() < 8 + payload_size {
        anyhow::bail!("payload 不完整");
    }
    let payload = &data[8..8 + payload_size];
    let parsed: VolcPayload = serde_json::from_slice(payload)?;
    Ok((parsed.result.text, flags == FLAG_ASYNC_FINAL))
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
    let connect_id = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string();
    let req = HandshakeRequest::builder()
        .uri(&uri)
        .header("Host", uri.host().unwrap_or(""))
        .header("X-Api-App-Key", &cfg.volc_app_key)
        .header("X-Api-Access-Key", &cfg.volc_access_token)
        .header("X-Api-Resource-Id", &cfg.volc_resource_id)
        .header("X-Api-Connect-Id", &connect_id)
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .body(())?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(req).await.context("豆包连接失败")?;
    let (mut write, mut read) = ws_stream.split();

    // 发送初始化请求
    let uid = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string();
    let init_payload = build_client_request(&uid);
    let init_msg = encode_message(MSG_FULL_CLIENT_REQUEST, FLAG_NO_SEQUENCE, SER_JSON, COMP_NONE, &init_payload);
    write.send(Message::Binary(init_msg)).await.context("发送初始化请求失败")?;

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
            let msg = encode_message(MSG_AUDIO_ONLY_REQUEST, FLAG_NO_SEQUENCE, SER_NONE, COMP_NONE, &chunk);
            if write.send(Message::Binary(msg)).await.is_err() {
                return write;
            }
        }
        // 发送结束帧
        let end = encode_message(MSG_AUDIO_ONLY_REQUEST, FLAG_LAST_PACKET, SER_NONE, COMP_NONE, &[]);
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
        let h = encode_header(MSG_FULL_CLIENT_REQUEST, FLAG_NO_SEQUENCE, SER_JSON, COMP_NONE);
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
        let payload = br#"{"result":{"text":"你好"}}"#;
        let msg = encode_message(0x09, FLAG_ASYNC_FINAL, SER_JSON, COMP_NONE, payload);
        let (text, is_final) = decode_response(&msg).unwrap();
        assert_eq!(text, "你好");
        assert!(is_final);
    }
}
