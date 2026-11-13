//! 跨平台 keyboard backend(基于 handy-keys 0.2),替换原 tauri-plugin-global-shortcut。
//!
//! 子模块:
//! - `config`:voice-claude 字符串(`"cmd+shift+f5"`/`"right_option"`) ↔ handy-keys
//!   的 `Hotkey/Modifiers` 转换层
//! - `state_machine`:三模式状态机(Toggle / PushToTalk / DoubleTapHold)
//! - `backend`:supervisor 线程,跑 handy-keys 实例 + 状态机循环 + 热重载

pub mod backend;
pub mod config;
pub mod state_machine;

pub use backend::{BackendConfig, KeyboardBackend};
pub use state_machine::{TriggerEvent, TriggerMode};

/// 把 `crate::config::Config` 解析成 supervisor 用的 `BackendConfig`。
///
/// 解析顺序:优先看 `trigger_mode` 字段,缺省时(老 config 升级)fallback 到
/// `push_to_talk` bool 推断。这样 0.2.x 用户的 config 加载后行为不变。
pub fn backend_config_from(cfg: &crate::config::Config) -> anyhow::Result<BackendConfig> {
    backend_config_from_impl(cfg, false)
}

/// 前端快捷键「⌨ 录入」期间的专用变体:进入 capture 模式。
///
/// capture 模式下 backend 仍然活着(钩子不卸载),但所有触发事件被丢弃,
/// 并且在 Windows 上额外注册 Alt+Space 为「吞掉」热键 —— 否则用户在录入时
/// 按下 Alt+Space,系统窗口菜单(还原/移动/最小化)会弹出,录入界面什么都
/// 收不到,看起来像"录入坏了"。macOS 没有这个保留组合,所以只在 Windows
/// 做。代价是:录入期间 Alt+Space 无法被捕获(OS 层就被吞了),需要用户
/// 手动在输入框输入 "alt+space"(前端会给出提示)。
///
/// 仅 Windows 编译:suspend_hotkey 在 macOS 上保持原来的完全卸载语义
/// (capture 模式会把主热键拦在 OS 层,录入界面收不到旧热键的 keydown,
/// 「重新录入当前热键」会静默失败)。macOS 下此函数无人调用,不 gate 的
/// 话会变成 dead code,clippy -D warnings 直接失败。
#[cfg(target_os = "windows")]
pub fn capture_backend_config_from(cfg: &crate::config::Config) -> anyhow::Result<BackendConfig> {
    backend_config_from_impl(cfg, true)
}

fn backend_config_from_impl(
    cfg: &crate::config::Config,
    capture_mode: bool,
) -> anyhow::Result<BackendConfig> {
    use crate::config::{TRIGGER_MODE_DOUBLE_TAP_HOLD, TRIGGER_MODE_PTT, TRIGGER_MODE_TOGGLE};

    // trigger_mode 缺省 / 是 toggle 时,看老 push_to_talk 兼容路径
    let mode = match cfg.trigger_mode.as_str() {
        TRIGGER_MODE_DOUBLE_TAP_HOLD => TriggerMode::DoubleTapHold,
        TRIGGER_MODE_PTT => TriggerMode::PushToTalk,
        TRIGGER_MODE_TOGGLE => {
            if cfg.push_to_talk {
                // 老 config:trigger_mode 缺省了 default 出 toggle,但 push_to_talk = true
                TriggerMode::PushToTalk
            } else {
                TriggerMode::Toggle
            }
        }
        other => {
            tracing::warn!(value = %other, "未知 trigger_mode,fallback 到 toggle");
            TriggerMode::Toggle
        }
    };

    // capture 模式永远走 manager 路径(listener 路径没有「吞热键」能力),
    // 主热键也照常注册 —— 让它被拦住,避免录入期间按老热键真的触发录音。
    let (hotkey, dtm) = if capture_mode {
        (Some(config::parse_hotkey(&cfg.hotkey)?), None)
    } else {
        match mode {
            TriggerMode::Toggle | TriggerMode::PushToTalk => {
                let hk = config::parse_hotkey(&cfg.hotkey)?;
                (Some(hk), None)
            }
            TriggerMode::DoubleTapHold => {
                let m = config::parse_double_tap_modifier(&cfg.double_tap_modifier)?;
                (None, Some(m))
            }
        }
    };
    let mode = if capture_mode {
        TriggerMode::Toggle
    } else {
        mode
    };

    Ok(BackendConfig {
        mode,
        hotkey,
        double_tap_modifier: dtm,
        capture_mode,
    })
}
