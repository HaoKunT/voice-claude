//! 公共 LLM 调用层:按 LlmBackend.mode 分派到 ollama / openai-compat / cloud。
//!
//! 0.4 起从 `correct.rs` 和 `hotword_sources/llm_filter.rs` 抽出来 —— 两个 feature
//! 都在调 LLM,共享后端连接配置(url/model/api_key/mode)但各自的 prompt。
//!
//! 接口故意做窄:`call(backend, prompt, timeout) -> String`,不暴露 system /
//! messages 数组结构。所有 prompt 逻辑(glossary 注入、占位符替换)都在 caller
//! 完成,这里只负责 HTTP 请求 + 响应解析。

use crate::config::{
    LlmBackend, POLISH_MODE_CLOUD, POLISH_MODE_OFF, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_BODY_BYTES: usize = 64 * 1024;
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

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

/// 按 backend.mode 分发调用,返回 LLM 输出的纯文本(已 trim)。
///
/// `prompt` 是完整渲染后的 user message 内容,caller 负责所有占位符替换 / 词典
/// 注入。空 prompt 也会发出去(让 backend 报错而不是悄悄返回 fallback)。
pub async fn call(backend: &LlmBackend, prompt: &str, timeout_secs: u64) -> Result<String> {
    match backend.mode.as_str() {
        POLISH_MODE_OFF | "" => {
            anyhow::bail!("backend 「{}」mode = off,不应在此被调用", backend.name)
        }
        POLISH_MODE_OLLAMA => call_ollama(backend, prompt, timeout_secs).await,
        POLISH_MODE_OPENROUTER => {
            call_openai_compatible(backend, OPENROUTER_URL, prompt, timeout_secs, true).await
        }
        POLISH_MODE_CLOUD => call_cloud(backend, prompt, timeout_secs).await,
        other => anyhow::bail!("backend 「{}」mode = {} 未支持", backend.name, other),
    }
}

async fn call_openai_compatible(
    backend: &LlmBackend,
    url: &str,
    prompt: &str,
    timeout_secs: u64,
    require_api_key: bool,
) -> Result<String> {
    if require_api_key && backend.api_key.is_empty() {
        anyhow::bail!("backend 「{}」缺少 API Key", backend.name);
    }
    let body = OpenAIRequest {
        model: &backend.model,
        messages: vec![OpenAIMessage {
            role: "user",
            content: prompt,
        }],
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;
    let mut req = client.post(url).json(&body);
    if !backend.api_key.is_empty() {
        req = req.bearer_auth(&backend.api_key);
    }
    let resp = req.send().await.context("call openai compatible")?;
    parse_openai_response(resp).await
}

async fn call_cloud(backend: &LlmBackend, prompt: &str, timeout_secs: u64) -> Result<String> {
    let url = if backend.url.ends_with("/chat/completions") {
        backend.url.clone()
    } else {
        format!("{}/v1/chat/completions", backend.url.trim_end_matches('/'))
    };
    call_openai_compatible(backend, &url, prompt, timeout_secs, false).await
}

async fn call_ollama(backend: &LlmBackend, prompt: &str, timeout_secs: u64) -> Result<String> {
    let req = OllamaRequest {
        model: &backend.model,
        prompt: prompt.to_string(),
        stream: false,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;
    let resp = client
        .post(&backend.url)
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

async fn parse_openai_response(resp: reqwest::Response) -> Result<String> {
    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("API {}: {}", status, text_body);
    }
    let parsed: OpenAIResponse = serde_json::from_str(&text_body)
        .with_context(|| format!("parse openai response: {}", text_body))?;
    Ok(parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_default())
}

fn truncate_body(s: String) -> String {
    if s.len() > MAX_BODY_BYTES {
        let mut t = s.into_bytes();
        t.truncate(MAX_BODY_BYTES);
        format!("{} ...(truncated)", String::from_utf8_lossy(&t))
    } else {
        s
    }
}

/// 检查 Ollama 是否在运行。url 空时用默认地址。
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
