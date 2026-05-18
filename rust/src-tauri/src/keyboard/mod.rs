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

    let (hotkey, dtm) = match mode {
        TriggerMode::Toggle | TriggerMode::PushToTalk => {
            let hk = config::parse_hotkey(&cfg.hotkey)?;
            (Some(hk), None)
        }
        TriggerMode::DoubleTapHold => {
            let m = config::parse_double_tap_modifier(&cfg.double_tap_modifier)?;
            (None, Some(m))
        }
    };

    Ok(BackendConfig {
        mode,
        hotkey,
        double_tap_modifier: dtm,
    })
}
