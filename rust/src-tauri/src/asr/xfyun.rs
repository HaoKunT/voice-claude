//! 讯飞 LLM-ASR WebSocket 流式识别。
//! 对应 Go 版的 xfyun_asr.go。
//!
//! 注:这个 office-api-ast-dx endpoint 不支持 WebSocket 握手里动态传 hot_words,
//! 讯飞官方约定需要用户到开放平台"自学习"控制台预先注册热词表。所以 config.hotwords
//! 对这个后端仅作"转写后字符串替换"用(见 hotwords.rs 的 apply)。若想让识别阶段
//! 就感知热词,请切换到豆包后端(见 volc.rs 的 corpus.context 实现)。

use crate::config::Config;
use anyhow::{Context, Result};
use base64::Engine;
use chrono::{FixedOffset, Utc};
use futures_util::{SinkExt, StreamExt};
// hmac 0.13 把 `new_from_slice` 移到 KeyInit trait,需要显式 import
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::json;
use sha1::Sha1;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

const XFYUN_URL: &str = "wss://office-api-ast-dx.iflyaisol.com/ast/communicate/v1";

type HmacSha1 = Hmac<Sha1>;

/// 构建鉴权 URL（对应 Go 版 xfyunBuildAuthParams）
pub fn build_auth_url(
    app_id: &str,
    access_key_id: &str,
    access_key_secret: &str,
) -> Result<String> {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let now = Utc::now().with_timezone(&tz);
    let utc_str = now.format("%Y-%m-%dT%H:%M:%S%z").to_string();
    let uuid = now.timestamp_nanos_opt().unwrap_or(0).to_string();

    let mut params: Vec<(&str, &str)> = vec![
        ("accessKeyId", access_key_id),
        ("appId", app_id),
        ("audio_encode", "pcm_s16le"),
        ("lang", "autodialect"),
        ("samplerate", "16000"),
        ("utc", utc_str.as_str()),
        ("uuid", uuid.as_str()),
    ];
    // 按 key 升序
    params.sort_by_key(|(k, _)| *k);

    // base string: url-encoded pairs joined by '&'
    let base = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let mut mac = HmacSha1::new_from_slice(access_key_secret.as_bytes()).context("hmac key")?;
    mac.update(base.as_bytes());
    let sig = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    // 最终 query：所有原始参数 + signature（依 Go 行为：保持原字母序并追加 signature）
    let mut final_params: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    final_params.push(("signature".to_string(), sig));

    let query = final_params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    Ok(format!("{}?{}", XFYUN_URL, query))
}

#[derive(Deserialize)]
struct XfyunMsg {
    #[serde(default)]
    action: String,
    #[serde(default)]
    msg_type: String,
    #[serde(default)]
    res_type: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize, Default)]
struct XfyunResultData {
    #[serde(default, rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    ls: bool,
    #[serde(default)]
    cn: CnWrap,
}

#[derive(Deserialize, Default)]
struct CnWrap {
    #[serde(default)]
    st: StWrap,
}

#[derive(Deserialize, Default)]
struct StWrap {
    #[serde(default, rename = "type")]
    typ: String,
    #[serde(default)]
    rt: Vec<Rt>,
}

#[derive(Deserialize, Default)]
struct Rt {
    #[serde(default)]
    ws: Vec<Ws>,
}

#[derive(Deserialize, Default)]
struct Ws {
    #[serde(default)]
    cw: Vec<Cw>,
}

#[derive(Deserialize, Default)]
struct Cw {
    #[serde(default)]
    w: String,
}

fn extract_text(data: &XfyunResultData) -> String {
    let mut s = String::new();
    for rt in &data.cn.st.rt {
        for ws in &rt.ws {
            for cw in &ws.cw {
                s.push_str(&cw.w);
            }
        }
    }
    s
}

pub async fn transcribe_stream(
    cfg: &Config,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    on_partial: Box<dyn Fn(String) + Send + Sync>,
    ready: oneshot::Sender<()>,
) -> Result<String> {
    if cfg.xfyun_app_id.is_empty()
        || cfg.xfyun_access_key_id.is_empty()
        || cfg.xfyun_access_secret.is_empty()
    {
        anyhow::bail!("请配置讯飞 AppID、AccessKeyID、AccessKeySecret");
    }
    let url = build_auth_url(
        &cfg.xfyun_app_id,
        &cfg.xfyun_access_key_id,
        &cfg.xfyun_access_secret,
    )?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .context("讯飞连接失败")?;
    let (mut write, mut read) = ws_stream.split();
    let _ = ready.send(());

    let mut finals = Vec::<String>::new();
    let mut session_id = String::new();

    // 接收协程：处理识别结果
    let recv_task = tokio::spawn(async move {
        let mut finals_local = Vec::<String>::new();
        let mut session_id_local = String::new();
        while let Some(msg) = read.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(_) => break,
            };
            let data = match msg {
                Message::Text(t) => t.into_bytes(),
                Message::Binary(b) => b,
                Message::Close(_) => break,
                _ => continue,
            };
            let parsed: XfyunMsg = match serde_json::from_slice(&data) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if parsed.action == "error" {
                tracing::error!(desc = %parsed.desc, "讯飞错误");
                break;
            }
            if parsed.msg_type == "result" && parsed.res_type == "asr" {
                let rd: XfyunResultData = serde_json::from_value(parsed.data).unwrap_or_default();
                if !rd.session_id.is_empty() {
                    session_id_local = rd.session_id.clone();
                }
                let text = extract_text(&rd);
                if !text.is_empty() {
                    if rd.cn.st.typ == "0" {
                        finals_local.push(text.clone());
                        tracing::debug!(%text, "讯飞最终结果");
                    } else {
                        tracing::debug!(%text, "讯飞中间结果");
                        on_partial(text);
                    }
                }
                if rd.ls {
                    break;
                }
            }
        }
        (finals_local, session_id_local)
    });

    // 发送协程：按 40ms 每 1280 字节推送
    let send_task = tokio::spawn(async move {
        const CHUNK_SIZE: usize = 1280;
        let mut buf = Vec::<u8>::with_capacity(CHUNK_SIZE * 2);
        while let Some(chunk) = pcm_rx.recv().await {
            buf.extend_from_slice(&chunk);
            while buf.len() >= CHUNK_SIZE {
                let part = buf[..CHUNK_SIZE].to_vec();
                if write.send(Message::Binary(part)).await.is_err() {
                    return write;
                }
                buf.drain(..CHUNK_SIZE);
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        }
        if !buf.is_empty() {
            let _ = write.send(Message::Binary(buf)).await;
        }
        write
    });

    let mut write = send_task.await.context("发送协程 join")?;
    let end_msg = if session_id.is_empty() {
        json!({ "end": true }).to_string()
    } else {
        json!({ "end": true, "sessionId": session_id }).to_string()
    };
    let _ = write.send(Message::Text(end_msg)).await;

    if let Ok(Ok((f, sid))) = tokio::time::timeout(Duration::from_secs(5), recv_task).await {
        finals = f;
        if !sid.is_empty() {
            session_id = sid;
        }
    }
    let _ = session_id; // 避免 unused 警告（某些路径不用）

    Ok(finals.concat())
}
