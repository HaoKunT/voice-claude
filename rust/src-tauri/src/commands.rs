//! Tauri IPC commands：前端（React）调用的 Rust 函数。

use crate::audio::{list_capture_devices, CaptureDevice};
use crate::config::Config;
use crate::{correct, dirs, history};
use crate::AppState;
use serde::Serialize;
use tauri::State;

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
