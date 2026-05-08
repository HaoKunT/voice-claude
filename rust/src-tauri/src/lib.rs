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
pub mod commands;
pub mod config;
pub mod correct;
pub mod dirs;
pub mod history;
pub mod hotkey;
pub mod hotwords;
pub mod input;
pub mod logger;
pub mod recorder;
pub mod tray;

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// 应用状态：配置 + 运行时可变数据。
pub struct AppState {
    pub config: Mutex<Arc<config::Config>>,
}

impl AppState {
    pub fn new(cfg: config::Config) -> Self {
        Self {
            config: Mutex::new(Arc::new(cfg)),
        }
    }

    pub fn snapshot(&self) -> Arc<config::Config> {
        Arc::clone(&self.config.lock())
    }

    pub fn replace(&self, cfg: config::Config) {
        *self.config.lock() = Arc::new(cfg);
    }
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();

            // 注册全局热键
            match hotkey::to_tauri_shortcut(&hotkey_str) {
                Ok(accel) => {
                    let handle2 = handle.clone();
                    let gs = app.global_shortcut();
                    if let Err(e) = gs.on_shortcut(accel.as_str(), move |_app, _shortcut, event| {
                        use tauri_plugin_global_shortcut::ShortcutState;
                        if event.state() == ShortcutState::Pressed {
                            let state = handle2.state::<AppState>();
                            let cfg = state.snapshot();
                            recorder::toggle(handle2.clone(), cfg);
                        }
                    }) {
                        tracing::error!(error = ?e, hotkey = %accel, "热键监听失败");
                    } else {
                        tracing::info!(hotkey = %accel, "热键已注册");
                    }
                }
                Err(e) => tracing::error!(error = ?e, "热键解析失败"),
            }

            tray::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_devices,
            commands::load_history,
            commands::delete_history,
            commands::clear_history,
            commands::check_ollama,
            commands::open_logs,
            commands::open_config_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
