//! 录音指示器悬浮窗。
//!
//! macOS：用 `tauri-nspanel` 把 WebviewWindow swizzle 成 NSPanel，
//! style mask 设 `NSWindowStyleMaskNonactivatingPanel | NSWindowStyleMaskBorderless`，
//! level 设 floating，`canBecomeKey = false` —— 真·不抢焦点（参考 type4me FloatingBarPanel）。
//!
//! Windows / Linux：用 Tauri 内置 always-on-top + skip-taskbar 兜底。

// tauri-nspanel v2 仍引用 cocoa crate，而 cocoa 已官方推荐迁移 objc2-app-kit；
// 上游未完成迁移前，我们允许这些 deprecation warnings。
#![allow(deprecated)]

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "indicator";
const W: f64 = 520.0;
const H: f64 = 140.0;

/// macOS 的 NSWindowStyleMask 常量
#[cfg(target_os = "macos")]
const NS_WINDOW_STYLE_MASK_BORDERLESS: i32 = 0;
#[cfg(target_os = "macos")]
const NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL: i32 = 1 << 7; // 0x80
#[cfg(target_os = "macos")]
const NS_FLOATING_WINDOW_LEVEL: i32 = 3;

/// 显示或创建悬浮指示器。
pub fn show(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        // 已经是 panel：直接 show
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.show();
            return;
        }
    }

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

            // macOS：把 NSWindow swizzle 成 NSPanel，设置不抢焦点 style mask
            #[cfg(target_os = "macos")]
            {
                use tauri_nspanel::WebviewWindowExt;
                match w.to_panel() {
                    Ok(panel) => {
                        panel.set_level(NS_FLOATING_WINDOW_LEVEL);
                        panel.set_style_mask(
                            NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL
                                | NS_WINDOW_STYLE_MASK_BORDERLESS,
                        );
                        panel.set_floating_panel(true);
                        // collection_behaviour: canJoinAllSpaces | fullScreenAuxiliary = 1 | 256
                        use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                        panel.set_collection_behaviour(
                            NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
                        );
                        panel.show();
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "转换为 NSPanel 失败");
                        let _ = w.show();
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                let _ = w.show();
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "创建悬浮窗失败");
        }
    }
}

/// 关闭悬浮指示器。
pub fn hide(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.order_out(None);
            return;
        }
    }

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
