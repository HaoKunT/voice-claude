//! 热键字符串解析：把用户配置的 "cmd+shift+f5" 转成 tauri-plugin-global-shortcut 可接受的格式。
//! 对应 Go 版的 hotkey.go。

use anyhow::{bail, Result};

/// 把用户风格的热键字符串（如 "cmd+shift+f5"、"ctrl+alt+space"）
/// 标准化成 tauri shortcut 字符串（如 "CommandOrControl+Shift+F5"）。
///
/// - cmd / command / rcmd / rcommand → CommandOrControl
/// - ctrl / rctrl → Control
/// - option / alt / ralt / roption → Alt
/// - shift / rshift → Shift
/// - win / super / meta → Super（Windows 键 / Super 键）
/// - 主键（a-z, 0-9, f1-f24, space/tab/...）保持字母并首字母大写
pub fn to_tauri_shortcut(input: &str) -> Result<String> {
    let parts: Vec<&str> = input.trim().split('+').map(str::trim).collect();
    if parts.len() < 2 {
        bail!("热键格式应为 mod+key，如 cmd+shift+f5");
    }

    let mut mods: Vec<&str> = Vec::new();
    let mut key: Option<String> = None;

    for raw in parts {
        let p = raw.to_lowercase();
        match p.as_str() {
            "cmd" | "command" | "rcmd" | "rcommand" => push_unique(&mut mods, "CommandOrControl"),
            "ctrl" | "control" | "rctrl" => push_unique(&mut mods, "Control"),
            "option" | "alt" | "ralt" | "roption" => push_unique(&mut mods, "Alt"),
            "shift" | "rshift" => push_unique(&mut mods, "Shift"),
            "win" | "super" | "meta" => push_unique(&mut mods, "Super"),
            other => {
                if other.is_empty() {
                    bail!("热键段为空");
                }
                key = Some(normalize_key(other)?);
            }
        }
    }

    let key = key.ok_or_else(|| anyhow::anyhow!("需要一个主键（如 space、a-z）"))?;
    #[cfg(target_os = "windows")]
    {
        if mods.contains(&"Super") && key == "Space" {
            bail!("win+space 是 Windows 系统保留快捷键（输入法/语言切换），无法注册");
        }
    }
    mods.push(&key);
    Ok(mods.join("+"))
}

fn push_unique(mods: &mut Vec<&str>, m: &'static str) {
    if !mods.contains(&m) {
        mods.push(m);
    }
}

fn normalize_key(p: &str) -> Result<String> {
    // 单个字母 / 数字
    if p.len() == 1 {
        let c = p.chars().next().unwrap();
        if c.is_ascii_alphanumeric() {
            return Ok(c.to_ascii_uppercase().to_string());
        }
    }
    // F 键
    if let Some(n) = p.strip_prefix('f') {
        if let Ok(num) = n.parse::<u8>() {
            if (1..=24).contains(&num) {
                return Ok(format!("F{}", num));
            }
        }
    }
    // 特殊键
    match p {
        "space" => Ok("Space".into()),
        "return" | "enter" => Ok("Enter".into()),
        "tab" => Ok("Tab".into()),
        "esc" | "escape" => Ok("Escape".into()),
        "delete" => Ok("Delete".into()),
        "backspace" => Ok("Backspace".into()),
        other => {
            bail!("未知按键: {}", other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_combo() {
        assert_eq!(
            to_tauri_shortcut("cmd+shift+f5").unwrap(),
            "CommandOrControl+Shift+F5"
        );
    }

    #[test]
    fn parses_with_option() {
        assert_eq!(
            to_tauri_shortcut("ctrl+alt+space").unwrap(),
            "Control+Alt+Space"
        );
    }

    #[test]
    fn normalizes_rmod_to_same() {
        // 修饰键顺序按用户输入保留，主键在最后
        assert_eq!(
            to_tauri_shortcut("rshift+rcmd+a").unwrap(),
            "Shift+CommandOrControl+A"
        );
        assert_eq!(
            to_tauri_shortcut("rcmd+rshift+a").unwrap(),
            "CommandOrControl+Shift+A"
        );
    }

    #[test]
    fn parses_super_combo() {
        assert_eq!(to_tauri_shortcut("win+shift+f5").unwrap(), "Super+Shift+F5");
    }

    #[test]
    fn rejects_no_key() {
        assert!(to_tauri_shortcut("cmd+shift").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(to_tauri_shortcut("").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(to_tauri_shortcut("cmd+XYZ").is_err());
    }

    #[test]
    fn fkey_range() {
        assert_eq!(
            to_tauri_shortcut("cmd+f12").unwrap(),
            "CommandOrControl+F12"
        );
        assert!(to_tauri_shortcut("cmd+f99").is_err());
    }
}
