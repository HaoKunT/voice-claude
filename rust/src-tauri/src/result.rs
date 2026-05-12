//! panel 输出模式的识别结果窗口。
//!
//! 独立于 indicator NSPanel —— 保持普通 WebviewWindow（**不** swizzle 成 NSPanel）。
//! 这样 macOS Accessory 模式下点击窗口时 app 会被正常 activate，TSM 的 input
//! context 挂得上，中文输入法候选窗口才能正常显示。NSPanel + NonactivatingPanel
//! 不会 activate app，IME 候选窗就没法挂上。

// tauri-nspanel 的 cocoa 依赖带 deprecation warning（同 indicator.rs 的解释）
#![allow(deprecated)]

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "result";
const W: f64 = 520.0;
// 比 indicator 的 180 略高，给 textarea + 按钮 + padding 留更多编辑空间
const H: f64 = 200.0;

/// 启动时预创建 result 窗口，保持隐藏。首次 show 时不用走完整 WebView 初始化。
/// 必须在 Tauri setup 里调用（主线程）。
pub fn prebuild(app: &AppHandle) {
    if app.get_webview_window(LABEL).is_some() {
        return;
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("result.html".into()))
        .title("voice-claude 识别结果")
        .inner_size(W, H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .visible_on_all_workspaces(true);

    match builder.build() {
        Ok(w) => {
            if let Err(e) = center(&w) {
                tracing::warn!(error = ?e, "result 窗口居中失败");
            }
            #[cfg(target_os = "macos")]
            {
                // 透明背景：否则 NSWindow 底板会压出白色矩形
                if let Ok(ns_window) = w.ns_window() {
                    use objc::{class, msg_send, sel, sel_impl};
                    use tauri_nspanel::cocoa::base::{id, NO};
                    unsafe {
                        let ns_window = ns_window as id;
                        let _: () = msg_send![ns_window, setOpaque: NO];
                        let clear: id = msg_send![class!(NSColor), clearColor];
                        let _: () = msg_send![ns_window, setBackgroundColor: clear];
                    }
                }
            }
            tracing::info!("result 窗口预创建完成");
        }
        Err(e) => {
            tracing::error!(error = ?e, "预创建 result 窗口失败");
        }
    }
}

/// 显示识别结果窗口并把文本推给前端。
/// 普通 window 的 show + set_focus 会让 app 变 active（Accessory 下不显 Dock 图标），
/// textarea 获得 first responder 后 IME 正常工作。
pub fn show(app: &AppHandle, text: &str) {
    if app.get_webview_window(LABEL).is_none() {
        prebuild(app);
    }
    let Some(w) = app.get_webview_window(LABEL) else {
        tracing::error!("result 窗口不存在，无法显示");
        return;
    };
    // 每次 show 前重新居中，适应外接显示器等场景
    if let Err(e) = center(&w) {
        tracing::warn!(error = ?e, "result 窗口居中失败");
    }
    // 先 show 再 emit，确保前端 listener 已挂上
    if let Err(e) = w.show() {
        tracing::warn!(error = ?e, "result 窗口显示失败");
    }
    if let Err(e) = w.set_focus() {
        tracing::warn!(error = ?e, "result 窗口聚焦失败");
    }
    if let Err(e) = app.emit_to(LABEL, "result-show", text.to_string()) {
        tracing::warn!(error = ?e, "发送 result-show 事件失败");
    }
}

/// 隐藏 result 窗口（✕ 按钮 / Esc 触发）。
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
