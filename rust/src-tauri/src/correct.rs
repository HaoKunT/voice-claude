//! AI 纠错：对 ASR 识别文本做后处理。
//! 对应 Go 版的 correct.go。

use crate::config::{
    Config, CORRECT_MODE_CLOUD, CORRECT_MODE_OFF, CORRECT_MODE_OLLAMA, CORRECT_MODE_OPENROUTER,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const CORRECTION_PROMPT: &str =
    "你是一个语音识别纠错助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
如果原文没有明显错误，直接返回原文。
只输出纠正后的文本，不要解释。";

const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
struct OpenAIMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OpenAIRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage<'a>>,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    #[serde(default)]
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    #[serde(default)]
    message: OpenAIMessageOwned,
}

#[derive(Deserialize, Default)]
struct OpenAIMessageOwned {
    #[serde(default)]
    content: String,
}

/// 主入口：按 cfg.correct_mode 分派。
pub async fn correct(text: &str, cfg: &Config) -> Result<String> {
    match cfg.correct_mode.as_str() {
        CORRECT_MODE_OFF | "" => Ok(text.to_string()),
        CORRECT_MODE_OLLAMA => correct_ollama(text, cfg).await,
        CORRECT_MODE_OPENROUTER => correct_openrouter(text, cfg).await,
        CORRECT_MODE_CLOUD => correct_cloud(text, cfg).await,
        _ => Ok(text.to_string()),
    }
}

async fn correct_openrouter(text: &str, cfg: &Config) -> Result<String> {
    if cfg.openrouter_api_key.is_empty() {
        anyhow::bail!("请配置 OpenRouter API Key");
    }
    let model = if cfg.correct_model.is_empty() {
        "qwen/qwen3-8b"
    } else {
        &cfg.correct_model
    };

    let body = OpenAIRequest {
        model,
        messages: vec![
            OpenAIMessage {
                role: "system",
                content: CORRECTION_PROMPT,
            },
            OpenAIMessage {
                role: "user",
                content: text,
            },
        ],
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.correct_timeout_secs()))
        .build()?;
    let resp = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(&cfg.openrouter_api_key)
        .json(&body)
        .send()
        .await
        .context("call openrouter")?;

    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("openrouter API {}: {}", status, text_body);
    }
    let parsed: OpenAIResponse = serde_json::from_str(&text_body)?;
    Ok(parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_else(|| text.to_string()))
}

async fn correct_cloud(text: &str, cfg: &Config) -> Result<String> {
    let url = if cfg.correct_url.ends_with("/chat/completions") {
        cfg.correct_url.clone()
    } else {
        format!(
            "{}/v1/chat/completions",
            cfg.correct_url.trim_end_matches('/')
        )
    };
    let body = OpenAIRequest {
        model: &cfg.correct_model,
        messages: vec![
            OpenAIMessage {
                role: "system",
                content: CORRECTION_PROMPT,
            },
            OpenAIMessage {
                role: "user",
                content: text,
            },
        ],
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.correct_timeout_secs()))
        .build()?;
    let mut req = client.post(&url).json(&body);
    if !cfg.correct_api_key.is_empty() {
        req = req.bearer_auth(&cfg.correct_api_key);
    }
    let resp = req.send().await.context("call cloud endpoint")?;
    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("cloud API {}: {}", status, text_body);
    }
    let parsed: OpenAIResponse = serde_json::from_str(&text_body)?;
    Ok(parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_else(|| text.to_string()))
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    #[serde(default)]
    response: String,
}

async fn correct_ollama(text: &str, cfg: &Config) -> Result<String> {
    let req = OllamaRequest {
        model: &cfg.correct_model,
        prompt: format!("{}\n\n原文：{}", CORRECTION_PROMPT, text),
        stream: false,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.correct_timeout_secs()))
        .build()?;
    let resp = client
        .post(&cfg.correct_url)
        .json(&req)
        .send()
        .await
        .context("call ollama")?;
    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("ollama API {}: {}", status, text_body);
    }
    let parsed: OllamaResponse = serde_json::from_str(&text_body)?;
    Ok(parsed.response.trim().to_string())
}

/// 检查 Ollama 是否在运行。
pub async fn check_ollama(url: &str) -> Result<()> {
    let base = if url.is_empty() {
        "http://localhost:11434"
    } else {
        url
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let resp = client
        .get(format!("{}/api/tags", base.trim_end_matches('/')))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("ollama 异常: HTTP {}", resp.status());
    }
    Ok(())
}

fn truncate_body(s: String) -> String {
    if s.len() > MAX_BODY_BYTES {
        s[..MAX_BODY_BYTES].to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn off_mode_returns_original() {
        let cfg = Config {
            correct_mode: "off".into(),
            ..Config::default()
        };
        assert_eq!(correct("hello", &cfg).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn empty_mode_returns_original() {
        let cfg = Config {
            correct_mode: String::new(),
            ..Config::default()
        };
        assert_eq!(correct("hello", &cfg).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn unknown_mode_returns_original() {
        let cfg = Config {
            correct_mode: "unknown".into(),
            ..Config::default()
        };
        assert_eq!(correct("hello", &cfg).await.unwrap(), "hello");
    }
}
