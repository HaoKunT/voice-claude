//! 跨平台配置 / 日志 / 历史数据库目录。
//! 对应 Go 版的 dirs.go。

use std::path::PathBuf;

/// 应用名称，用于所有配置目录
pub const APP_NAME: &str = "voice-claude";

/// 返回应用配置目录。
///
/// - macOS: ~/Library/Application Support/voice-claude
/// - Windows: %APPDATA%\voice-claude
/// - Linux: ~/.config/voice-claude
pub fn config_dir() -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "windows") {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    };
    base.join(APP_NAME)
}

/// 返回应用日志目录。
///
/// - macOS: ~/Library/Logs/voice-claude
/// - Windows: %LOCALAPPDATA%\voice-claude\logs
/// - Linux: ~/.cache/voice-claude/logs
pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join("Library").join("Logs").join(APP_NAME))
            .unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "windows") {
        dirs::data_local_dir()
            .map(|d| d.join(APP_NAME).join("logs"))
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        dirs::cache_dir()
            .map(|d| d.join(APP_NAME).join("logs"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// 配置文件完整路径
pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// 历史记录数据库路径
pub fn history_path() -> PathBuf {
    config_dir().join("history.db")
}

/// 日志文件路径
pub fn log_file_path() -> PathBuf {
    log_dir().join(format!("{}.log", APP_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_contain_app_name() {
        assert!(config_dir().to_string_lossy().contains(APP_NAME));
        assert!(log_dir().to_string_lossy().contains(APP_NAME));
        assert!(config_path().to_string_lossy().ends_with("config.json"));
        assert!(history_path().to_string_lossy().ends_with("history.db"));
    }
}
