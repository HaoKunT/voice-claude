//! 系统托盘菜单。
//! 对应 Go 版的 tray.go。

use anyhow::Result;
use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};

/// 嵌入的托盘图标（32x32 模板图像）
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");

pub fn setup(app: &AppHandle<Wry>) -> Result<()> {
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "历史记录", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, "logs", "打开日志", true, None::<&str>)?;
    let config_dir = MenuItem::with_id(app, "config_dir", "打开配置目录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &settings,
            &history,
            &logs,
            &config_dir,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

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

fn handle_menu_event(app: &AppHandle<Wry>, event: MenuEvent) {
    match event.id.as_ref() {
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
