//! voice-claude Rust 后端
//!
//! 模块结构：
//! - dirs / logger: 平台相关基础设施
//! - config: 配置读写
//! - history: SQLite 历史
//! - hotwords: 热词替换
//! - keyboard: 跨平台 keyboard backend(handy-keys 包装) + 三模式状态机
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
pub mod hotwords;
pub mod indicator;
pub mod input;
pub mod keyboard;
pub mod logger;
pub mod recorder;
pub mod result;
pub mod tray;
pub mod vad;

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::Manager;

use crate::keyboard::KeyboardBackend;

/// 应用状态：配置 + 运行时可变数据。
pub struct AppState {
    pub config: Mutex<Arc<config::Config>>,
    /// 跨平台 keyboard backend(handy-keys 包装)。supervisor 线程在 backend 内
    /// 持有,Drop 时自动 shutdown + join。Some/None 区分"已启动"和"录键 widget
    /// 临时挂起"两种状态。
    pub keyboard: Mutex<Option<KeyboardBackend>>,
}

impl AppState {
    pub fn new(cfg: config::Config) -> Self {
        Self {
            config: Mutex::new(Arc::new(cfg)),
            keyboard: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> Arc<config::Config> {
        Arc::clone(&self.config.lock())
    }

    pub fn replace(&self, cfg: config::Config) {
        *self.config.lock() = Arc::new(cfg);
    }
}

/// 启动或热重载 keyboard backend(supervisor + handy-keys 实例)。
///
/// - 没启动过 → `KeyboardBackend::start` 起线程并立刻 probe 注册一次确认无误
/// - 已启动 → 发 `Reload` 给 supervisor,旧 HotkeyManager Drop 自动卸 OS hook
///
/// 启动时(setup)和 save_config 里 hotkey/trigger_mode 变更时都调用。
pub fn start_or_reload_keyboard(
    app: &tauri::AppHandle,
    cfg: &config::Config,
) -> anyhow::Result<()> {
    let bcfg = keyboard::backend_config_from(cfg)?;
    let state = app.state::<AppState>();
    let mut slot = state.keyboard.lock();
    match slot.as_ref() {
        Some(kb) => kb.reload(bcfg),
        None => {
            let kb = KeyboardBackend::start(app.clone(), bcfg)?;
            *slot = Some(kb);
            Ok(())
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::Config::load();
    let _log_guard = logger::init(&cfg.log_level);
    if let Err(e) = history::init() {
        tracing::warn!(error = ?e, "历史数据库初始化失败");
    }

    // setup 闭包里需要一份 cfg 副本:① 后台预热当前选中的本地引擎,避免用户
    // 启动后第一次按热键触发 4-5s 冷启动(尤其 FireRed/Qwen3 这种大模型)
    // ② 给 keyboard backend 启动用(它需要解析 hotkey + 推断 trigger_mode)
    let cfg_for_warm = cfg.clone();
    let cfg_for_keyboard = cfg.clone();
    let state = AppState::new(cfg);

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

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
            // 启动跨平台 keyboard backend(handy-keys + supervisor 线程)
            // save_config 里 hotkey 变更时也走同一函数(reload 路径)
            if let Err(e) = start_or_reload_keyboard(app.handle(), &cfg_for_keyboard) {
                tracing::error!(error = ?e, "启动时启动 keyboard backend 失败");
            }

            tray::setup(app.handle())?;

            // 预创建 indicator NSPanel：启动时一次性创建 + swizzle 成 NSPanel，
            // 后续录音只 show/hide，永远不再重新创建窗口，避免 WebView 初始化期间抢焦点。
            indicator::prebuild(app.handle());

            // 预创建 result 普通窗口：panel 输出模式下展示识别结果给用户编辑。
            // 必须是普通 WebviewWindow（非 NSPanel），否则 IME 候选窗挂不上。
            result::prebuild(app.handle());

            // 后台预热当前选中的本地引擎 —— 避免首次按热键时 4-5s 冷启动卡顿。
            // 仅 cfg.asr_provider == "local" 时才有意义;非 local 后端没什么可预热。
            // spawn_blocking 走线程池,不阻塞 setup 也不影响 tokio runtime 调度。
            let warm_cfg = cfg_for_warm;
            if warm_cfg.asr_provider == config::ASR_PROVIDER_LOCAL {
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = asr::local::warm_up(&warm_cfg) {
                        tracing::warn!(error = ?e, "启动预热失败");
                    }
                });
            }

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
            commands::list_local_engines,
            commands::get_local_engine_info,
            commands::download_local_engine,
            commands::import_local_engine_tarball,
            commands::get_punct_model_info,
            commands::download_punct_model,
            commands::import_punct_model_tarball,
            commands::bench_transcribe_file,
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
