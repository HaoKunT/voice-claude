//! voice-claude Rust 后端
//!
//! 模块结构：
//! - dirs / logger: 平台相关基础设施
//! - config: 配置读写
//! - history: SQLite 历史
//! - hotwords: 热词替换
//! - hotkey: 热键字符串解析
//! - audio: cpal 录音
//! - input: enigo 键盘模拟
//! - correct: AI 纠错
//! - asr/*: 5 种 ASR 后端
//! - recorder: 主录音流程
//! - commands: Tauri IPC 命令
//! - tray: 系统托盘菜单

pub mod asr;
pub mod audio;
pub mod beep;
pub mod commands;
pub mod config;
pub mod correct;
pub mod dirs;
pub mod history;
pub mod hotkey;
pub mod hotwords;
pub mod indicator;
pub mod input;
pub mod logger;
pub mod recorder;
pub mod result;
pub mod tray;
pub mod vad;

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// 应用状态：配置 + 运行时可变数据。
pub struct AppState {
    pub config: Mutex<Arc<config::Config>>,
    pub registered_hotkey: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(cfg: config::Config) -> Self {
        Self {
            config: Mutex::new(Arc::new(cfg)),
            registered_hotkey: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> Arc<config::Config> {
        Arc::clone(&self.config.lock())
    }

    pub fn replace(&self, cfg: config::Config) {
        *self.config.lock() = Arc::new(cfg);
    }
}

/// 录音期间用的临时 ESC 全局热键 accelerator 字符串。
const CANCEL_HOTKEY: &str = "Escape";

/// 录音开始时调用:注册 ESC 为临时全局热键,按下即取消录音。
/// 若当前主热键本身就是 Escape(罕见),跳过注册避免冲突。
pub fn register_cancel_hotkey(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if state
        .registered_hotkey
        .lock()
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case(CANCEL_HOTKEY))
    {
        tracing::debug!("主热键即 ESC,跳过注册 ESC 取消热键");
        return;
    }
    let gs = app.global_shortcut();
    // 上次录音未能注销干净时兜底(比如 drop 期间异常),忽略错误
    let _ = gs.unregister(CANCEL_HOTKEY);
    let handle = app.clone();
    let result = gs.on_shortcut(CANCEL_HOTKEY, move |_app, _shortcut, event| {
        use tauri_plugin_global_shortcut::ShortcutState;
        if event.state() == ShortcutState::Pressed {
            tracing::info!("ESC 按下,取消录音");
            let _ = handle.emit("recording-cancelled", ());
            recorder::cancel();
        }
    });
    if let Err(e) = result {
        tracing::warn!(error = ?e, "注册 ESC 取消热键失败");
    }
}

/// 录音结束(正常停止或取消)时调用:注销 ESC 临时热键。
pub fn unregister_cancel_hotkey(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if state
        .registered_hotkey
        .lock()
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case(CANCEL_HOTKEY))
    {
        return;
    }
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister(CANCEL_HOTKEY) {
        tracing::debug!(error = ?e, "注销 ESC 热键(可能从未注册),忽略");
    }
}

/// 注册全局热键。先 unregister_all 清掉旧的，再 on_shortcut 绑新的。
/// 启动时和 save_config 里 hotkey 变更时都调用。
pub fn register_hotkey(app: &tauri::AppHandle, hotkey_str: &str) -> anyhow::Result<()> {
    let accel = hotkey::to_tauri_shortcut(hotkey_str)
        .map_err(|e| anyhow::anyhow!("热键解析失败：{}", e))?;
    let gs = app.global_shortcut();
    let handle = app.clone();
    gs.on_shortcut(accel.as_str(), move |_app, _shortcut, event| {
        use tauri_plugin_global_shortcut::ShortcutState;
        let state = handle.state::<AppState>();
        let cfg = state.snapshot();
        match event.state() {
            ShortcutState::Pressed => {
                if cfg.push_to_talk {
                    recorder::start(handle.clone(), cfg);
                } else {
                    recorder::toggle(handle.clone(), cfg);
                }
            }
            ShortcutState::Released => {
                if cfg.push_to_talk {
                    recorder::stop();
                }
            }
        }
    })
    .map_err(|e| anyhow::anyhow!("注册热键失败：{}", e))?;
    let state = app.state::<AppState>();
    let prev = state.registered_hotkey.lock().clone();
    if let Some(prev) = prev {
        if prev != accel {
            if let Err(e) = gs.unregister(prev.as_str()) {
                tracing::warn!(error = ?e, "注销旧热键失败，忽略");
            }
        }
    }
    *state.registered_hotkey.lock() = Some(accel.clone());
    tracing::info!(hotkey = %accel, "热键已注册");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::Config::load();
    let _log_guard = logger::init(&cfg.log_level);
    if let Err(e) = history::init() {
        tracing::warn!(error = ?e, "历史数据库初始化失败");
    }

    let hotkey_str = cfg.hotkey.clone();
    let state = AppState::new(cfg);

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .manage(state)
        .on_window_event(|window, event| {
            // 关闭主窗口时：隐藏窗口 + 切回 Accessory 让 app 从 Dock 消失
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                    #[cfg(target_os = "macos")]
                    {
                        use tauri::{ActivationPolicy, Manager};
                        let _ = window
                            .app_handle()
                            .set_activation_policy(ActivationPolicy::Accessory);
                    }
                }
            }
        })
        .setup(move |app| {
            // macOS：Accessory 模式，app 不占 Dock，NSPanel 才能真·不抢焦点。
            // 点击 tray 菜单"设置"时会临时切到 Regular 再显示主窗口（见 tray.rs）。
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
            }
            // 注册全局热键（抽成独立函数，save_config 里也会调）
            if let Err(e) = register_hotkey(app.handle(), &hotkey_str) {
                tracing::error!(error = ?e, "启动时注册热键失败");
            }

            tray::setup(app.handle())?;

            // 预创建 indicator NSPanel：启动时一次性创建 + swizzle 成 NSPanel，
            // 后续录音只 show/hide，永远不再重新创建窗口，避免 WebView 初始化期间抢焦点。
            indicator::prebuild(app.handle());

            // 预创建 result 普通窗口：panel 输出模式下展示识别结果给用户编辑。
            // 必须是普通 WebviewWindow（非 NSPanel），否则 IME 候选窗挂不上。
            result::prebuild(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_devices,
            commands::load_history,
            commands::delete_history,
            commands::clear_history,
            commands::get_history_stats,
            commands::get_latency_stats,
            commands::repolish_history,
            commands::check_ollama,
            commands::open_logs,
            commands::open_log_dir,
            commands::close_indicator,
            commands::close_result_window,
            commands::cancel_recording,
            commands::suspend_hotkey,
            commands::resume_hotkey,
            commands::read_recent_logs,
            commands::open_config_dir,
            commands::is_sense_voice_available,
            commands::get_sense_voice_info,
            commands::download_sense_voice,
            commands::import_sense_voice_tarball,
            commands::get_app_info,
            commands::export_hotwords_csv,
            commands::import_hotwords_csv,
            commands::export_config,
            commands::import_config,
            commands::check_accessibility,
            commands::open_accessibility_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
