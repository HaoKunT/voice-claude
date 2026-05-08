//! 智谱 GLM-ASR（HTTP 批处理，支持超 30s 自动分段）。
//! 对应 Go 版的 asr.go。

use crate::asr::wav::split_wav;
use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::Duration;

const ZHIPU_URL: &str = "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions";
const MAX_SEGMENT_SECONDS: f64 = 28.0;

#[derive(Deserialize)]
struct ZhipuResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

pub async fn transcribe(cfg: &Config, wav: &[u8]) -> Result<String> {
    if cfg.asr_api_key.is_empty() {
        anyhow::bail!("请配置智谱 API Key");
    }
    let segments = split_wav(wav, MAX_SEGMENT_SECONDS)?;
    if segments.is_empty() {
        return Ok(String::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let mut parts = Vec::with_capacity(segments.len());
    for (i, seg) in segments.into_iter().enumerate() {
        let text = transcribe_segment(&client, &cfg.asr_api_key, seg)
            .await
            .with_context(|| format!("分段 {}", i + 1))?;
        parts.push(text);
    }
    Ok(parts.concat())
}

async fn transcribe_segment(
    client: &reqwest::Client,
    api_key: &str,
    wav: Vec<u8>,
) -> Result<String> {
    let file_part = Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let form = Form::new().text("model", "glm-asr").part("file", file_part);

    let resp = client
        .post(ZHIPU_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("zhipu API {}: {}", status, body);
    }
    let parsed: ZhipuResponse =
        serde_json::from_str(&body).with_context(|| format!("parse response: {}", body))?;
    if let Some(e) = parsed.error {
        anyhow::bail!("zhipu error: {}", e);
    }
    Ok(parsed.text)
}
