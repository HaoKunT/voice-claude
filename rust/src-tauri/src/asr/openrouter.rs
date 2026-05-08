//! OpenRouter Whisper（HTTP 批处理）。
//! 对应 Go 版的 openrouter_asr.go。

use crate::config::Config;
use anyhow::Result;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::Duration;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/audio/transcriptions";

#[derive(Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
}

pub async fn transcribe(cfg: &Config, wav: &[u8]) -> Result<String> {
    if cfg.openrouter_api_key.is_empty() {
        anyhow::bail!("请配置 OpenRouter API Key");
    }

    let file_part = Part::bytes(wav.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let form = Form::new()
        .text("model", "openai/whisper-large-v3-turbo")
        .part("file", file_part);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let resp = client
        .post(OPENROUTER_URL)
        .bearer_auth(&cfg.openrouter_api_key)
        .multipart(form)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("openrouter API {}: {}", status, body);
    }
    let parsed: WhisperResponse = serde_json::from_str(&body)?;
    Ok(parsed.text)
}
