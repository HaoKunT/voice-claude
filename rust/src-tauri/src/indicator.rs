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

/// 启动时预创建 indicator 窗口并立即转换为 NSPanel，但保持隐藏。
/// 这样后续 show() 不会触发窗口创建（避免首次创建时的 app 激活抢焦点）。
/// 必须在 Tauri setup 里调用，main 线程上下文。
pub fn prebuild(app: &AppHandle) {
    if app.get_webview_window(LABEL).is_some() {
        return;
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("indicator.html".into()))
        .title("")
        .inner_size(W, H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false) // 预创建时不显示，show() 时再展示
        .visible_on_all_workspaces(true);

    match builder.build() {
        Ok(w) => {
            if let Err(e) = center(&w) {
                tracing::warn!(error = ?e, "悬浮窗居中失败");
            }
            #[cfg(target_os = "macos")]
            {
                // 转 panel 前先把底层 NSWindow 的 opaque / backgroundColor 改为透明；
                // 否则即使 webview 背景是 transparent，NSWindow 仍会有白色底板。
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

                use tauri_nspanel::WebviewWindowExt;
                match w.to_panel() {
                    Ok(panel) => {
                        panel.set_level(NS_FLOATING_WINDOW_LEVEL);
                        panel.set_style_mask(
                            NS_WINDOW_STYLE_MASK_NONACTIVATING_PANEL
                                | NS_WINDOW_STYLE_MASK_BORDERLESS,
                        );
                        panel.set_floating_panel(true);
                        // 关键：panel 只在真的需要时才 become key（默认 false 会抢焦点）
                        panel.set_becomes_key_only_if_needed(true);
                        // 防止 app 失活时 panel 被 hide
                        panel.set_hides_on_deactivate(false);
                        use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                        panel.set_collection_behaviour(
                            NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                                | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
                        );
                        tracing::info!("indicator NSPanel 预创建完成");
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "转换为 NSPanel 失败");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "预创建悬浮窗失败");
        }
    }
}

/// 显示悬浮指示器（复用预创建的 NSPanel）。
///
/// **不要调 panel.show()** —— tauri-nspanel 的 show() 内部会 make_key_window，
/// 导致目标 app 失焦，enigo 输入到错误窗口。只 order_front_regardless。
pub fn show(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.order_front_regardless();
            return;
        }
        // 没预创建，补创建一次（可能 prebuild 失败）
        prebuild(app);
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.order_front_regardless();
            return;
        }
    }

    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
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
