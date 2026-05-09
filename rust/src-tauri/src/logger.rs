//! 日志初始化：同时输出到 stderr 和日志文件（按天 rotation + 7 天自动清理）。
//! 对应 Go 版的 logger.go。

use crate::dirs::log_dir;
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const LOG_RETENTION_DAYS: u64 = 7;
const LOG_PREFIX: &str = "voice-claude";
const LOG_SUFFIX: &str = "log";

/// 初始化日志系统。返回的 WorkerGuard 保证日志刷盘，需要在 main 中持有。
pub fn init(level: &str) -> WorkerGuard {
    fs::create_dir_all(log_dir()).ok();
    cleanup_old_logs();

    // daily rotation：voice-claude.2026-05-09.log 格式
    // （之前用简写 rolling::daily 产出 voice-claude.log.2026-05-09，
    // .log 不在末尾 macOS 无法识别扩展名，双击打不开）
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .build(log_dir())
        .expect("日志 appender 初始化失败");
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
/// 匹配新格式 `voice-claude.YYYY-MM-DD.log` 和旧格式 `voice-claude.log.YYYY-MM-DD`
/// 以及首版无后缀的 `voice-claude.log`——统一按 prefix 过滤。
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
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with(LOG_PREFIX) {
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
