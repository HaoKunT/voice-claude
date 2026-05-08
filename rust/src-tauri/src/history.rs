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
    DB.set(Mutex::new(conn))
        .map_err(|_| anyhow::anyhow!("DB already initialized"))?;
    Ok(())
}

/// 保存一条识别记录；DB 未初始化时静默返回。
pub fn save(raw: &str, corrected: &str, provider: &str) {
    let Some(lock) = DB.get() else {
        return;
    };
    let conn = lock.lock();
    let ts = chrono::Utc::now().timestamp();
    let _ = conn.execute(
        "INSERT INTO history (created_at, raw_text, corrected_text, asr_provider) VALUES (?, ?, ?, ?)",
        params![ts, raw, corrected, provider],
    );
}

/// 读取最近 limit 条记录。
pub fn load(limit: i64) -> Result<Vec<HistoryEntry>> {
    let Some(lock) = DB.get() else {
        return Ok(Vec::new());
    };
    let conn = lock.lock();
    let mut stmt = conn.prepare(
        "SELECT id, created_at, raw_text, corrected_text, asr_provider
         FROM history ORDER BY created_at DESC LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            created_at: row.get(1)?,
            raw_text: row.get(2)?,
            corrected_text: row.get(3)?,
            asr_provider: row.get(4)?,
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
