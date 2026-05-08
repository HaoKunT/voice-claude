//! 系统托盘菜单。
//! 对应 Go 版的 tray.go，包含最近识别结果快捷复制。

use anyhow::Result;
use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};

/// 嵌入的托盘图标（32x32 模板图像）
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");

/// 最近识别结果显示数量
const RECENT_COUNT: usize = 5;

/// 复制某条历史 id 的文本到剪贴板（menu event id 前缀）
const RECENT_COPY_PREFIX: &str = "recent:";

pub fn setup(app: &AppHandle<Wry>) -> Result<()> {
    let menu = build_menu(app)?;
    let icon = Image::from_bytes(TRAY_ICON_PNG)?;
    let _tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("voice-claude")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .build(app)?;
    Ok(())
}

/// 根据最新的历史记录重建 tray 菜单（recorder 在每次识别完成后调）。
pub fn refresh(app: &AppHandle<Wry>) -> Result<()> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let menu = build_menu(app)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

fn build_menu(app: &AppHandle<Wry>) -> Result<Menu<Wry>> {
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "历史记录", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, "logs", "打开日志", true, None::<&str>)?;
    let config_dir = MenuItem::with_id(app, "config_dir", "打开配置目录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    // 最近 RECENT_COUNT 条识别结果（点击复制到剪贴板）
    let recent = crate::history::load(RECENT_COUNT as i64).unwrap_or_default();
    let mut recent_items: Vec<MenuItem<Wry>> = Vec::with_capacity(recent.len());
    for entry in &recent {
        let label = truncate(&entry.corrected_text, 40);
        let id = format!("{}{}", RECENT_COPY_PREFIX, entry.id);
        let item = MenuItem::with_id(app, &id, &label, true, None::<&str>)?;
        recent_items.push(item);
    }

    // 拼装菜单：操作项 → 分隔 → 最近结果 → 分隔 → 退出
    let sep1 = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        vec![&settings, &history, &logs, &config_dir, &sep1];

    // 最近识别（可选分组标题）
    let recent_header = MenuItem::with_id(app, "recent_header", "最近识别", false, None::<&str>)?;
    items.push(&recent_header);
    if recent_items.is_empty() {
        let empty = MenuItem::with_id(app, "recent_empty", "  暂无记录", false, None::<&str>)?;
        recent_items.push(empty);
    }
    for it in &recent_items {
        items.push(it);
    }

    let sep2 = PredefinedMenuItem::separator(app)?;
    items.push(&sep2);
    items.push(&quit);

    Menu::with_items(app, &items).map_err(Into::into)
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = chars.iter().take(max_chars).collect();
        format!("{}…", truncated)
    }
}

fn handle_menu_event(app: &AppHandle<Wry>, event: MenuEvent) {
    let id = event.id.as_ref();
    // 最近识别结果：copy 到剪贴板
    if let Some(hist_id_str) = id.strip_prefix(RECENT_COPY_PREFIX) {
        if let Ok(hist_id) = hist_id_str.parse::<i64>() {
            copy_recent(app, hist_id);
        }
        return;
    }
    match id {
        "settings" => show_main(app, None),
        "history" => show_main(app, Some("#/history")),
        "logs" => {
            let _ = open_path(&crate::dirs::log_file_path().to_string_lossy());
        }
        "config_dir" => {
            let _ = open_path(&crate::dirs::config_dir().to_string_lossy());
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

/// 找到指定 id 的历史记录，复制 corrected_text 到剪贴板。
fn copy_recent(app: &AppHandle<Wry>, hist_id: i64) {
    let entries = crate::history::load(RECENT_COUNT as i64).unwrap_or_default();
    if let Some(entry) = entries.into_iter().find(|e| e.id == hist_id) {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        if let Err(e) = app.clipboard().write_text(entry.corrected_text.clone()) {
            tracing::warn!(error = ?e, "复制到剪贴板失败");
        } else {
            tracing::info!(text = %entry.corrected_text, "已复制到剪贴板");
        }
    }
}

/// macOS Accessory 模式下 app 本身是隐藏的，点击 tray 菜单拉起主窗口时需要：
/// 1. 临时切回 Regular（否则窗口出不来或抢不到焦点）
/// 2. show + set_focus
/// 3. 评估可选的 url hash 路由
fn show_main(app: &AppHandle<Wry>, hash: Option<&str>) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let _ = app.set_activation_policy(ActivationPolicy::Regular);
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(h) = hash {
            let _ = w.eval(format!("window.location.hash = '{}'", h));
        }
    }
}

fn open_path(path: &str) -> Result<()> {
    use std::process::Command;
    if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()?;
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()?;
    } else {
        Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}
