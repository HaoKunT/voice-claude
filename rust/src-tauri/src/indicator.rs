//! 录音指示器悬浮窗。
//!
//! macOS：用 Tauri WebView 窗口 + macos-private-api 做浮动 + always-on-top。
//! 真·非激活 panel 需要 tauri-nspanel 插件，在后续 iteration 引入。
//! Windows：用 WebView 窗口 + `set_skip_taskbar` + `set_always_on_top`。

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "indicator";
const W: f64 = 520.0;
const H: f64 = 140.0;

/// 显示或创建悬浮指示器。
pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
        return;
    }

    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("indicator.html".into()))
        .title("")
        .inner_size(W, H)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .focused(false)
        .visible_on_all_workspaces(true);

    match builder.build() {
        Ok(w) => {
            if let Err(e) = center(&w) {
                tracing::warn!(error = ?e, "悬浮窗居中失败");
            }
            let _ = w.show();
        }
        Err(e) => {
            tracing::error!(error = ?e, "创建悬浮窗失败");
        }
    }
}

/// 关闭悬浮指示器。
pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

fn center(w: &tauri::WebviewWindow) -> tauri::Result<()> {
    if let Some(monitor) = w.current_monitor()? {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let win_w = (W * scale) as i32;
        let win_h = (H * scale) as i32;
        let x = (size.width as i32 - win_w) / 2;
        let y = (size.height as i32 - win_h) / 2;
        w.set_position(PhysicalPosition::new(x, y))?;
    }
    Ok(())
}
