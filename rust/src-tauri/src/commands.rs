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
pub fn save_config(cfg: Config, state: State<'_, AppState>) -> Result<(), String> {
    cfg.save().map_err(|e| e.to_string())?;
    state.replace(cfg);
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
    open_path(&dirs::log_file_path().to_string_lossy())
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

#[tauri::command]
pub async fn download_sense_voice(app: AppHandle) -> Result<(), String> {
    local::download_model(move |progress| {
        let _ = app.emit("sense-voice-download-progress", progress);
    })
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
