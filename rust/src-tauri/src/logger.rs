//! 日志初始化：同时输出到 stderr 和日志文件（按天 rotation + 7 天自动清理）。
//! 对应 Go 版的 logger.go。

use crate::dirs::{log_dir, log_file_path};
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const LOG_RETENTION_DAYS: u64 = 7;

/// 初始化日志系统。返回的 WorkerGuard 保证日志刷盘，需要在 main 中持有。
pub fn init(level: &str) -> WorkerGuard {
    fs::create_dir_all(log_dir()).ok();
    cleanup_old_logs();

    // daily rotation：voice-claude.log.2026-05-08 格式
    let file_appender = tracing_appender::rolling::daily(log_dir(), log_file_name());
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(parse_level(level)));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true))
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    guard
}

/// 启动时删除 LOG_RETENTION_DAYS 天之前的日志文件。
fn cleanup_old_logs() {
    use std::time::{Duration, SystemTime};
    let cutoff =
        match SystemTime::now().checked_sub(Duration::from_secs(LOG_RETENTION_DAYS * 86400)) {
            Some(t) => t,
            None => return,
        };
    let Ok(entries) = fs::read_dir(log_dir()) else {
        return;
    };
    let base = log_file_name();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // 只清理以 voice-claude.log 开头的文件（rolling 产物）
        if !name_str.starts_with(&base) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        if mtime < cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 根据字符串 level 返回 EnvFilter 的格式。
pub fn parse_level(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "trace" => "trace".into(),
        "debug" => "debug".into(),
        "warn" => "warn".into(),
        "error" => "error".into(),
        _ => "info".into(),
    }
}

fn log_file_name() -> String {
    log_file_path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "voice-claude.log".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_level_valid() {
        assert_eq!(parse_level("debug"), "debug");
        assert_eq!(parse_level("WARN"), "warn");
        assert_eq!(parse_level("error"), "error");
    }

    #[test]
    fn parse_level_default() {
        assert_eq!(parse_level(""), "info");
        assert_eq!(parse_level("unknown"), "info");
    }
}
