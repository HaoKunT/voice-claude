//! OpenRouter Whisper(HTTP 批处理)。
//!
//! OpenRouter 的 transcription API 不走 OpenAI 兼容的 multipart form,而是自己
//! 的 JSON 协议:body = `{model, input_audio: {data: base64, format}}`,和 chat
//! completions 里的 input_audio modality 同构。格式写错服务端会把 multipart
//! boundary `--xxx` 当 JSON body 解析,报 `No number after minus sign in JSON`。

use crate::config::Config;
use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/audio/transcriptions";
const DEFAULT_MODEL: &str = "openai/whisper-large-v3-turbo";

#[derive(Serialize)]
struct InputAudio<'a> {
    data: &'a str,
    format: &'a str,
}

#[derive(Serialize)]
struct TranscribeRequest<'a> {
    model: &'a str,
    input_audio: InputAudio<'a>,
    /// 强制 language(ISO-639-1,如 "zh")。Whisper 对气声自动判定不稳定,
    /// 经常把中文气声识别成韩语;空字符串跳过本字段走服务端 auto。
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
}

#[derive(Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
}

pub async fn transcribe(cfg: &Config, wav: &[u8]) -> Result<String> {
    if cfg.openrouter_api_key.is_empty() {
        anyhow::bail!("请配置 OpenRouter API Key");
    }

    let wav_b64 = base64::engine::general_purpose::STANDARD.encode(wav);
    let model = if cfg.openrouter_model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        cfg.openrouter_model.trim()
    };
    let language = {
        let lang = cfg.openrouter_language.trim();
        if lang.is_empty() {
            None
        } else {
            Some(lang)
        }
    };
    let body = TranscribeRequest {
        model,
        input_audio: InputAudio {
            data: &wav_b64,
            format: "wav",
        },
        language,
    };
    tracing::info!(model, language = ?language, "openrouter ASR 请求");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let resp = client
        .post(OPENROUTER_URL)
        .bearer_auth(&cfg.openrouter_api_key)
        .json(&body)
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
