//! 录音指示器悬浮窗。
//!
//! macOS：用 `tauri-nspanel` 把 WebviewWindow swizzle 成 NSPanel，
//! style mask 设 `NSWindowStyleMaskNonactivatingPanel | NSWindowStyleMaskBorderless`，
//! level 设 floating，`canBecomeKey = false` —— 真·不抢焦点（参考 type4me FloatingBarPanel）。
//!
//! Windows：`SetWindowLongPtrW` 给窗口加 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`
//! ——前者让窗口不会成为前台/抢焦点(macOS NSPanel canBecomeKey=false 的等价),
//! 后者让窗口不进任务栏 / Alt-Tab(同 skip_taskbar 但更彻底)。
//! 透明背景 + 毛玻璃靠 `window-vibrancy::apply_mica`(Win11) / `apply_acrylic`(Win10)。
//!
//! Linux:用 Tauri 内置 always-on-top + skip-taskbar 兜底,无原生 vibrancy。

// tauri-nspanel v2 仍引用 cocoa crate，而 cocoa 已官方推荐迁移 objc2-app-kit；
// 上游未完成迁移前，我们允许这些 deprecation warnings。
#![allow(deprecated)]

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "indicator";
const W: f64 = 520.0;
// 180 足以在 panel 输出模式下容纳 textarea(40-80) + 按钮栏(~32) + padding；
// 录音态下用 flex justify-center 居中显示波形+计时器，上下留白无碍
const H: f64 = 180.0;

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
            #[cfg(target_os = "windows")]
            apply_windows_indicator_chrome(&w);
        }
        Err(e) => {
            tracing::error!(error = ?e, "预创建悬浮窗失败");
        }
    }
}

/// Windows 端 indicator 的「不抢焦点 + 毛玻璃」配置:
/// 1. WS_EX_NOACTIVATE:窗口被 show 时不抢前台 / 不让 enigo 写错目标 app
/// 2. WS_EX_TOOLWINDOW:不进任务栏和 Alt-Tab(skip_taskbar 在某些 DWM 主题下漏过)
/// 3. apply_mica(Win11) → fallback apply_acrylic(Win10):毛玻璃,跟 macOS NSPanel
///    透明背景对位
///
/// 任何步骤失败都只是 warn,不影响录音主流程。
#[cfg(target_os = "windows")]
fn apply_windows_indicator_chrome(w: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    match w.hwnd() {
        Ok(hwnd) => unsafe {
            // windows 0.61 的 HWND 是 InterfaceType(非 Copy),Param<HWND> 给 &T 实现 ——
            // 两次调用都用 &hwnd 共享借用,不消费 hwnd
            let cur = GetWindowLongPtrW(&hwnd, GWL_EXSTYLE);
            let new_style = cur | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
            SetWindowLongPtrW(&hwnd, GWL_EXSTYLE, new_style);
        },
        Err(e) => {
            tracing::warn!(error = ?e, "indicator: 获取 hwnd 失败,跳过 NOACTIVATE 设置");
        }
    }

    // Mica 只在 Win11 build 22000+ 可用;失败回退 Acrylic(Win10 v1809+)
    if let Err(mica_err) = window_vibrancy::apply_mica(w, Some(true)) {
        tracing::debug!(error = ?mica_err, "indicator: apply_mica 失败,尝试 acrylic");
        // RGBA:深灰带轻度透明,跟 macOS NSPanel 视觉接近
        if let Err(acrylic_err) = window_vibrancy::apply_acrylic(w, Some((20, 20, 25, 200))) {
            tracing::warn!(error = ?acrylic_err, "indicator: acrylic 也失败,fallback CSS backdrop-filter");
        } else {
            tracing::info!("indicator: 启用 Acrylic 毛玻璃");
        }
    } else {
        tracing::info!("indicator: 启用 Mica 毛玻璃");
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
