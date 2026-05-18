//! voice-claude 风格热键字符串 ↔ handy-keys 类型的转换层。
//!
//! 替换原 `crate::hotkey::to_tauri_shortcut`(那个产 tauri-plugin-global-shortcut 的字符串,
//! 这里换成 handy-keys 的 `Hotkey` 结构体)。语义对齐原版本:
//! - 平台抽象:macOS 下 `cmd` → `CMD`;非 macOS 下 `cmd` → `CTRL`(对齐
//!   tauri 的 `CommandOrControl` 老语义,老用户配置不变就能继续用)
//! - 左右修饰键:`rcmd`/`rshift`/`ralt` 等映射到 `*_RIGHT` 变体
//! - F 键范围:F1-F20(handy-keys 上限,原 1..=24 已不再合理 —— F21+ 真实
//!   键盘几乎不存在,且 handy-keys 上游有未合 issue,先收紧到 F20)
//! - Backspace/Delete 命名:handy-keys 用 macOS 风格 —— `Key::Delete` 是 ⌫(退格),
//!   `Key::ForwardDelete` 是 ⌦(真删除)。voice-claude 老格式 `"backspace"` →
//!   `Key::Delete`,老格式 `"delete"` → `Key::ForwardDelete`,符合用户直觉。

use anyhow::{anyhow, bail, Result};
use handy_keys::{Hotkey, Key, Modifiers};

/// 解析 voice-claude 风格的主热键字符串(`"cmd+shift+f5"`)成 handy-keys 的 `Hotkey`。
///
/// 至少一个修饰键 + 一个主键。返回的 `Hotkey` 直接喂 `HotkeyManager::register`。
pub fn parse_hotkey(input: &str) -> Result<Hotkey> {
    let parts: Vec<&str> = input.trim().split('+').map(str::trim).collect();
    if parts.len() < 2 {
        bail!("热键格式应为 mod+key,如 cmd+shift+f5");
    }

    let mut modifiers = Modifiers::empty();
    let mut key: Option<Key> = None;

    for raw in parts {
        let p = raw.to_lowercase();
        if let Some(m) = parse_modifier_token(&p) {
            modifiers |= m;
            continue;
        }
        if p.is_empty() {
            bail!("热键段为空");
        }
        if key.is_some() {
            bail!("只允许一个主键");
        }
        key = Some(parse_key(&p)?);
    }

    let key = key.ok_or_else(|| anyhow!("需要一个主键(如 space、a-z)"))?;

    #[cfg(target_os = "windows")]
    {
        // win+space 是 Windows 系统级输入法切换,平台留给系统占用,我们注册不上。
        if modifiers.contains(win_super()) && key == Key::Space {
            bail!("win+space 是 Windows 系统保留快捷键(输入法/语言切换),无法注册");
        }
    }

    Hotkey::new(modifiers, key).map_err(|e| anyhow!("{}", e))
}

/// 解析 `cfg.double_tap_modifier` 字段(`"right_option"` / `"left_ctrl"` 等)成单个
/// `Modifiers` bit。值跟 handy-keys 的 `to_handy_string` 风格对齐(下划线分词)。
pub fn parse_double_tap_modifier(input: &str) -> Result<Modifiers> {
    let p = input.trim().to_lowercase();
    let m = match p.as_str() {
        "left_cmd" | "left_command" => Modifiers::CMD_LEFT,
        "right_cmd" | "right_command" => Modifiers::CMD_RIGHT,
        "left_option" | "left_alt" => Modifiers::OPT_LEFT,
        "right_option" | "right_alt" => Modifiers::OPT_RIGHT,
        "left_ctrl" | "left_control" => Modifiers::CTRL_LEFT,
        "right_ctrl" | "right_control" => Modifiers::CTRL_RIGHT,
        "left_shift" => Modifiers::SHIFT_LEFT,
        "right_shift" => Modifiers::SHIFT_RIGHT,
        #[cfg(target_os = "macos")]
        "fn" => Modifiers::FN,
        other => bail!("不支持的双击 modifier: {}", other),
    };
    Ok(m)
}

fn parse_modifier_token(p: &str) -> Option<Modifiers> {
    match p {
        "cmd" | "command" => Some(cmd_or_ctrl_both()),
        "rcmd" | "rcommand" => Some(cmd_or_ctrl_right()),
        "ctrl" | "control" => Some(Modifiers::CTRL),
        "rctrl" => Some(Modifiers::CTRL_RIGHT),
        "option" | "alt" => Some(Modifiers::OPT),
        "ralt" | "roption" => Some(Modifiers::OPT_RIGHT),
        "shift" => Some(Modifiers::SHIFT),
        "rshift" => Some(Modifiers::SHIFT_RIGHT),
        "win" | "super" | "meta" => Some(win_super()),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn cmd_or_ctrl_both() -> Modifiers {
    Modifiers::CMD
}
#[cfg(target_os = "macos")]
fn cmd_or_ctrl_right() -> Modifiers {
    Modifiers::CMD_RIGHT
}
#[cfg(not(target_os = "macos"))]
fn cmd_or_ctrl_both() -> Modifiers {
    Modifiers::CTRL
}
#[cfg(not(target_os = "macos"))]
fn cmd_or_ctrl_right() -> Modifiers {
    Modifiers::CTRL_RIGHT
}

#[cfg(target_os = "windows")]
fn win_super() -> Modifiers {
    // handy-keys 在 Windows 把 Windows logo 键映射到 CMD bit。
    Modifiers::CMD
}
#[cfg(not(target_os = "windows"))]
fn win_super() -> Modifiers {
    // 其他平台没有 Win/Super 键的"标准"概念,语义降级为 CMD —— 跟原 tauri
    // shortcut 字符串 "Super" 对齐(老配置含 win/super 的虽然在 macOS 几乎
    // 没意义,但解析不报错)
    Modifiers::CMD
}

fn parse_key(p: &str) -> Result<Key> {
    if p.len() == 1 {
        let c = p.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Ok(letter_key(c));
        }
        if c.is_ascii_digit() {
            return Ok(digit_key(c));
        }
    }
    if let Some(n) = p.strip_prefix('f') {
        if let Ok(num) = n.parse::<u8>() {
            return f_key(num);
        }
    }
    match p {
        "space" => Ok(Key::Space),
        "return" | "enter" => Ok(Key::Return),
        "tab" => Ok(Key::Tab),
        "esc" | "escape" => Ok(Key::Escape),
        // handy-keys 用 macOS 风格命名:Delete=⌫(退格),ForwardDelete=⌦
        "delete" => Ok(Key::ForwardDelete),
        "backspace" => Ok(Key::Delete),
        other => bail!("未知按键: {}", other),
    }
}

fn letter_key(c: char) -> Key {
    match c.to_ascii_lowercase() {
        'a' => Key::A,
        'b' => Key::B,
        'c' => Key::C,
        'd' => Key::D,
        'e' => Key::E,
        'f' => Key::F,
        'g' => Key::G,
        'h' => Key::H,
        'i' => Key::I,
        'j' => Key::J,
        'k' => Key::K,
        'l' => Key::L,
        'm' => Key::M,
        'n' => Key::N,
        'o' => Key::O,
        'p' => Key::P,
        'q' => Key::Q,
        'r' => Key::R,
        's' => Key::S,
        't' => Key::T,
        'u' => Key::U,
        'v' => Key::V,
        'w' => Key::W,
        'x' => Key::X,
        'y' => Key::Y,
        'z' => Key::Z,
        _ => unreachable!("letter_key 调用前已校验 is_ascii_alphabetic"),
    }
}

fn digit_key(c: char) -> Key {
    match c {
        '0' => Key::Num0,
        '1' => Key::Num1,
        '2' => Key::Num2,
        '3' => Key::Num3,
        '4' => Key::Num4,
        '5' => Key::Num5,
        '6' => Key::Num6,
        '7' => Key::Num7,
        '8' => Key::Num8,
        '9' => Key::Num9,
        _ => unreachable!("digit_key 调用前已校验 is_ascii_digit"),
    }
}

fn f_key(num: u8) -> Result<Key> {
    Ok(match num {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        _ => bail!("F 键超出范围: F{}(支持 F1-F20)", num),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_combo() {
        let hk = parse_hotkey("cmd+shift+f5").unwrap();
        assert_eq!(hk.key, Some(Key::F5));
        assert!(hk.modifiers.contains(Modifiers::SHIFT));
        #[cfg(target_os = "macos")]
        assert!(hk.modifiers.contains(Modifiers::CMD));
        #[cfg(not(target_os = "macos"))]
        assert!(hk.modifiers.contains(Modifiers::CTRL));
    }

    #[test]
    fn parses_with_option() {
        let hk = parse_hotkey("ctrl+alt+space").unwrap();
        assert_eq!(hk.key, Some(Key::Space));
        assert!(hk.modifiers.contains(Modifiers::CTRL));
        assert!(hk.modifiers.contains(Modifiers::OPT));
    }

    #[test]
    fn rmod_uses_right_variant() {
        let hk = parse_hotkey("rshift+rcmd+a").unwrap();
        assert_eq!(hk.key, Some(Key::A));
        assert!(hk.modifiers.contains(Modifiers::SHIFT_RIGHT));
        assert!(!hk.modifiers.contains(Modifiers::SHIFT_LEFT));
        #[cfg(target_os = "macos")]
        {
            assert!(hk.modifiers.contains(Modifiers::CMD_RIGHT));
            assert!(!hk.modifiers.contains(Modifiers::CMD_LEFT));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert!(hk.modifiers.contains(Modifiers::CTRL_RIGHT));
            assert!(!hk.modifiers.contains(Modifiers::CTRL_LEFT));
        }
    }

    #[test]
    fn parses_super_combo() {
        let hk = parse_hotkey("win+shift+f5").unwrap();
        assert_eq!(hk.key, Some(Key::F5));
        assert!(hk.modifiers.contains(Modifiers::SHIFT));
        // win → CMD bit(Windows logo / 其他平台 fallback 也是 CMD)
        assert!(hk.modifiers.contains(Modifiers::CMD));
    }

    #[test]
    fn rejects_no_key() {
        assert!(parse_hotkey("cmd+shift").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_hotkey("").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_hotkey("cmd+XYZ").is_err());
    }

    #[test]
    fn fkey_range() {
        assert_eq!(parse_hotkey("cmd+f12").unwrap().key, Some(Key::F12));
        assert_eq!(parse_hotkey("cmd+f20").unwrap().key, Some(Key::F20));
        // F21+ 不再支持(原版本允许 1..=24,但 handy-keys 上限 F20)
        assert!(parse_hotkey("cmd+f21").is_err());
        assert!(parse_hotkey("cmd+f99").is_err());
    }

    #[test]
    fn double_tap_modifier_parses_canonical_names() {
        assert_eq!(
            parse_double_tap_modifier("right_option").unwrap(),
            Modifiers::OPT_RIGHT
        );
        assert_eq!(
            parse_double_tap_modifier("left_option").unwrap(),
            Modifiers::OPT_LEFT
        );
        assert_eq!(
            parse_double_tap_modifier("right_ctrl").unwrap(),
            Modifiers::CTRL_RIGHT
        );
        assert_eq!(
            parse_double_tap_modifier("right_shift").unwrap(),
            Modifiers::SHIFT_RIGHT
        );
        // alt/option 同义
        assert_eq!(
            parse_double_tap_modifier("right_alt").unwrap(),
            Modifiers::OPT_RIGHT
        );
    }

    #[test]
    fn double_tap_modifier_rejects_unknown() {
        assert!(parse_double_tap_modifier("right_super").is_err());
        assert!(parse_double_tap_modifier("").is_err());
    }

    #[test]
    fn delete_and_backspace_map_correctly() {
        // voice-claude 老语义:"backspace" = ⌫,"delete" = ⌦
        assert_eq!(
            parse_hotkey("cmd+backspace").unwrap().key,
            Some(Key::Delete)
        );
        assert_eq!(
            parse_hotkey("cmd+delete").unwrap().key,
            Some(Key::ForwardDelete)
        );
    }

    #[test]
    fn duplicate_main_key_rejected() {
        // a + b 同时给两个主键应当报错
        assert!(parse_hotkey("cmd+a+b").is_err());
    }
}
