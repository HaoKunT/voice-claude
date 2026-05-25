//! AI 润色：渲染 prompt(占位符 + glossary 注入),调 llm_client 拿结果。
//!
//! 0.4 起 LLM 后端连接(mode/url/model/api_key)抽到 `LlmBackend`,本模块只负责
//! profile 维度的逻辑(prompt 模板 + glossary 段落)。

use crate::config::{LlmBackend, PolishProfile};
use crate::llm_client;
use crate::profile_templates::effective_prompt;
use anyhow::Result;

/// 主入口:按 backend.mode 分派(off → 直接返回原文)。
///
/// - `text`:ASR 识别原文
/// - `profile`:活跃 polish profile(prompt 来源)
/// - `backend`:profile 引用的 LLM 后端连接
/// - `timeout_secs`:HTTP 超时
/// - `glossary`:`cfg.hotwords` 关键词列表 —— 渲染成"识别词典"段落注入 prompt,
///   给 LLM 跨语种 / 写法映射的上下文(原 ASR 后字符串替换路径已删除,
///   词典统一从这里喂给 LLM)。
///
/// 返回:LLM 输出的纯文本(已 trim);任何环节出错返回 `Err`,**不**做静默 fallback。
/// 调用方(recorder.rs)按需把 Err fallback 到原文 + 记 polish_timeout 统计。
///
/// prompt 渲染规则:`profile.prompt` 含 `{text}`/`{glossary}` 占位符就替换;
/// 没占位符时按"词典 → prompt → 原文"的默认布局拼接。
pub async fn correct(
    text: &str,
    profile: &PolishProfile,
    backend: &LlmBackend,
    timeout_secs: u64,
    glossary: &[String],
) -> Result<String> {
    if !backend.is_active() {
        return Ok(text.to_string());
    }
    let (_system, user) = render_prompt(profile, text, glossary);
    llm_client::call(backend, &user, timeout_secs).await
}

/// 渲染 prompt:`{text}` 占位符替换识别原文;`{glossary}` 占位符替换词典段落。
/// 缺占位符时:原文 → 末尾追加"原文:..."块;词典(若有)→ 在原文之前追加。
fn render_prompt(profile: &PolishProfile, text: &str, glossary: &[String]) -> (String, String) {
    // glossary 段落:仅当词典非空时注入
    let glossary_block = format_glossary(glossary);
    let prompt = effective_prompt(profile);

    let has_text_ph = prompt.contains("{text}");
    let has_glossary_ph = prompt.contains("{glossary}");
    let body = match (has_text_ph, has_glossary_ph) {
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
    let glossary_words = glossary.iter().filter(|s| !s.trim().is_empty()).count();
    let glossary_injected = !glossary_block.is_empty() && body.contains(&glossary_block);
    tracing::debug!(
        glossary_words,
        glossary_injected,
        glossary_chars = glossary_block.chars().count(),
        has_text_ph,
        has_glossary_ph,
        body_chars = body.chars().count(),
        "润色 prompt 渲染完成"
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::POLISH_MODE_OFF;

    fn test_profile() -> PolishProfile {
        PolishProfile::default_named("test", "test")
    }

    fn off_backend() -> LlmBackend {
        let mut b = LlmBackend::default_named("test", "test");
        b.mode = POLISH_MODE_OFF.into();
        b
    }

    #[tokio::test]
    async fn off_mode_returns_original() {
        assert_eq!(
            correct("hello", &test_profile(), &off_backend(), 10, &[])
                .await
                .unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn empty_mode_returns_original() {
        let mut b = off_backend();
        b.mode = String::new();
        assert_eq!(
            correct("hello", &test_profile(), &b, 10, &[])
                .await
                .unwrap(),
            "hello"
        );
    }

    #[test]
    fn render_prompt_substitutes_placeholder() {
        let mut p = test_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert_eq!(user, "润色：hello");
    }

    #[test]
    fn render_prompt_appends_when_no_placeholder() {
        let mut p = test_profile();
        p.prompt = "请润色".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert!(user.contains("请润色"));
        assert!(user.contains("原文：hello"));
    }

    #[test]
    fn render_prompt_substitutes_glossary_placeholder() {
        let mut p = test_profile();
        p.prompt = "{glossary}\n\n润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into(), "voice-claude".into()]);
        assert!(user.contains("识别词典"));
        assert!(user.contains("- Claude"));
        assert!(user.contains("- voice-claude"));
        assert!(user.contains("润色：hello"));
    }

    #[test]
    fn render_prompt_prepends_glossary_when_only_text_placeholder() {
        let mut p = test_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into()]);
        assert!(user.contains("识别词典"));
        assert!(user.find("识别词典").unwrap() < user.find("润色：").unwrap());
        assert!(user.contains("润色：hello"));
    }

    #[test]
    fn render_prompt_no_glossary_block_when_empty() {
        let mut p = test_profile();
        p.prompt = "润色：{text}".into();
        let (_, user) = render_prompt(&p, "hello", &[]);
        assert!(!user.contains("识别词典"));
    }

    #[test]
    fn render_prompt_default_template_layout() {
        let mut p = test_profile();
        p.prompt = "请润色".into();
        let (_, user) = render_prompt(&p, "hello", &["Claude".into()]);
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
        assert!(!s.contains("- \n"));
    }

    #[test]
    fn format_glossary_empty_input_returns_empty() {
        assert!(format_glossary(&[]).is_empty());
        assert!(format_glossary(&["".into(), "  ".into()]).is_empty());
    }
}
