//! 系统托盘菜单。
//! 对应 Go 版的 tray.go。

use anyhow::Result;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};

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

    let _tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("voice-claude")
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .build(app)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle<Wry>, event: MenuEvent) {
    match event.id.as_ref() {
        "settings" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "history" => {
            if let Some(w) = app.get_webview_window("main") {
                // 简化：主窗口里根据 URL hash 切换到 history 页
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.eval("window.location.hash = '#/history'");
            }
        }
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
