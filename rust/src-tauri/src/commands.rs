//! Tauri IPC commands：前端（React）调用的 Rust 函数。

use crate::asr::local;
use crate::audio::{list_capture_devices, CaptureDevice};
use crate::config::Config;
use crate::AppState;
use crate::{correct, dirs, history};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize)]
pub struct DeviceInfo {
    pub name: String,
}

impl From<CaptureDevice> for DeviceInfo {
    fn from(d: CaptureDevice) -> Self {
        Self { name: d.name }
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    (*state.snapshot()).clone()
}

#[tauri::command]
pub fn save_config(
    cfg: Config,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let prev = state.snapshot();
    let prev_hotkey = prev.hotkey.clone();
    let prev_log_level = prev.log_level.clone();
    let new_hotkey = cfg.hotkey.clone();
    let new_log_level = cfg.log_level.clone();
    cfg.save().map_err(|e| e.to_string())?;
    state.replace(cfg);
    // 热键变了就重新注册全局热键，否则老 accelerator 还挂着、新的没生效
    if new_hotkey != prev_hotkey {
        if let Err(e) = crate::register_hotkey(&app, &new_hotkey) {
            tracing::warn!(error = ?e, "save_config 后重注册热键失败");
            return Err(format!("热键注册失败：{}", e));
        }
    }
    // 日志级别变了就热替换 tracing EnvFilter
    if new_log_level != prev_log_level {
        crate::logger::reload(&new_log_level);
    }
    Ok(())
}

#[tauri::command]
pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    list_capture_devices()
        .map(|ds| ds.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_history(limit: i64) -> Result<Vec<history::HistoryEntry>, String> {
    history::load(limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history(id: i64) -> Result<(), String> {
    history::delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    history::clear().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_ollama(url: String) -> Result<(), String> {
    correct::check_ollama(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    // daily rotation 后 log_file_path() 的那个无后缀文件不再被写入，
    // 改成找日志目录下 mtime 最新的 voice-claude.log* 文件打开；
    // 完全没日志就退化为打开目录。
    let dir = dirs::log_dir();
    let mut latest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            // 匹配新格式 voice-claude.YYYY-MM-DD.log 和老格式 voice-claude.log.YYYY-MM-DD，
            // 以及首版无后缀的 voice-claude.log
            if !name.to_string_lossy().starts_with("voice-claude") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if latest.as_ref().map_or(true, |(_, t)| mtime > *t) {
                        latest = Some((entry.path(), mtime));
                    }
                }
            }
        }
    }
    match latest {
        Some((p, _)) => open_path(&p.to_string_lossy()),
        None => open_path(&dir.to_string_lossy()),
    }
}

#[tauri::command]
pub fn open_log_dir() -> Result<(), String> {
    open_path(&dirs::log_dir().to_string_lossy())
}

/// 读取最新日志文件的最后 limit 行，返回给前端展示。
/// 找 mtime 最新的 voice-claude.*.log；日志没生成时返回空列表。
#[tauri::command]
pub fn read_recent_logs(limit: usize) -> Result<Vec<String>, String> {
    let dir = dirs::log_dir();
    let mut latest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("voice-claude") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if latest.as_ref().map_or(true, |(_, t)| mtime > *t) {
                        latest = Some((entry.path(), mtime));
                    }
                }
            }
        }
    }
    let Some((path, _)) = latest else {
        return Ok(vec![]);
    };
    // 避免把特别大的日志全读入内存：超过 2MB 只取末尾一段
    const MAX_READ: u64 = 2 * 1024 * 1024;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let file_len = meta.len();
    let from = file_len.saturating_sub(MAX_READ);
    let content = if from == 0 {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
        let mut buf = Vec::with_capacity(MAX_READ as usize);
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        // 丢掉第一行（可能 seek 到半个 UTF-8 字符或半行）
        let s = String::from_utf8_lossy(&buf).into_owned();
        match s.find('\n') {
            Some(i) => s[i + 1..].to_string(),
            None => s,
        }
    };
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].to_vec())
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_hash: &'static str,
    pub git_dirty: &'static str,
    pub rustc_version: &'static str,
    pub build_time: &'static str,
    pub target: &'static str,
    pub tauri_version: &'static str,
    pub debug: bool,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("VC_GIT_HASH"),
        git_dirty: env!("VC_GIT_DIRTY"),
        rustc_version: env!("VC_RUSTC_VERSION"),
        build_time: env!("VC_BUILD_TIME"),
        target: env!("VC_TARGET"),
        tauri_version: tauri::VERSION,
        debug: cfg!(debug_assertions),
    }
}

#[tauri::command]
pub fn is_sense_voice_available() -> bool {
    local::is_available()
}

#[derive(Serialize)]
pub struct SenseVoiceInfo {
    pub url: String,
    pub sha256: String,
    pub available: bool,
    pub model_dir: String,
}

#[tauri::command]
pub fn get_sense_voice_info() -> SenseVoiceInfo {
    SenseVoiceInfo {
        url: local::MODEL_URL.into(),
        sha256: local::MODEL_SHA256.into(),
        available: local::is_available(),
        model_dir: local::model_path().to_string_lossy().into_owned(),
    }
}

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

#[tauri::command]
pub async fn download_sense_voice(app: AppHandle) -> Result<(), String> {
    local::download_model(move |downloaded, total| {
        let _ = app.emit(
            "sense-voice-download-progress",
            DownloadProgress { downloaded, total },
        );
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_sense_voice_tarball(path: String) -> Result<(), String> {
    local::import_tarball(std::path::PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())
}

/// 把当前热词导出为 CSV 字符串（前端用 save dialog 保存）
#[tauri::command]
pub fn export_hotwords_csv(state: State<'_, AppState>) -> String {
    let cfg = state.snapshot();
    let mut out = String::from("原词,替换为\n");
    let mut entries: Vec<_> = cfg.hotwords.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in entries {
        out.push_str(&format!("{},{}\n", csv_escape(k), csv_escape(v)));
    }
    out
}

/// 从 CSV 字符串解析热词，覆盖当前配置（保留其他字段）
#[tauri::command]
pub fn import_hotwords_csv(
    csv: String,
    merge: bool,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let mut hotwords = if merge {
        state.snapshot().hotwords.clone()
    } else {
        std::collections::HashMap::new()
    };
    let mut added = 0usize;
    for (i, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // 跳过表头（可选）
        if i == 0 && (line.starts_with("原词") || line.to_lowercase().starts_with("from")) {
            continue;
        }
        let Some(row) = parse_csv_row(line) else {
            continue;
        };
        if row.len() < 2 {
            continue;
        }
        let from = row[0].trim();
        let to = row[1].trim();
        if from.is_empty() {
            continue;
        }
        hotwords.insert(from.to_string(), to.to_string());
        added += 1;
    }

    // 更新配置
    let mut cfg = (*state.snapshot()).clone();
    cfg.hotwords = hotwords;
    cfg.save().map_err(|e| e.to_string())?;
    state.replace(cfg);
    Ok(added)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 简易 CSV 行解析（支持引号包围 + 引号转义）。
fn parse_csv_row(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quote {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quote = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quote = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    Some(fields)
}

#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    open_path(&dirs::config_dir().to_string_lossy())
}

fn open_path(path: &str) -> Result<(), String> {
    use std::process::Command;
    let cmd = if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    cmd.map(|_| ()).map_err(|e| e.to_string())
}

/// macOS：查询辅助功能权限是否授予（ad-hoc 签名升级后此权限常会失效）。
/// 其他平台直接返回 true。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility() -> bool {
    true
}

/// 跳到系统「隐私与安全性 → 辅助功能」面板。
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        open_path("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("仅 macOS 支持".into())
    }
}
