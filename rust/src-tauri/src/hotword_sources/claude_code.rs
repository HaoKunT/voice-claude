//! Claude Code 历史 source。读 `~/.claude/projects/<encoded-cwd>/<session>.jsonl`
//! 的 `type=="user"` 行,把 message.content 拼起来作为用户文本。
//!
//! jsonl 行结构(实测):
//! ```
//! {"type":"user","message":{"role":"user","content":"<text>"},
//!  "timestamp":"2026-05-12T02:34:45.183Z","cwd":"...", ...}
//! ```
//! `message.content` 可能是字符串,也可能是 array(用户上传图片 / file 时
//! Anthropic API 格式),解析时两种都兼容。
//!
//! 隐私:只读用户自己编辑过的 user message,不读 assistant 回复(也读不动
//! —— assistant 行 thinking 是 base64 加密)。

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use super::HotwordSource;

pub struct ClaudeCodeSource;

impl HotwordSource for ClaudeCodeSource {
    fn id(&self) -> &'static str {
        "claude_code"
    }

    fn label(&self) -> &'static str {
        "Claude Code 历史"
    }

    fn available(&self) -> bool {
        projects_dir().is_some_and(|p| p.exists())
    }

    fn extract_user_text(&self, days: u32) -> Result<String> {
        let Some(root) = projects_dir() else {
            anyhow::bail!("找不到 Claude Code projects 目录(~/.claude/projects)");
        };
        let cutoff = Utc::now() - Duration::days(days as i64);
        let mut buf = String::new();
        let mut file_count = 0usize;
        let mut user_msg_count = 0usize;
        for project in std::fs::read_dir(&root)? {
            let project = project?;
            if !project.file_type()?.is_dir() {
                continue;
            }
            for session in std::fs::read_dir(project.path())? {
                let session = session?;
                let path = session.path();
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                file_count += 1;
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(rec) = serde_json::from_str::<RawLine>(line) else {
                        continue;
                    };
                    if rec.r#type.as_deref() != Some("user") {
                        continue;
                    }
                    // 时间戳过滤
                    if let Some(ts) = rec.timestamp.as_deref() {
                        if let Ok(t) = DateTime::parse_from_rfc3339(ts) {
                            if t.with_timezone(&Utc) < cutoff {
                                continue;
                            }
                        }
                    }
                    let Some(msg) = rec.message else { continue };
                    let text = match msg.content {
                        Content::Text(s) => s,
                        Content::Blocks(blocks) => blocks
                            .into_iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text),
                                ContentBlock::Other => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    };
                    if text.trim().is_empty() {
                        continue;
                    }
                    buf.push_str(&text);
                    buf.push('\n');
                    user_msg_count += 1;
                }
            }
        }
        tracing::info!(
            file_count,
            user_msg_count,
            extracted_chars = buf.chars().count(),
            days,
            "Claude Code history scanned"
        );
        Ok(buf)
    }
}

fn projects_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

#[derive(Deserialize)]
struct RawLine {
    r#type: Option<String>,
    timestamp: Option<String>,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    content: Content,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for Content {
    fn default() -> Self {
        Content::Text(String::new())
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}
