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
    /// ASR 调用耗时(从调用开始到拿到最终文本)。旧数据为 0。
    #[serde(default)]
    pub asr_ms: i64,
    /// 润色调用耗时。off profile 或未润色为 0。旧数据为 0。
    #[serde(default)]
    pub polish_ms: i64,
    /// 本次使用的润色 model 名。off / 未润色为空。旧数据为空。
    #[serde(default)]
    pub polish_model: String,
    /// 本次使用的润色 provider 标识。
    ///
    /// - ollama / openrouter mode:直接是 mode 名
    /// - cloud mode:profile.url 的 host(api.groq.com 等),因为同为 cloud 可能接不同厂商
    ///
    /// off / 未润色为空。旧数据可能是字面 "cloud"(升级前 commit 写的值)。
    /// 统计不按这个分组,展示层把每个 model 用过的 provider 集合列出来。
    #[serde(default)]
    pub polish_mode: String,
    /// 本次润色是否触发了超时。超时时 polish_ms 记 timeout_ms 入 p99,polish_timeout=true
    /// 单独计数,让 StatsView 能显示"这个 model 经常超时"。旧数据为 false。
    #[serde(default)]
    pub polish_timeout: bool,
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
    // Migration: 逐列检查并 ALTER ADD COLUMN(SQLite 不支持 IF NOT EXISTS 语法,用
    // PRAGMA table_info 查当前列)。老数据新列填默认值,聚合统计时用 WHERE > 0 过滤。
    let existing_cols: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(history)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let cols: std::collections::HashSet<String> = rows.filter_map(Result::ok).collect();
        cols
    };
    let migrations: &[(&str, &str)] = &[
        (
            "duration_ms",
            "ALTER TABLE history ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "asr_ms",
            "ALTER TABLE history ADD COLUMN asr_ms INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "polish_ms",
            "ALTER TABLE history ADD COLUMN polish_ms INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "polish_model",
            "ALTER TABLE history ADD COLUMN polish_model TEXT NOT NULL DEFAULT ''",
        ),
        (
            "polish_mode",
            "ALTER TABLE history ADD COLUMN polish_mode TEXT NOT NULL DEFAULT ''",
        ),
        (
            "polish_timeout",
            "ALTER TABLE history ADD COLUMN polish_timeout INTEGER NOT NULL DEFAULT 0",
        ),
    ];
    for (col, sql) in migrations {
        if !existing_cols.contains(*col) {
            conn.execute(sql, [])
                .with_context(|| format!("add {}", col))?;
        }
    }
    DB.set(Mutex::new(conn))
        .map_err(|_| anyhow::anyhow!("DB already initialized"))?;
    Ok(())
}

/// 一次识别的全部落库数据(延时拆分用)。
pub struct SaveEntry<'a> {
    pub raw: &'a str,
    pub corrected: &'a str,
    pub provider: &'a str,
    pub duration_ms: i64,
    pub asr_ms: i64,
    pub polish_ms: i64,
    pub polish_model: &'a str,
    pub polish_mode: &'a str,
    pub polish_timeout: bool,
}

/// 保存一条识别记录;DB 未初始化时静默返回。
pub fn save(entry: SaveEntry<'_>) {
    let Some(lock) = DB.get() else {
        return;
    };
    let conn = lock.lock();
    let ts = chrono::Utc::now().timestamp();
    let _ = conn.execute(
        "INSERT INTO history (created_at, raw_text, corrected_text, asr_provider, duration_ms, asr_ms, polish_ms, polish_model, polish_mode, polish_timeout)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            ts,
            entry.raw,
            entry.corrected,
            entry.provider,
            entry.duration_ms,
            entry.asr_ms,
            entry.polish_ms,
            entry.polish_model,
            entry.polish_mode,
            entry.polish_timeout as i64,
        ],
    );
}

const HISTORY_COLUMNS: &str =
    "id, created_at, raw_text, corrected_text, asr_provider, duration_ms, asr_ms, polish_ms, polish_model, polish_mode, polish_timeout";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        created_at: row.get(1)?,
        raw_text: row.get(2)?,
        corrected_text: row.get(3)?,
        asr_provider: row.get(4)?,
        duration_ms: row.get(5).unwrap_or(0),
        asr_ms: row.get(6).unwrap_or(0),
        polish_ms: row.get(7).unwrap_or(0),
        polish_model: row.get(8).unwrap_or_default(),
        polish_mode: row.get(9).unwrap_or_default(),
        polish_timeout: row.get::<_, i64>(10).unwrap_or(0) != 0,
    })
}

/// 按 id 查询单条记录;不存在返回 None。
pub fn get(id: i64) -> Result<Option<HistoryEntry>> {
    let Some(lock) = DB.get() else {
        return Ok(None);
    };
    let conn = lock.lock();
    let sql = format!("SELECT {HISTORY_COLUMNS} FROM history WHERE id = ?");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], map_row)?;
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
    let sql = format!("SELECT {HISTORY_COLUMNS} FROM history ORDER BY created_at DESC LIMIT ?");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], map_row)?;
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

/// 延时统计单行:按 ASR provider 或 润色 model 分组后的汇总。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyRow {
    /// 分组键(provider 或 model 名)
    pub key: String,
    pub count: i64,
    pub avg_ms: i64,
    pub p99_ms: i64,
    /// 仅润色行用:这个 model 用过的 provider 集合(ollama / openrouter / api.groq.com 等)。
    /// ASR 行为空。老数据 polish_mode 为空字符串时也不入集合。
    #[serde(default)]
    pub providers: Vec<String>,
    /// 仅润色行用:本组里触发超时的次数(polish_timeout=1 的行数)。ASR 行为 0。
    #[serde(default)]
    pub timeout_count: i64,
}

/// 单个时间窗口(全量 / 近 24h / 近 7d)的 ASR + 润色延时。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyWindow {
    pub asr: Vec<LatencyRow>,
    pub polish: Vec<LatencyRow>,
}

/// 三套时间窗口的延时统计,供「状态」页展示。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyStats {
    pub all_time: LatencyWindow,
    pub last_24h: LatencyWindow,
    pub last_7d: LatencyWindow,
}

/// 聚合 ASR 和润色延时,按全量 / 近 24h / 近 7d 三套窗口切片。
/// SQLite 没内置 percentile,数据量不大(几千条级)直接读到 Rust 里排序算。
pub fn latency_stats() -> Result<LatencyStats> {
    let Some(lock) = DB.get() else {
        return Ok(LatencyStats::default());
    };
    let conn = lock.lock();
    // 只读统计需要的 7 列,避免把 raw_text/corrected_text 这种大字段一起拖进来
    let mut stmt = conn.prepare(
        "SELECT created_at, asr_provider, asr_ms, polish_ms, polish_model, polish_mode, polish_timeout FROM history",
    )?;
    let rows: Vec<(i64, String, i64, i64, String, String, bool)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
                row.get::<_, i64>(3).unwrap_or(0),
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, i64>(6).unwrap_or(0) != 0,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let now = chrono::Utc::now().timestamp();
    let cutoff_24h = now - 24 * 3600;
    let cutoff_7d = now - 7 * 24 * 3600;

    let mut all = WindowAcc::default();
    let mut d24 = WindowAcc::default();
    let mut d7 = WindowAcc::default();
    for (ts, provider, asr_ms, polish_ms, polish_model, polish_mode, polish_timeout) in &rows {
        all.add(
            provider,
            *asr_ms,
            polish_model,
            polish_mode,
            *polish_ms,
            *polish_timeout,
        );
        if *ts >= cutoff_7d {
            d7.add(
                provider,
                *asr_ms,
                polish_model,
                polish_mode,
                *polish_ms,
                *polish_timeout,
            );
        }
        if *ts >= cutoff_24h {
            d24.add(
                provider,
                *asr_ms,
                polish_model,
                polish_mode,
                *polish_ms,
                *polish_timeout,
            );
        }
    }

    Ok(LatencyStats {
        all_time: all.finish(),
        last_24h: d24.finish(),
        last_7d: d7.finish(),
    })
}

#[derive(Default)]
struct PolishAcc {
    samples: Vec<i64>,
    providers: std::collections::BTreeSet<String>,
    timeout_count: i64,
}

#[derive(Default)]
struct WindowAcc {
    asr: std::collections::BTreeMap<String, Vec<i64>>,
    /// 润色 key = model 名,value = (延时样本 + provider 集合 + 超时次数)
    polish: std::collections::BTreeMap<String, PolishAcc>,
}

impl WindowAcc {
    fn add(
        &mut self,
        provider: &str,
        asr_ms: i64,
        polish_model: &str,
        polish_mode: &str,
        polish_ms: i64,
        polish_timeout: bool,
    ) {
        if asr_ms > 0 && !provider.is_empty() {
            self.asr
                .entry(provider.to_string())
                .or_default()
                .push(asr_ms);
        }
        if polish_ms > 0 && !polish_model.is_empty() {
            let e = self.polish.entry(polish_model.to_string()).or_default();
            e.samples.push(polish_ms);
            if !polish_mode.is_empty() {
                e.providers.insert(polish_mode.to_string());
            }
            if polish_timeout {
                e.timeout_count += 1;
            }
        }
    }

    fn finish(self) -> LatencyWindow {
        LatencyWindow {
            asr: summarize_asr(self.asr),
            polish: summarize_polish(self.polish),
        }
    }
}

fn p99_index(len: usize) -> usize {
    // p99:排序后取 ceil(0.99 * len) - 1 位置(clamp 到 [0, len-1])。
    // len < 100 时 p99 就是最大值
    ((0.99_f64 * len as f64).ceil() as usize)
        .saturating_sub(1)
        .min(len - 1)
}

fn summarize_asr(groups: std::collections::BTreeMap<String, Vec<i64>>) -> Vec<LatencyRow> {
    let mut rows: Vec<LatencyRow> = groups
        .into_iter()
        .map(|(key, mut v)| {
            v.sort_unstable();
            let count = v.len() as i64;
            let sum: i64 = v.iter().sum();
            LatencyRow {
                key,
                count,
                avg_ms: sum / count,
                p99_ms: v[p99_index(v.len())],
                providers: Vec::new(),
                timeout_count: 0,
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.count));
    rows
}

fn summarize_polish(groups: std::collections::BTreeMap<String, PolishAcc>) -> Vec<LatencyRow> {
    let mut rows: Vec<LatencyRow> = groups
        .into_iter()
        .map(|(key, acc)| {
            let mut v = acc.samples;
            v.sort_unstable();
            let count = v.len() as i64;
            let sum: i64 = v.iter().sum();
            LatencyRow {
                key,
                count,
                avg_ms: sum / count,
                p99_ms: v[p99_index(v.len())],
                providers: acc.providers.into_iter().collect(),
                timeout_count: acc.timeout_count,
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.count));
    rows
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
