//! Claude Code 历史 source。读 `~/.claude/projects/<encoded-cwd>/<session>.jsonl`
//! 的 user / assistant 行,提取 plain text 内容做热词分析。
//!
//! jsonl 行结构(实测):
//! ```
//! {"type":"user","message":{"role":"user","content":"<text or blocks>"},
//!  "timestamp":"2026-05-12T02:34:45.183Z","cwd":"...", ...}
//! {"type":"assistant","message":{"role":"assistant","content":[
//!     {"type":"thinking", ...},
//!     {"type":"text","text":"..."},
//!     {"type":"tool_use", ...}
//! ]}, ...}
//! ```
//!
//! **过滤规则**(用户明确要求):
//! - 只取 user 真实输入文本 + assistant 回复文字内容(text 类型 block)
//! - 跳过工具调用(`tool_use`)、工具执行结果(`tool_result`)
//! - 跳过 thinking(Claude 内部推理,不是给用户看的回复)
//! - 跳过 image / 其他媒体 block

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
        let mut stats = ScanStats::default();

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
                // 文件级 mtime 短路:jsonl 一旦不再 append,mtime 就是最后一行
                // 写入时间;mtime < cutoff 说明文件里所有行都早于 cutoff,
                // 整个文件直接跳过(既快又对)。注意只能短路,反过来不行 ——
                // 同一文件可能横跨好几天,mtime ≥ cutoff 仍要按行 timestamp 再过。
                if let Ok(meta) = session.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let mtime_utc: DateTime<Utc> = mtime.into();
                        if mtime_utc < cutoff {
                            stats.files_skipped_by_mtime += 1;
                            continue;
                        }
                    }
                }
                stats.file_count += 1;
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
                    let role = match rec.r#type.as_deref() {
                        Some("user") => Role::User,
                        Some("assistant") => Role::Assistant,
                        _ => continue,
                    };
                    // 时间戳过滤(safer default):没 timestamp / parse 失败一律
                    // 跳过。之前是"没 timestamp 就放行",summary 行 / 特殊条目
                    // 没 timestamp 就无视 days 都进候选,用户反馈"days=1 还是
                    // 出好多天前的内容"就是这条。
                    let Some(ts) = rec.timestamp.as_deref() else {
                        stats.skipped_no_timestamp += 1;
                        continue;
                    };
                    let Ok(t) = DateTime::parse_from_rfc3339(ts) else {
                        stats.skipped_bad_timestamp += 1;
                        continue;
                    };
                    if t.with_timezone(&Utc) < cutoff {
                        stats.skipped_by_timestamp += 1;
                        continue;
                    }
                    let Some(msg) = rec.message else { continue };
                    let text = extract_plain_text(msg.content, &mut stats);
                    if text.trim().is_empty() {
                        continue;
                    }
                    buf.push_str(&text);
                    buf.push('\n');
                    match role {
                        Role::User => stats.user_msg_count += 1,
                        Role::Assistant => stats.assistant_msg_count += 1,
                    }
                }
            }
        }
        tracing::info!(
            file_count = stats.file_count,
            files_skipped_by_mtime = stats.files_skipped_by_mtime,
            user_msgs = stats.user_msg_count,
            assistant_msgs = stats.assistant_msg_count,
            text_blocks = stats.text_blocks,
            skipped_by_timestamp = stats.skipped_by_timestamp,
            skipped_no_timestamp = stats.skipped_no_timestamp,
            skipped_bad_timestamp = stats.skipped_bad_timestamp,
            skipped_tool_use = stats.skipped_tool_use,
            skipped_tool_result = stats.skipped_tool_result,
            skipped_thinking = stats.skipped_thinking,
            skipped_other = stats.skipped_other,
            extracted_chars = buf.chars().count(),
            days,
            "Claude Code history scanned"
        );
        Ok(buf)
    }
}

enum Role {
    User,
    Assistant,
}

#[derive(Default)]
struct ScanStats {
    file_count: usize,
    files_skipped_by_mtime: usize,
    user_msg_count: usize,
    assistant_msg_count: usize,
    text_blocks: usize,
    skipped_by_timestamp: usize,
    skipped_no_timestamp: usize,
    skipped_bad_timestamp: usize,
    skipped_tool_use: usize,
    skipped_tool_result: usize,
    skipped_thinking: usize,
    skipped_other: usize,
}

fn extract_plain_text(content: Content, stats: &mut ScanStats) -> String {
    match content {
        Content::Text(s) => {
            stats.text_blocks += 1;
            s
        }
        Content::Blocks(blocks) => {
            let mut parts = Vec::new();
            for b in blocks {
                match b {
                    ContentBlock::Text { text } => {
                        stats.text_blocks += 1;
                        parts.push(text);
                    }
                    ContentBlock::ToolUse => stats.skipped_tool_use += 1,
                    ContentBlock::ToolResult => stats.skipped_tool_result += 1,
                    ContentBlock::Thinking => stats.skipped_thinking += 1,
                    ContentBlock::Other => stats.skipped_other += 1,
                }
            }
            parts.join("\n")
        }
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
    ToolUse,
    ToolResult,
    Thinking,
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_only_from_blocks() {
        let mut stats = ScanStats::default();
        let blocks = Content::Blocks(vec![
            ContentBlock::Text {
                text: "用户问题".into(),
            },
            ContentBlock::ToolUse,
            ContentBlock::Text {
                text: "更多内容".into(),
            },
            ContentBlock::ToolResult,
            ContentBlock::Thinking,
        ]);
        let r = extract_plain_text(blocks, &mut stats);
        assert_eq!(r, "用户问题\n更多内容");
        assert_eq!(stats.text_blocks, 2);
        assert_eq!(stats.skipped_tool_use, 1);
        assert_eq!(stats.skipped_tool_result, 1);
        assert_eq!(stats.skipped_thinking, 1);
    }

    #[test]
    fn extract_string_content() {
        let mut stats = ScanStats::default();
        let r = extract_plain_text(Content::Text("hello".into()), &mut stats);
        assert_eq!(r, "hello");
        assert_eq!(stats.text_blocks, 1);
    }

    #[test]
    fn unknown_block_type_counts_as_other() {
        // tool_result 里实际格式可能是 {"type":"tool_result","content":"..."} —— serde 用 tag
        // 区分,只要是 snake_case match 上 ToolResult/ToolUse/Thinking 就走对应分支,
        // 否则走 Other(比如 image / 未来新增的 block 类型)
        let json = r#"[{"type":"image","source":{"type":"base64","data":"..."}}]"#;
        let blocks: Vec<ContentBlock> = serde_json::from_str(json).unwrap();
        let mut stats = ScanStats::default();
        for b in blocks {
            match b {
                ContentBlock::Other => stats.skipped_other += 1,
                _ => panic!("expected Other for image block"),
            }
        }
        assert_eq!(stats.skipped_other, 1);
    }

    #[test]
    fn tool_result_block_routes_correctly() {
        let json = r#"[{"type":"tool_result","tool_use_id":"x","content":"..."}]"#;
        let blocks: Vec<ContentBlock> = serde_json::from_str(json).unwrap();
        let mut stats = ScanStats::default();
        for b in blocks {
            if let ContentBlock::ToolResult = b {
                stats.skipped_tool_result += 1;
            } else {
                panic!("expected ToolResult");
            }
        }
        assert_eq!(stats.skipped_tool_result, 1);
    }
}
