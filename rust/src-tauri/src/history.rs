//! 历史记录：SQLite 本地存储。
//! 对应 Go 版的 history.go。

use crate::dirs::{config_dir, history_path};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: i64, // unix seconds
    pub raw_text: String,
    pub corrected_text: String,
    pub asr_provider: String,
    #[serde(default)]
    pub duration_ms: i64,
}

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 初始化历史数据库，首次调用时创建表结构。
pub fn init() -> Result<()> {
    fs::create_dir_all(config_dir()).ok();
    let conn = Connection::open(history_path())
        .with_context(|| format!("open sqlite at {:?}", history_path()))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at INTEGER NOT NULL,
            raw_text TEXT NOT NULL,
            corrected_text TEXT NOT NULL,
            asr_provider TEXT NOT NULL
        )",
        [],
    )
    .context("create history table")?;
    // Migration: 加 duration_ms 列（旧数据为 0）。
    // 用 PRAGMA 查现有列，不存在才 ALTER（ALTER TABLE IF NOT EXISTS 在 SQLite 不支持列）
    let has_duration = {
        let mut stmt = conn.prepare("PRAGMA table_info(history)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        cols.iter().any(|c| c == "duration_ms")
    };
    if !has_duration {
        conn.execute(
            "ALTER TABLE history ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("add duration_ms column")?;
    }
    DB.set(Mutex::new(conn))
        .map_err(|_| anyhow::anyhow!("DB already initialized"))?;
    Ok(())
}

/// 保存一条识别记录；DB 未初始化时静默返回。
pub fn save(raw: &str, corrected: &str, provider: &str, duration_ms: i64) {
    let Some(lock) = DB.get() else {
        return;
    };
    let conn = lock.lock();
    let ts = chrono::Utc::now().timestamp();
    let _ = conn.execute(
        "INSERT INTO history (created_at, raw_text, corrected_text, asr_provider, duration_ms) VALUES (?, ?, ?, ?, ?)",
        params![ts, raw, corrected, provider, duration_ms],
    );
}

/// 读取最近 limit 条记录。
pub fn load(limit: i64) -> Result<Vec<HistoryEntry>> {
    let Some(lock) = DB.get() else {
        return Ok(Vec::new());
    };
    let conn = lock.lock();
    let mut stmt = conn.prepare(
        "SELECT id, created_at, raw_text, corrected_text, asr_provider, duration_ms
         FROM history ORDER BY created_at DESC LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            created_at: row.get(1)?,
            raw_text: row.get(2)?,
            corrected_text: row.get(3)?,
            asr_provider: row.get(4)?,
            duration_ms: row.get(5).unwrap_or(0),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// 删除指定 id 的记录。
pub fn delete(id: i64) -> Result<()> {
    let Some(lock) = DB.get() else {
        return Ok(());
    };
    let conn = lock.lock();
    conn.execute("DELETE FROM history WHERE id = ?", params![id])?;
    Ok(())
}

/// 清空全部历史。
pub fn clear() -> Result<()> {
    let Some(lock) = DB.get() else {
        return Ok(());
    };
    let conn = lock.lock();
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}
