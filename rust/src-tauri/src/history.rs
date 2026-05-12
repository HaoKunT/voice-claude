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

/// 按 id 查询单条记录;不存在返回 None。
pub fn get(id: i64) -> Result<Option<HistoryEntry>> {
    let Some(lock) = DB.get() else {
        return Ok(None);
    };
    let conn = lock.lock();
    let mut stmt = conn.prepare(
        "SELECT id, created_at, raw_text, corrected_text, asr_provider, duration_ms
         FROM history WHERE id = ?",
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            created_at: row.get(1)?,
            raw_text: row.get(2)?,
            corrected_text: row.get(3)?,
            asr_provider: row.get(4)?,
            duration_ms: row.get(5).unwrap_or(0),
        })
    })?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
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

/// 聚合统计,用于 HistoryView 顶部 dashboard。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub total_count: i64,
    pub total_duration_ms: i64,
    pub total_chars: i64,
    /// 字/分钟,基于 duration_ms > 0 的记录;没有可测数据返回 0
    pub avg_chars_per_minute: f32,
    /// 按人均打字 40 字/分钟估计节省分钟数,取 max(0, 打字耗时 - 口述耗时)
    pub saved_minutes: f32,
    /// 首条记录时间(unix seconds),无记录时 None
    pub first_created_at: Option<i64>,
}

const TYPING_CHARS_PER_MINUTE: f32 = 40.0;

pub fn stats() -> Result<Stats> {
    let Some(lock) = DB.get() else {
        return Ok(Stats::default());
    };
    let conn = lock.lock();
    // 直接把 corrected_text 读出来在 Rust 里 chars().count(),
    // SQLite 的 length() 对中文返回字节数不准
    let mut stmt = conn.prepare("SELECT created_at, corrected_text, duration_ms FROM history")?;
    let mut total_count = 0i64;
    let mut total_duration_ms = 0i64;
    let mut total_chars = 0i64;
    let mut measured_duration_ms = 0i64;
    let mut measured_chars = 0i64;
    let mut first_created_at: Option<i64> = None;

    let rows = stmt.query_map([], |row| {
        let created_at: i64 = row.get(0)?;
        let text: String = row.get(1)?;
        let dur: i64 = row.get(2).unwrap_or(0);
        Ok((created_at, text, dur))
    })?;
    for r in rows {
        let (created_at, text, dur) = r?;
        total_count += 1;
        total_duration_ms += dur;
        let chars = text.chars().count() as i64;
        total_chars += chars;
        if dur > 0 {
            measured_duration_ms += dur;
            measured_chars += chars;
        }
        match first_created_at {
            Some(ts) if ts <= created_at => {}
            _ => first_created_at = Some(created_at),
        }
    }

    let avg_chars_per_minute = if measured_duration_ms > 0 {
        (measured_chars as f32) / (measured_duration_ms as f32 / 60_000.0)
    } else {
        0.0
    };
    // 节省分钟数:打字耗时估计 - 实际口述耗时
    let typed_minutes = (total_chars as f32) / TYPING_CHARS_PER_MINUTE;
    let spoken_minutes = (total_duration_ms as f32) / 60_000.0;
    let saved_minutes = (typed_minutes - spoken_minutes).max(0.0);

    Ok(Stats {
        total_count,
        total_duration_ms,
        total_chars,
        avg_chars_per_minute,
        saved_minutes,
        first_created_at,
    })
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
