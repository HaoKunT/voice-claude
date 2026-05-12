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
pub fn save_config(cfg: Config, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let prev = state.snapshot();
    let prev_hotkey = prev.hotkey.clone();
    let prev_log_level = prev.log_level.clone();
    let new_hotkey = cfg.hotkey.clone();
    let new_log_level = cfg.log_level.clone();
    // 热键变了就重新注册全局热键，否则老 accelerator 还挂着、新的没生效
    if new_hotkey != prev_hotkey {
        if let Err(e) = crate::register_hotkey(&app, &new_hotkey) {
            tracing::warn!(error = ?e, "save_config 后重注册热键失败");
            return Err(format!("热键注册失败：{}", e));
        }
    }
    if let Err(e) = cfg.save() {
        if new_hotkey != prev_hotkey {
            let _ = crate::register_hotkey(&app, &prev_hotkey);
        }
        return Err(e.to_string());
    }
    state.replace(cfg);
    // 日志级别变了就热替换 tracing EnvFilter
    if new_log_level != prev_log_level {
        crate::logger::reload(&new_log_level);
    }
    // 广播给其他前端组件（比如主窗口 sidebar 显示的快捷键）刷新自己的 cfg 副本
    let _ = app.emit("config-updated", ());
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

/// 历史记录聚合统计(总次数/时长/字数/字速/节省时间),用于 HistoryView 仪表盘。
#[tauri::command]
pub fn get_history_stats() -> Result<history::Stats, String> {
    history::stats().map_err(|e| e.to_string())
}

/// 用指定 profile 对历史条目的 raw_text 重新跑一遍润色,返回新结果。
/// 不写回数据库 —— 让用户可以试不同 profile 看效果,满意自己复制。
///
/// profile_id 找不到时 fallback 到 active profile —— 前端选中的 profile 可能刚好
/// 在另一个窗口被删/改了,此时给个可用结果比抛错更友好。fallback 会在日志里 warn。
#[tauri::command]
pub async fn repolish_history(
    history_id: i64,
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let entry = history::get(history_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("历史记录 #{} 不存在", history_id))?;
    let cfg = state.snapshot();
    let profile = match cfg.polish_profiles.iter().find(|p| p.id == profile_id) {
        Some(p) => p,
        None => {
            tracing::warn!(
                requested = %profile_id,
                fallback = %cfg.active_profile_id,
                "repolish: profile 找不到,fallback 到 active profile"
            );
            cfg.active_profile()
        }
    };
    tracing::info!(
        profile = %profile.name,
        mode = %profile.mode,
        model = %profile.model,
        "repolish: 开始润色"
    );
    correct::correct(&entry.raw_text, profile, cfg.correct_timeout_secs())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_ollama(url: String) -> Result<(), String> {
    correct::check_ollama(&url).await.map_err(|e| e.to_string())
}

/// 找日志目录下 mtime 最新的日志文件。
/// 匹配新格式 `voice-claude.YYYY-MM-DD.log` + 老格式 `voice-claude.log.YYYY-MM-DD`
/// + 首版无后缀的 `voice-claude.log`。open_logs 和 read_recent_logs 共用。
fn find_latest_log() -> Option<std::path::PathBuf> {
    let dir = dirs::log_dir();
    let entries = std::fs::read_dir(&dir).ok()?;
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("voice-claude"))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mtime))
        })
        .max_by_key(|(_, t)| *t)
        .map(|(p, _)| p)
}

#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    match find_latest_log() {
        Some(p) => open_path(&p.to_string_lossy()),
        None => open_path(&dirs::log_dir().to_string_lossy()),
    }
}

#[tauri::command]
pub fn open_log_dir() -> Result<(), String> {
    open_path(&dirs::log_dir().to_string_lossy())
}

/// 导出整个 config.json 内容作为字符串（前端负责写盘）。
#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.snapshot();
    serde_json::to_string_pretty(&*cfg).map_err(|e| e.to_string())
}

/// 从 JSON 字符串导入整个 config。顺序：
///   1. 反序列化 + 预校验 hotkey 字符串（不实注册，避免后面 save 失败时热键已改）
///   2. save 到磁盘（可能 IO 失败）
///   3. register_hotkey（真正注册系统热键）
///   4. replace AppState + reload log + 广播
/// 任一步骤失败前都没改状态，失败后磁盘/内存/系统热键保持一致。
#[tauri::command]
pub fn import_config(
    json: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let new_cfg: Config =
        serde_json::from_str(&json).map_err(|e| format!("JSON 解析失败：{}", e))?;
    // 预校验 hotkey 语法（Rust hotkey::to_tauri_shortcut），先不注册
    crate::hotkey::to_tauri_shortcut(&new_cfg.hotkey)
        .map_err(|e| format!("导入的热键无效：{}", e))?;
    new_cfg.save().map_err(|e| e.to_string())?;
    crate::register_hotkey(&app, &new_cfg.hotkey)
        .map_err(|e| format!("导入的热键注册失败：{}", e))?;
    let new_log_level = new_cfg.log_level.clone();
    state.replace(new_cfg);
    crate::logger::reload(&new_log_level);
    let _ = app.emit("config-updated", ());
    Ok(())
}

/// 录入新快捷键前暂停系统级热键。否则用户按当前热键时会触发录音，
/// webview 拿不到 keydown 事件。
#[tauri::command]
pub fn suspend_hotkey(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    // 同步清 AppState.registered_hotkey，否则 resume 时 register_hotkey 会
    // 错以为还有旧的 accel 需要 unregister，产生多余 warn 日志
    *state.registered_hotkey.lock() = None;
    Ok(())
}

/// 录入取消后把系统热键恢复成当前 config 里的 hotkey。
/// 录入成功后 save_config 会自己 re-register 新的，这个 command 可不调。
#[tauri::command]
pub fn resume_hotkey(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.snapshot();
    crate::register_hotkey(&app, &cfg.hotkey).map_err(|e| e.to_string())
}

/// panel 输出模式下，悬浮窗里的 ✕ 按钮点击后调这个关窗口。
#[tauri::command]
pub fn close_indicator(app: AppHandle) -> Result<(), String> {
    let app_for_hide = app.clone();
    app.run_on_main_thread(move || {
        crate::indicator::hide(&app_for_hide);
    })
    .map_err(|e| e.to_string())
}

/// panel 输出模式的识别结果窗口关闭按钮。
#[tauri::command]
pub fn close_result_window(app: AppHandle) -> Result<(), String> {
    let app_for_hide = app.clone();
    app.run_on_main_thread(move || {
        crate::result::hide(&app_for_hide);
    })
    .map_err(|e| e.to_string())
}

/// 录音中取消本次(丢弃录音,不走 ASR/AI/输出)。ESC 全局热键外的兜底入口,
/// 比如悬浮窗 webview 获得焦点时本地 keydown 也可以 invoke 这个命令。
#[tauri::command]
pub fn cancel_recording() -> Result<(), String> {
    crate::recorder::cancel();
    Ok(())
}

/// 读取最新日志文件的最后 limit 行，返回给前端展示。
#[tauri::command]
pub fn read_recent_logs(limit: usize) -> Result<Vec<String>, String> {
    let Some(path) = find_latest_log() else {
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
