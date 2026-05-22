//! 系统托盘菜单。
//! 对应 Go 版的 tray.go，包含最近识别结果快捷复制。

use anyhow::Result;
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};

use crate::AppState;

/// 嵌入的托盘图标（专用于菜单栏：只有线条、透明底）。
/// 不能复用 32x32.png，那张带深色 squircle 底，macOS template 模式会把
/// 整个非透明区域染成纯白，显示成白方框。
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray.png");

/// 最近识别结果显示数量
const RECENT_COUNT: usize = 5;

/// 复制某条历史 id 的文本到剪贴板（menu event id 前缀）
const RECENT_COPY_PREFIX: &str = "recent:";

/// 切换 ASR 后端的 menu event id 前缀。后跟 provider id(zhipu/xfyun/volc/openrouter/local)。
const ASR_SWITCH_PREFIX: &str = "asr:";

/// 切换活跃 polish profile 的 menu event id 前缀。后跟 profile.id。
const POLISH_SWITCH_PREFIX: &str = "polish:";

/// ASR 后端选项 —— 跟前端 api.ts 的 ASR_PROVIDERS 对齐(顺序也对齐,
/// 用户在两个 UI 看到的顺序一致)。
const ASR_OPTIONS: &[(&str, &str)] = &[
    ("volc", "豆包(流式)"),
    ("xfyun", "讯飞(流式)"),
    ("zhipu", "智谱 GLM-ASR"),
    ("openrouter", "OpenRouter Whisper"),
    ("local", "本地引擎(离线)"),
];

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
    // sleep / 锁屏唤醒后 macOS CGEventTap 可能死掉(tap_is_enabled 仍报 true 但
    // 不收事件,handy-keys 100ms 自检救不了)。给用户一个手动重注册入口,
    // 比之前"改一下热键再改回来"的 workaround 直接。
    let reregister =
        MenuItem::with_id(app, "reregister_hotkey", "重新注册热键", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let cfg = app.state::<AppState>().snapshot();

    // ASR 后端 submenu(列出所有 provider,active 那个 ✓)
    let asr_submenu = build_asr_submenu(app, &cfg.asr_provider)?;

    // AI 润色 submenu(列出所有 polish_profiles,active 那个 ✓;
    // mode=off 的 profile label 后加"(关闭)"提示)
    let polish_submenu = build_polish_submenu(app, &cfg)?;

    // 最近 RECENT_COUNT 条识别结果（点击复制到剪贴板）
    let recent = crate::history::load(RECENT_COUNT as i64).unwrap_or_default();
    let mut recent_items: Vec<MenuItem<Wry>> = Vec::with_capacity(recent.len());
    for entry in &recent {
        let label = truncate(&entry.corrected_text, 40);
        let id = format!("{}{}", RECENT_COPY_PREFIX, entry.id);
        let item = MenuItem::with_id(app, &id, &label, true, None::<&str>)?;
        recent_items.push(item);
    }

    // 拼装菜单:操作项 → 分隔 → ASR / 润色 切换 → 重注册热键 → 分隔 → 最近结果 → 分隔 → 退出
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep_post_switch = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![
        &settings,
        &history,
        &logs,
        &config_dir,
        &sep1,
        &asr_submenu,
        &polish_submenu,
        &reregister,
        &sep_post_switch,
    ];

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

/// "识别后端" submenu。CheckMenuItem 自带 ✓ 渲染,active 那一项 checked。
fn build_asr_submenu(app: &AppHandle<Wry>, current: &str) -> Result<Submenu<Wry>> {
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::with_capacity(ASR_OPTIONS.len());
    for (id, label) in ASR_OPTIONS {
        let event_id = format!("{}{}", ASR_SWITCH_PREFIX, id);
        let it =
            CheckMenuItem::with_id(app, &event_id, *label, true, current == *id, None::<&str>)?;
        items.push(it);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    Submenu::with_items(app, "识别后端", true, &refs).map_err(Into::into)
}

/// "AI 润色" submenu。每个 profile 一项 CheckMenuItem;active profile ✓;
/// 单个 profile 的 backend_id 空 / 引用了不存在的 backend → label 加"(关闭)"提示
/// 切到这个 profile 等于不润色。
fn build_polish_submenu(app: &AppHandle<Wry>, cfg: &crate::config::Config) -> Result<Submenu<Wry>> {
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::with_capacity(cfg.polish_profiles.len().max(1));
    if cfg.polish_profiles.is_empty() {
        // 兜底:理论上 Config::default 至少有一个 profile,但防御性渲染一个 disabled
        let it = CheckMenuItem::with_id(
            app,
            "polish_empty",
            "(无 profile)",
            false,
            false,
            None::<&str>,
        )?;
        items.push(it);
    } else {
        for p in &cfg.polish_profiles {
            let event_id = format!("{}{}", POLISH_SWITCH_PREFIX, p.id);
            let off = p.backend_id.is_empty()
                || cfg.backend_by_id(&p.backend_id).is_none();
            let label = if off {
                format!("{}(关闭)", p.name)
            } else {
                p.name.clone()
            };
            let checked = p.id == cfg.active_profile_id;
            let it = CheckMenuItem::with_id(app, &event_id, &label, true, checked, None::<&str>)?;
            items.push(it);
        }
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    Submenu::with_items(app, "AI 润色", true, &refs).map_err(Into::into)
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
    if let Some(provider) = id.strip_prefix(ASR_SWITCH_PREFIX) {
        switch_asr_provider(app, provider);
        return;
    }
    if let Some(profile_id) = id.strip_prefix(POLISH_SWITCH_PREFIX) {
        switch_polish_profile(app, profile_id);
        return;
    }
    match id {
        "settings" => show_main(app, None),
        "history" => show_main(app, Some("#/history")),
        "logs" => {
            // 统一走 commands::open_logs 的逻辑：找 mtime 最新的 log，没有就开目录
            let _ = crate::commands::open_logs();
        }
        "config_dir" => {
            let _ = open_path(&crate::dirs::config_dir().to_string_lossy());
        }
        "reregister_hotkey" => {
            // 强制让 KeyboardBackend 重建 supervisor 线程 + handy-keys
            // HotkeyManager / KeyboardListener,救活 sleep 唤醒后失活的 CGEventTap。
            // 直接 take 出 backend Drop,然后用当前 cfg 重 start —— 不走 reload,
            // 因为 reload 复用同一个 supervisor 线程,如果连那都死了 reload 救不了。
            let state = app.state::<AppState>();
            let cfg = state.snapshot();
            let _ = state.keyboard.lock().take(); // Drop 旧 backend
            match crate::start_or_reload_keyboard(app, &cfg) {
                Ok(()) => tracing::info!("tray: 已手动重注册热键"),
                Err(e) => tracing::warn!(error = ?e, "tray: 重注册热键失败"),
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

/// tray 切 ASR 后端 → 改 cfg.asr_provider → 走 save_config(它会处理本地引擎
/// warmup / unload)→ 通知前端 + 刷 tray。
fn switch_asr_provider(app: &AppHandle<Wry>, provider: &str) {
    let mut new_cfg = (*app.state::<AppState>().snapshot()).clone();
    if new_cfg.asr_provider == provider {
        return; // 同一个,无需变更
    }
    new_cfg.asr_provider = provider.to_string();
    apply_config_change(app, new_cfg, "切换 ASR 后端");
}

/// tray 切活跃 polish profile → 改 cfg.active_profile_id → save_config。
fn switch_polish_profile(app: &AppHandle<Wry>, profile_id: &str) {
    let mut new_cfg = (*app.state::<AppState>().snapshot()).clone();
    if new_cfg.active_profile_id == profile_id {
        return;
    }
    // 校验 profile 真实存在,防止 stale menu 点到已删 profile
    if !new_cfg.polish_profiles.iter().any(|p| p.id == profile_id) {
        tracing::warn!(profile_id = %profile_id, "tray: profile 不存在,忽略切换");
        return;
    }
    new_cfg.active_profile_id = profile_id.to_string();
    apply_config_change(app, new_cfg, "切换 polish profile");
}

/// 把变更后的 cfg 喂给 commands::save_config(它已经处理 hotkey reload /
/// 本地引擎 warmup / 日志级别热替换 / 广播 config-updated 等)。
/// 失败仅 warn —— tray 操作不该崩 app,用户重试即可。
fn apply_config_change(app: &AppHandle<Wry>, new_cfg: crate::config::Config, op: &str) {
    let state = app.state::<AppState>();
    if let Err(e) = crate::commands::save_config(new_cfg, state, app.clone()) {
        tracing::warn!(error = %e, op = %op, "tray 触发 save_config 失败");
        return;
    }
    if let Err(e) = refresh(app) {
        tracing::warn!(error = ?e, "tray 刷新菜单失败");
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
