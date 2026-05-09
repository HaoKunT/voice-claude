//! AI 润色：按当前活跃 profile 对 ASR 识别文本做后处理。
//! 0.1.2 起接入多 profile，每个 profile 是一套完整的 (mode, url, model, api_key, prompt)。

use crate::config::{
    PolishProfile, POLISH_MODE_CLOUD, POLISH_MODE_OFF, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

/// 主入口：按 profile.mode 分派。
/// profile.prompt 里的 `{text}` 会被替换为识别原文；没有占位符时把原文 append 到末尾。
pub async fn correct(text: &str, profile: &PolishProfile, timeout_secs: u64) -> Result<String> {
    match profile.mode.as_str() {
        POLISH_MODE_OFF | "" => Ok(text.to_string()),
        POLISH_MODE_OLLAMA => call_ollama(text, profile, timeout_secs).await,
        POLISH_MODE_OPENROUTER => {
            call_openai_compatible(
                text,
                profile,
                "https://openrouter.ai/api/v1/chat/completions",
                timeout_secs,
            )
            .await
        }
        POLISH_MODE_CLOUD => call_cloud(text, profile, timeout_secs).await,
        _ => Ok(text.to_string()),
    }
}

/// 把 prompt 模板里的 {text} 占位符替换为实际文本；若模板里没占位符，把原文追加到末尾。
fn render_prompt(profile: &PolishProfile, text: &str) -> (String, String) {
    // 返回 (system, user)：我们把整段 prompt（替换后）作为 user，system 留空 —— 简化且更可控
    // 也可以拆分 system / user，但多数用户不关心，放一起更直白
    let body = if profile.prompt.contains("{text}") {
        profile.prompt.replace("{text}", text)
    } else {
        format!("{}\n\n原文：{}", profile.prompt, text)
    };
    (String::new(), body)
}

async fn call_openai_compatible(
    text: &str,
    profile: &PolishProfile,
    url: &str,
    timeout_secs: u64,
) -> Result<String> {
    if profile.api_key.is_empty() {
        anyhow::bail!("profile 「{}」缺少 API Key", profile.name);
    }
    let (_system, user) = render_prompt(profile, text);
    let body = OpenAIRequest {
        model: &profile.model,
        messages: vec![OpenAIMessage {
            role: "user",
            content: &user,
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;
    let resp = client
        .post(url)
        .bearer_auth(&profile.api_key)
        .json(&body)
        .send()
        .await
        .context("call openai compatible")?;
    parse_openai_response(resp, text).await
}

async fn call_cloud(text: &str, profile: &PolishProfile, timeout_secs: u64) -> Result<String> {
    let url = if profile.url.ends_with("/chat/completions") {
        profile.url.clone()
    } else {
        format!("{}/v1/chat/completions", profile.url.trim_end_matches('/'))
    };
    let (_system, user) = render_prompt(profile, text);
    let body = OpenAIRequest {
        model: &profile.model,
        messages: vec![OpenAIMessage {
            role: "user",
            content: &user,
        }],
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;
    let mut req = client.post(&url).json(&body);
    if !profile.api_key.is_empty() {
        req = req.bearer_auth(&profile.api_key);
    }
    let resp = req.send().await.context("call cloud endpoint")?;
    parse_openai_response(resp, text).await
}

async fn parse_openai_response(resp: reqwest::Response, fallback: &str) -> Result<String> {
    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("API {}: {}", status, text_body);
    }
    let parsed: OpenAIResponse = serde_json::from_str(&text_body)?;
    Ok(parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_else(|| fallback.to_string()))
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

async fn call_ollama(text: &str, profile: &PolishProfile, timeout_secs: u64) -> Result<String> {
    let (_system, user) = render_prompt(profile, text);
    let req = OllamaRequest {
        model: &profile.model,
        prompt: user,
        stream: false,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;
    let resp = client
        .post(&profile.url)
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

    fn off_profile() -> PolishProfile {
        let mut p = PolishProfile::default_named("test", "test");
        p.mode = "off".into();
        p
    }

    #[tokio::test]
    async fn off_mode_returns_original() {
        assert_eq!(correct("hello", &off_profile(), 10).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn empty_mode_returns_original() {
        let mut p = off_profile();
        p.mode = String::new();
        assert_eq!(correct("hello", &p, 10).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn unknown_mode_returns_original() {
        let mut p = off_profile();
        p.mode = "unknown".into();
        assert_eq!(correct("hello", &p, 10).await.unwrap(), "hello");
    }

    #[test]
    fn render_prompt_substitutes_placeholder() {
        let mut p = off_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello");
        assert_eq!(user, "润色：hello");
    }

    #[test]
    fn render_prompt_appends_when_no_placeholder() {
        let mut p = off_profile();
        p.prompt = "请润色".into();
        let (_, user) = render_prompt(&p, "hello");
        assert!(user.contains("请润色"));
        assert!(user.contains("原文：hello"));
    }
}
