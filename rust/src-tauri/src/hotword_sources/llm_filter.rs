//! LLM 二次筛选 —— 把本地频率统计出的候选词列表喂给用户配置的 polish profile
//! 后端,让 LLM 过滤掉垃圾词(普通缩写、编程关键字等),只保留真正适合做
//! 语音识别词典的术语(专有名词、项目名、人名等)。
//!
//! **架构 note**:这里 inline 复制了 `correct.rs` 的 LLM 调用逻辑。原因是
//! 这次只 tap polish profile 的后端(url/model/api_key/mode)做"自由 prompt"
//! 调用,跟 polish 流程的 prompt 渲染语义不同。等用户决定把 LLM 后端配置
//! 抽成独立模块(对话里提过)时,把这里和 correct.rs 一起重构,共享 LLM
//! 调用层。
//!
//! 工作量上看 ~80 行复制代码,跟"refactor correct.rs 影响刚发的 v0.3.2"
//! 比,选了 isolation 优先。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::{
    PolishProfile, POLISH_MODE_CLOUD, POLISH_MODE_OFF, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER,
};

use super::analyze::Candidate;

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_CANDIDATES_TO_SEND: usize = 200;

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
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    #[serde(default)]
    response: String,
}

/// 调用 LLM 二次筛选候选词。返回 LLM 推荐保留的词列表(可能是 candidates 的
/// 子集,也可能 LLM 加了一些它觉得相关但没在频率 top 里的词 —— 我们都接受,
/// 最终用户在 modal 里勾选时还能再过)。
pub async fn filter_candidates(
    candidates: &[Candidate],
    profile: &PolishProfile,
    timeout_secs: u64,
) -> Result<Vec<String>> {
    if profile.mode == POLISH_MODE_OFF || profile.mode.is_empty() {
        anyhow::bail!(
            "profile 「{}」的模式是 off,LLM 二次筛选需要一个有效的 polish profile",
            profile.name
        );
    }

    let prompt = build_filter_prompt(candidates);
    let response = match profile.mode.as_str() {
        POLISH_MODE_OPENROUTER => {
            call_openai_compatible(
                &prompt,
                profile,
                "https://openrouter.ai/api/v1/chat/completions",
                timeout_secs,
            )
            .await?
        }
        POLISH_MODE_CLOUD => call_cloud(&prompt, profile, timeout_secs).await?,
        POLISH_MODE_OLLAMA => call_ollama(&prompt, profile, timeout_secs).await?,
        other => anyhow::bail!("不支持的 polish mode: {}", other),
    };

    Ok(parse_word_list(&response))
}

/// 构造给 LLM 的提示词,要求它返回 JSON array of strings。
/// 候选词附带频率,LLM 能据此判断重要性。
fn build_filter_prompt(candidates: &[Candidate]) -> String {
    let mut s = String::from(
        "你是语音识别词典审核员。\n\
         下面是从用户对话历史中按词频提取出的候选词,请筛选哪些适合加入\
         语音识别词典(boosting 用)。\n\n\
         加入标准:\n\
         - 专有名词、项目名、公司名、人名(如 Claude / voice-claude / Anthropic / FireRedASR)\n\
         - 易被语音识别错的英文术语 / 技术名词(如 CGEventTap / sherpa-onnx)\n\
         - 用户高频提到的领域术语\n\n\
         排除标准:\n\
         - 普通英文单词(如 about / problem / system,即便 freq 很高)\n\
         - 编程通用关键字(function / return / class)\n\
         - 缩写歧义大的(API / SDK,这种容易误识别但词典放进去帮助有限)\n\
         - 单字符 / 双字符\n\n\
         候选词(按出现次数排序):\n",
    );
    for c in candidates.iter().take(MAX_CANDIDATES_TO_SEND) {
        s.push_str(&format!("- {} (freq={})\n", c.word, c.freq));
    }
    s.push_str(
        "\n请输出 JSON array of strings,**只**输出 JSON,不要任何解释 / markdown 包装。\n\
         例:[\"voice-claude\", \"Claude\", \"sherpa-onnx\"]\n",
    );
    s
}

/// 从 LLM 响应里提取 JSON array of strings。LLM 偶尔会在前后加 markdown
/// fence (```json ... ```),宽容解析。如果完全 parse 失败,降级为按行 / 逗号
/// 分割提取看起来像词的 token。
fn parse_word_list(response: &str) -> Vec<String> {
    // 找第一个 [ 和最后一个 ],尝试 JSON 解析
    let start = response.find('[');
    let end = response.rfind(']');
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            let candidate_json = &response[s..=e];
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(candidate_json) {
                return arr
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    // 降级:每行一个词,过滤空 / 太长
    response
        .lines()
        .map(|l| {
            l.trim()
                .trim_matches(|c: char| c == '-' || c == '"' || c == ',')
                .trim()
        })
        .filter(|l| !l.is_empty() && l.len() <= 60 && !l.contains(' '))
        .map(|l| l.to_string())
        .collect()
}

// === 以下是 LLM 调用,跟 correct.rs 的实现一致(临时复制,等 LLM 后端
// 配置抽离重构时合并) ===

async fn call_openai_compatible(
    user: &str,
    profile: &PolishProfile,
    url: &str,
    timeout_secs: u64,
) -> Result<String> {
    if profile.api_key.is_empty() {
        anyhow::bail!("profile 「{}」缺少 API Key", profile.name);
    }
    let body = OpenAIRequest {
        model: &profile.model,
        messages: vec![OpenAIMessage {
            role: "user",
            content: user,
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
    parse_openai_response(resp).await
}

async fn call_cloud(user: &str, profile: &PolishProfile, timeout_secs: u64) -> Result<String> {
    let url = if profile.url.ends_with("/chat/completions") {
        profile.url.clone()
    } else {
        format!("{}/v1/chat/completions", profile.url.trim_end_matches('/'))
    };
    let body = OpenAIRequest {
        model: &profile.model,
        messages: vec![OpenAIMessage {
            role: "user",
            content: user,
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
    parse_openai_response(resp).await
}

async fn parse_openai_response(resp: reqwest::Response) -> Result<String> {
    let status = resp.status();
    let text_body = truncate_body(resp.text().await.unwrap_or_default());
    if !status.is_success() {
        anyhow::bail!("LLM API {}: {}", status, text_body);
    }
    let parsed: OpenAIResponse = serde_json::from_str(&text_body)?;
    Ok(parsed
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_default())
}

async fn call_ollama(user: &str, profile: &PolishProfile, timeout_secs: u64) -> Result<String> {
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

fn truncate_body(s: String) -> String {
    if s.len() <= MAX_BODY_BYTES {
        s
    } else {
        let mut t = s.into_bytes();
        t.truncate(MAX_BODY_BYTES);
        let truncated = String::from_utf8_lossy(&t).to_string();
        format!("{}... (truncated)", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pure_json_array() {
        let r = parse_word_list("[\"foo\", \"bar\", \"baz\"]");
        assert_eq!(r, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn parse_with_markdown_fence() {
        let r = parse_word_list("```json\n[\"foo\", \"bar\"]\n```");
        assert_eq!(r, vec!["foo", "bar"]);
    }

    #[test]
    fn parse_with_explanation_around() {
        let r = parse_word_list("好的,以下是筛选结果:\n[\"foo\", \"bar\"]\n希望对你有帮助");
        assert_eq!(r, vec!["foo", "bar"]);
    }

    #[test]
    fn parse_fallback_line_format() {
        // LLM 不听话直接列每行
        let r = parse_word_list("- foo\n- bar\nbaz");
        assert_eq!(r, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn build_prompt_includes_candidates() {
        let cands = vec![
            Candidate {
                word: "voice-claude".into(),
                freq: 50,
            },
            Candidate {
                word: "FireRedASR".into(),
                freq: 30,
            },
        ];
        let p = build_filter_prompt(&cands);
        assert!(p.contains("voice-claude"));
        assert!(p.contains("freq=50"));
        assert!(p.contains("JSON array"));
    }
}
