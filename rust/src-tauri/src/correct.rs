//! AI 润色：按当前活跃 profile 对 ASR 识别文本做后处理。
//! 0.1.2 起接入多 profile，每个 profile 是一套完整的 (mode, url, model, api_key, prompt)。

use crate::config::{
    PolishProfile, POLISH_MODE_CLOUD, POLISH_MODE_OFF, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER,
};
use crate::profile_templates::effective_prompt;
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

/// 主入口:按 profile.mode 分派。
///
/// - `text`:ASR 识别原文
/// - `profile`:活跃 polish profile
/// - `timeout_secs`:HTTP 超时
/// - `glossary`:`cfg.hotwords` 关键词列表 —— 渲染成"识别词典"段落注入 prompt,
///   给 LLM 跨语种 / 写法映射的上下文(原 ASR 后字符串替换路径已删除,
///   现在两条路:① ASR boosting 让识别准 ② LLM 后处理做语义级映射)。
///   profile.prompt 含 `{glossary}` 占位符就替换;否则缺省追加到末尾(没词典时跳过)。
pub async fn correct(
    text: &str,
    profile: &PolishProfile,
    timeout_secs: u64,
    glossary: &[String],
) -> Result<String> {
    match profile.mode.as_str() {
        POLISH_MODE_OFF | "" => Ok(text.to_string()),
        POLISH_MODE_OLLAMA => call_ollama(text, profile, timeout_secs, glossary).await,
        POLISH_MODE_OPENROUTER => {
            call_openai_compatible(
                text,
                profile,
                "https://openrouter.ai/api/v1/chat/completions",
                timeout_secs,
                glossary,
            )
            .await
        }
        POLISH_MODE_CLOUD => call_cloud(text, profile, timeout_secs, glossary).await,
        _ => Ok(text.to_string()),
    }
}

/// 渲染 prompt:`{text}` 占位符替换识别原文;`{glossary}` 占位符替换词典段落。
/// 缺占位符时:原文 → 末尾追加"原文:..."块;词典(若有)→ 在原文之前追加。
fn render_prompt(profile: &PolishProfile, text: &str, glossary: &[String]) -> (String, String) {
    // glossary 段落:仅当词典非空时注入
    let glossary_block = format_glossary(glossary);
    let prompt = effective_prompt(profile);

    let body = match (prompt.contains("{text}"), prompt.contains("{glossary}")) {
        (true, true) => prompt
            .replace("{glossary}", &glossary_block)
            .replace("{text}", text),
        (true, false) => {
            // prompt 含 {text} 但无 {glossary}:词典(若有)拼到 prompt 之前
            let filled = prompt.replace("{text}", text);
            if glossary_block.is_empty() {
                filled
            } else {
                format!("{}\n\n{}", glossary_block, filled)
            }
        }
        (false, true) => {
            // prompt 含 {glossary} 但无 {text}:原文追加到末尾
            let filled = prompt.replace("{glossary}", &glossary_block);
            format!("{}\n\n原文：{}", filled, text)
        }
        (false, false) => {
            // 默认 prompt 模板:词典(若有) → prompt → 原文
            let mut s = String::new();
            if !glossary_block.is_empty() {
                s.push_str(&glossary_block);
                s.push_str("\n\n");
            }
            s.push_str(prompt);
            s.push_str("\n\n原文：");
            s.push_str(text);
            s
        }
    };
    (String::new(), body)
}

/// 把关键词列表渲染成 LLM 友好的段落。空列表返回空串(调用方据此跳过注入)。
fn format_glossary(glossary: &[String]) -> String {
    let words: Vec<&str> = glossary
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if words.is_empty() {
        return String::new();
    }
    let mut s = String::from("识别词典(请按这里给出的写法 / 拼写校正原文中相应词汇):\n");
    for w in &words {
        s.push_str("- ");
        s.push_str(w);
        s.push('\n');
    }
    s.pop(); // 去掉末尾换行
    s
}

async fn call_openai_compatible(
    text: &str,
    profile: &PolishProfile,
    url: &str,
    timeout_secs: u64,
    glossary: &[String],
) -> Result<String> {
    if profile.api_key.is_empty() {
        anyhow::bail!("profile 「{}」缺少 API Key", profile.name);
    }
    let (_system, user) = render_prompt(profile, text, glossary);
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

async fn call_cloud(
    text: &str,
    profile: &PolishProfile,
    timeout_secs: u64,
    glossary: &[String],
) -> Result<String> {
    let url = if profile.url.ends_with("/chat/completions") {
        profile.url.clone()
    } else {
        format!("{}/v1/chat/completions", profile.url.trim_end_matches('/'))
    };
    let (_system, user) = render_prompt(profile, text, glossary);
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

async fn call_ollama(
    text: &str,
    profile: &PolishProfile,
    timeout_secs: u64,
    glossary: &[String],
) -> Result<String> {
    let (_system, user) = render_prompt(profile, text, glossary);
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
        assert_eq!(
            correct("hello", &off_profile(), 10, &[]).await.unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn empty_mode_returns_original() {
        let mut p = off_profile();
        p.mode = String::new();
        assert_eq!(correct("hello", &p, 10, &[]).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn unknown_mode_returns_original() {
        let mut p = off_profile();
        p.mode = "unknown".into();
        assert_eq!(correct("hello", &p, 10, &[]).await.unwrap(), "hello");
    }

    #[test]
    fn render_prompt_substitutes_placeholder() {
        let mut p = off_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert_eq!(user, "润色：hello");
    }

    #[test]
    fn render_prompt_appends_when_no_placeholder() {
        let mut p = off_profile();
        p.prompt = "请润色".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert!(user.contains("请润色"));
        assert!(user.contains("原文：hello"));
    }

    #[test]
    fn render_prompt_substitutes_glossary_placeholder() {
        let mut p = off_profile();
        p.prompt = "{glossary}\n\n润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into(), "voice-claude".into()]);
        assert!(user.contains("识别词典"));
        assert!(user.contains("- Claude"));
        assert!(user.contains("- voice-claude"));
        assert!(user.contains("润色：hello"));
    }

    #[test]
    fn render_prompt_prepends_glossary_when_only_text_placeholder() {
        let mut p = off_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into()]);
        // glossary 块在 prompt 之前
        assert!(user.contains("识别词典"));
        assert!(user.find("识别词典").unwrap() < user.find("润色：").unwrap());
        assert!(user.contains("润色：hello"));
    }

    #[test]
    fn render_prompt_no_glossary_block_when_empty() {
        let mut p = off_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert!(!user.contains("识别词典"));
    }

    #[test]
    fn render_prompt_default_template_layout() {
        let mut p = off_profile();
        p.prompt = "请润色".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into()]);
        // 默认顺序:词典 → prompt → 原文
        let glossary_pos = user.find("识别词典").unwrap();
        let prompt_pos = user.find("请润色").unwrap();
        let text_pos = user.find("原文：hello").unwrap();
        assert!(glossary_pos < prompt_pos);
        assert!(prompt_pos < text_pos);
    }

    #[test]
    fn format_glossary_skips_empty_and_whitespace() {
        let g: Vec<String> = vec!["Claude".into(), "  ".into(), "".into(), "voice".into()];
        let s = format_glossary(&g);
        assert!(s.contains("Claude"));
        assert!(s.contains("voice"));
        // 不该出现空 bullet
        assert!(!s.contains("- \n"));
    }

    #[test]
    fn format_glossary_empty_input_returns_empty() {
        assert!(format_glossary(&[]).is_empty());
        assert!(format_glossary(&["".into(), "  ".into()]).is_empty());
    }
}
