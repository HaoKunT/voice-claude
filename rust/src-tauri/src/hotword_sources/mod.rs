//! 热词候选源 —— 从外部数据(用户聊天历史、笔记、commit log 等)抽取
//! 用户高频词汇作为识别词典候选。
//!
//! 当前只有 `ClaudeCode` 一个 source(从 `~/.claude/projects/` 读 jsonl 的
//! type=user/assistant 行,只取 text block,跳工具调用 / 工具结果 / thinking),
//! 但抽象设计成 trait 让未来添加 Cursor / VSCode chat / git log 等其他来源时,
//! 前后端共用同一套 source → LLM 提取 → UI 勾选流程,只需新增一个 source 实现
//! + 注册即可。
//!
//! 数据流(单层 LLM 提取):
//!   1. `extract_user_text(days)` 从 source 读取最近 N 天的用户文本(纯本地)
//!   2. `llm_filter::extract_hotwords(text, existing, ...)` 把原文整段交给 LLM,
//!      LLM 自己识别专名 / 术语并返回 JSON array(中英混合)
//!   3. (commands)在原文里 substring count 算 freq + 过滤已存在
//!   4. (前端)弹 modal 让用户勾选 → 加入 cfg.hotwords
//!
//! **历史**:之前还有一层"本地 token 切分 + 频率统计 + 停用词过滤"做候选,
//! 但本地分词只切英文 token,中文专名完全没机会进候选;LLM 看到全英文候选
//! 列表也跟着只挑英文。删掉本地层后由 LLM 直接从原文挑词,中英文都能出。

pub mod claude_code;
pub mod llm_filter;

use anyhow::Result;
use serde::Serialize;

/// 一个热词数据源。新增来源(Cursor / VSCode chat / git log 等)时,实现此
/// trait 并在 `available_sources()` 里注册即可,前端 modal 自动多一个选项。
pub trait HotwordSource: Send + Sync {
    /// 稳定的 id,用作前后端 IPC 参数。例:`"claude_code"`。
    fn id(&self) -> &'static str;

    /// 人类可读的展示名。例:`"Claude Code 历史"`。
    fn label(&self) -> &'static str;

    /// 来源数据是否可用(目录是否存在等)。不可用时前端 dropdown 灰掉这一项。
    fn available(&self) -> bool;

    /// 抽取最近 `days` 天用户产生的文本(用户消息 / 笔记 / commit message 等
    /// 由用户编辑的内容,排除自动生成 / 系统消息)。返回拼接好的 String,
    /// 候选词分析阶段不关心来源,只做 source-agnostic 的频率统计 + 过滤。
    fn extract_user_text(&self, days: u32) -> Result<String>;
}

/// 给前端展示的 source 元数据。
#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub id: String,
    pub label: String,
    pub available: bool,
}

/// 注册所有可用 source。当前只有 ClaudeCode;新增时 push 进来即可。
pub fn available_sources() -> Vec<Box<dyn HotwordSource>> {
    vec![Box::new(claude_code::ClaudeCodeSource)]
}

pub fn list_for_ui() -> Vec<SourceInfo> {
    available_sources()
        .iter()
        .map(|s| SourceInfo {
            id: s.id().to_string(),
            label: s.label().to_string(),
            available: s.available(),
        })
        .collect()
}

pub fn find_source(id: &str) -> Option<Box<dyn HotwordSource>> {
    available_sources().into_iter().find(|s| s.id() == id)
}
