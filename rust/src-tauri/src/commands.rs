//! Tauri IPC commands：前端（React）调用的 Rust 函数。

use crate::asr::local;
use crate::audio::{list_capture_devices, CaptureDevice};
use crate::config::Config;
use crate::AppState;
use crate::{correct, dirs, history};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct DeviceInfo {
    pub name: String,
}

impl From<CaptureDevice> for DeviceInfo {
    fn from(d: CaptureDevice) -> Self {
        Self { name: d.name }
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    (*state.snapshot()).clone()
}

#[tauri::command]
pub fn save_config(cfg: Config, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    use crate::asr::local;
    use crate::config::ASR_PROVIDER_LOCAL;

    let prev = state.snapshot();
    let prev_hotkey = prev.hotkey.clone();
    let prev_ptt = prev.push_to_talk;
    let prev_trigger_mode = prev.trigger_mode.clone();
    let prev_dtm = prev.double_tap_modifier.clone();
    let prev_log_level = prev.log_level.clone();
    let prev_asr_provider = prev.asr_provider.clone();
    let prev_local_engine = prev.local_engine.clone();
    let prev_local_coreml = prev.local_use_coreml;
    let prev_hotwords = prev.hotwords.clone();

    let new_log_level = cfg.log_level.clone();
    // 任一影响 keyboard backend 的字段变了就 reload。push_to_talk 保留是因为
    // 老 config 升级路径(trigger_mode 缺省时 fallback 看 push_to_talk)。
    let needs_reload = cfg.hotkey != prev_hotkey
        || cfg.push_to_talk != prev_ptt
        || cfg.trigger_mode != prev_trigger_mode
        || cfg.double_tap_modifier != prev_dtm;
    if needs_reload {
        if let Err(e) = crate::start_or_reload_keyboard(&app, &cfg) {
            tracing::warn!(error = ?e, "save_config 后 reload keyboard backend 失败");
            return Err(format!("热键注册失败：{}", e));
        }
    }
    if let Err(e) = cfg.save() {
        if needs_reload {
            // 回滚到旧 cfg。keyboard backend 已经按新 cfg reload 过,要再 reload 回去。
            let _ = crate::start_or_reload_keyboard(&app, &prev);
        }
        return Err(e.to_string());
    }
    state.replace(cfg);
    // 日志级别变了就热替换 tracing EnvFilter
    if new_log_level != prev_log_level {
        crate::logger::reload(&new_log_level);
    }

    // 本地模型生命周期:
    //   - 切到非 local 后端 → unload 释放内存(SenseVoice 几百 MB / FireRed 几 GB)
    //   - 切到 local / 换 engine / 换 coreml / 改热词 → 后台预热新模型
    let new_cfg = state.snapshot();
    let now_local = new_cfg.asr_provider == ASR_PROVIDER_LOCAL;
    let was_local = prev_asr_provider == ASR_PROVIDER_LOCAL;
    if was_local && !now_local {
        local::unload();
    } else if now_local
        && (prev_asr_provider != new_cfg.asr_provider
            || prev_local_engine != new_cfg.local_engine
            || prev_local_coreml != new_cfg.local_use_coreml
            || prev_hotwords != new_cfg.hotwords)
    {
        let cfg_for_warm = (*new_cfg).clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = local::warm_up(&cfg_for_warm) {
                tracing::warn!(error = ?e, "save_config 后预热失败");
            }
        });
    }

    // 广播给其他前端组件（比如主窗口 sidebar 显示的快捷键）刷新自己的 cfg 副本
    let _ = app.emit("config-updated", ());
    Ok(())
}

#[tauri::command]
pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    list_capture_devices()
        .map(|ds| ds.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_history(limit: i64) -> Result<Vec<history::HistoryEntry>, String> {
    history::load(limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history(id: i64) -> Result<(), String> {
    history::delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    history::clear().map_err(|e| e.to_string())
}

/// 历史记录聚合统计(总次数/时长/字数/字速/节省时间),用于 HistoryView 仪表盘。
#[tauri::command]
pub fn get_history_stats() -> Result<history::Stats, String> {
    history::stats().map_err(|e| e.to_string())
}

/// ASR / 润色延时统计:全量 + 近 24h + 近 7d 三套窗口,按 provider / model 分组。
#[tauri::command]
pub fn get_latency_stats() -> Result<history::LatencyStats, String> {
    history::latency_stats().map_err(|e| e.to_string())
}

/// 用指定 profile 对历史条目的 raw_text 重新跑一遍润色,返回新结果。
/// 不写回数据库 —— 让用户可以试不同 profile 看效果,满意自己复制。
///
/// profile_id 找不到时 fallback 到 active profile —— 前端选中的 profile 可能刚好
/// 在另一个窗口被删/改了,此时给个可用结果比抛错更友好。fallback 会在日志里 warn。
#[tauri::command]
pub async fn repolish_history(
    history_id: i64,
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let entry = history::get(history_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("历史记录 #{} 不存在", history_id))?;
    let cfg = state.snapshot();
    let profile = match cfg.polish_profiles.iter().find(|p| p.id == profile_id) {
        Some(p) => p,
        None => {
            tracing::warn!(
                requested = %profile_id,
                fallback = %cfg.active_profile_id,
                "repolish: profile 找不到,fallback 到 active profile"
            );
            cfg.active_profile()
        }
    };
    let backend = match cfg.backend_by_id(&profile.backend_id) {
        Some(b) => b,
        None => {
            if profile.backend_id.is_empty() {
                return Err(format!(
                    "profile 「{}」选了「关闭」,无法重润色",
                    profile.name
                ));
            }
            return Err(format!(
                "profile 「{}」引用的 backend 「{}」不存在",
                profile.name, profile.backend_id
            ));
        }
    };
    tracing::info!(
        profile = %profile.name,
        backend = %backend.name,
        mode = %backend.mode,
        model = %backend.model,
        "repolish: 开始润色"
    );
    correct::correct(
        &entry.raw_text,
        profile,
        backend,
        cfg.correct_timeout_secs(),
        &cfg.hotwords,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_ollama(url: String) -> Result<(), String> {
    crate::llm_client::check_ollama(&url)
        .await
        .map_err(|e| e.to_string())
}

/// 找日志目录下 mtime 最新的日志文件。
/// 匹配新格式 `voice-claude.YYYY-MM-DD.log` + 老格式 `voice-claude.log.YYYY-MM-DD`
/// + 首版无后缀的 `voice-claude.log`。open_logs 和 read_recent_logs 共用。
fn find_latest_log() -> Option<std::path::PathBuf> {
    let dir = dirs::log_dir();
    let entries = std::fs::read_dir(&dir).ok()?;
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("voice-claude"))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mtime))
        })
        .max_by_key(|(_, t)| *t)
        .map(|(p, _)| p)
}

#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    match find_latest_log() {
        Some(p) => open_path(&p.to_string_lossy()),
        None => open_path(&dirs::log_dir().to_string_lossy()),
    }
}

#[tauri::command]
pub fn open_log_dir() -> Result<(), String> {
    open_path(&dirs::log_dir().to_string_lossy())
}

/// 导出整个 config.json 内容作为字符串（前端负责写盘）。
#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.snapshot();
    serde_json::to_string_pretty(&*cfg).map_err(|e| e.to_string())
}

/// 从 JSON 字符串导入整个 config。顺序：
///   1. 反序列化 + 预校验 hotkey 字符串(不实注册,避免后面 save 失败时热键已改)
///   2. save 到磁盘(可能 IO 失败)
///   3. start_or_reload_keyboard(真正注册系统热键)
///   4. replace AppState + reload log + 广播
/// 任一步骤失败前都没改状态,失败后磁盘/内存/系统热键保持一致。
#[tauri::command]
pub fn import_config(
    json: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let new_cfg: Config =
        serde_json::from_str(&json).map_err(|e| format!("JSON 解析失败：{}", e))?;
    // 预校验 hotkey 语法,先不注册
    crate::keyboard::config::parse_hotkey(&new_cfg.hotkey)
        .map_err(|e| format!("导入的热键无效：{}", e))?;
    new_cfg.save().map_err(|e| e.to_string())?;
    crate::start_or_reload_keyboard(&app, &new_cfg)
        .map_err(|e| format!("导入的热键注册失败：{}", e))?;
    let new_log_level = new_cfg.log_level.clone();
    state.replace(new_cfg);
    crate::logger::reload(&new_log_level);
    let _ = app.emit("config-updated", ());
    Ok(())
}

/// 录入新快捷键前暂停系统级热键。否则用户按当前热键时会触发录音,
/// webview 拿不到 keydown 事件。
///
/// 实现:把 KeyboardBackend 整个 take 出来 Drop —— 这会发 Shutdown 给 supervisor
/// 线程,旧 HotkeyManager Drop 自动卸 OS hook。比 unregister_all 更彻底,handy-keys
/// 没有 unregister_all API 也不影响。
#[tauri::command]
pub fn suspend_hotkey(_app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _ = state.keyboard.lock().take();
    Ok(())
}

/// 录入取消后把系统热键恢复成当前 config 里的 hotkey。
/// 录入成功后 save_config 会自己 re-register 新的,这个 command 可不调。
#[tauri::command]
pub fn resume_hotkey(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.snapshot();
    crate::start_or_reload_keyboard(&app, &cfg).map_err(|e| e.to_string())
}

/// panel 输出模式下，悬浮窗里的 ✕ 按钮点击后调这个关窗口。
#[tauri::command]
pub fn close_indicator(app: AppHandle) -> Result<(), String> {
    let app_for_hide = app.clone();
    app.run_on_main_thread(move || {
        crate::indicator::hide(&app_for_hide);
    })
    .map_err(|e| e.to_string())
}

/// panel 输出模式的识别结果窗口关闭按钮。
#[tauri::command]
pub fn close_result_window(app: AppHandle) -> Result<(), String> {
    let app_for_hide = app.clone();
    app.run_on_main_thread(move || {
        crate::result::hide(&app_for_hide);
    })
    .map_err(|e| e.to_string())
}

/// 录音中取消本次(丢弃录音,不走 ASR/AI/输出)。ESC 全局热键外的兜底入口,
/// 比如悬浮窗 webview 获得焦点时本地 keydown 也可以 invoke 这个命令。
#[tauri::command]
pub fn cancel_recording() -> Result<(), String> {
    crate::recorder::cancel();
    Ok(())
}

/// 读取最新日志文件的最后 limit 行，返回给前端展示。
#[tauri::command]
pub fn read_recent_logs(limit: usize) -> Result<Vec<String>, String> {
    let Some(path) = find_latest_log() else {
        return Ok(vec![]);
    };
    // 避免把特别大的日志全读入内存：超过 2MB 只取末尾一段
    const MAX_READ: u64 = 2 * 1024 * 1024;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let file_len = meta.len();
    let from = file_len.saturating_sub(MAX_READ);
    let content = if from == 0 {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
        let mut buf = Vec::with_capacity(MAX_READ as usize);
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        // 丢掉第一行（可能 seek 到半个 UTF-8 字符或半行）
        let s = String::from_utf8_lossy(&buf).into_owned();
        match s.find('\n') {
            Some(i) => s[i + 1..].to_string(),
            None => s,
        }
    };
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].to_vec())
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_hash: &'static str,
    pub git_dirty: &'static str,
    pub rustc_version: &'static str,
    pub build_time: &'static str,
    pub target: &'static str,
    pub tauri_version: &'static str,
    pub debug: bool,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("VC_GIT_HASH"),
        git_dirty: env!("VC_GIT_DIRTY"),
        rustc_version: env!("VC_RUSTC_VERSION"),
        build_time: env!("VC_BUILD_TIME"),
        target: env!("VC_TARGET"),
        tauri_version: tauri::VERSION,
        debug: cfg!(debug_assertions),
    }
}

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    /// 当前下载的引擎 id,前端按这个匹配自己 panel 的进度
    pub engine_id: String,
}

#[derive(Serialize)]
pub struct LocalEngineInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    pub url: String,
    pub sha256: String,
    pub model_dir: String,
    pub available: bool,
    pub size_mb: u32,
}

fn engine_to_info(engine: local::LocalEngine) -> LocalEngineInfo {
    LocalEngineInfo {
        id: engine.id().to_string(),
        label: engine.label().to_string(),
        description: engine.description().to_string(),
        url: engine.model_url(),
        sha256: engine.sha256().to_string(),
        model_dir: engine.install_path().to_string_lossy().into_owned(),
        available: engine.is_available(),
        size_mb: engine.approx_size_mb(),
    }
}

/// 列出所有本地 ASR 引擎(给设置页 dropdown + 状态面板)。
#[tauri::command]
pub fn list_local_engines() -> Vec<LocalEngineInfo> {
    local::LocalEngine::ALL
        .iter()
        .map(|&e| engine_to_info(e))
        .collect()
}

#[derive(Serialize)]
pub struct ProfileTemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mode: String,
    pub prompt: String,
}

/// 给前端 Profile 设置页用 —— 列举所有内置模板(id/name/description + 当前 prompt 文本)。
/// 前端"📋 从模板"挑一个新建 builtin profile;ProfileCard 显示 builtin profile 时
/// 也按 template_id 在这个列表里查 prompt 文本展示(只读)。
#[tauri::command]
pub fn list_profile_templates() -> Vec<ProfileTemplateInfo> {
    crate::profile_templates::ALL_TEMPLATES
        .iter()
        .map(|t| ProfileTemplateInfo {
            id: t.id.into(),
            name: t.name.into(),
            description: t.description.into(),
            mode: t.mode.into(),
            prompt: t.prompt.into(),
        })
        .collect()
}

/// 按 id 查询单个引擎信息(刷新下载状态)。
#[tauri::command]
pub fn get_local_engine_info(id: String) -> LocalEngineInfo {
    engine_to_info(local::LocalEngine::from_id(&id))
}

/// 下载指定引擎模型,进度通过 `local-engine-download-progress` 事件推送。
/// payload 含 engine_id,前端按 id 过滤自己面板的进度。
#[tauri::command]
pub async fn download_local_engine(id: String, app: AppHandle) -> Result<(), String> {
    let engine = local::LocalEngine::from_id(&id);
    let engine_id = engine.id().to_string();
    local::download_engine(engine, move |downloaded, total| {
        let _ = app.emit(
            "local-engine-download-progress",
            DownloadProgress {
                downloaded,
                total,
                engine_id: engine_id.clone(),
            },
        );
    })
    .await
    .map_err(|e| e.to_string())
}

/// 从本地 tar.bz2 导入指定引擎模型(国内下载失败兜底)。
#[tauri::command]
pub async fn import_local_engine_tarball(id: String, path: String) -> Result<(), String> {
    let engine = local::LocalEngine::from_id(&id);
    local::import_engine_tarball(engine, std::path::PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PunctModelInfo {
    pub label: String,
    pub description: String,
    pub url: String,
    pub sha256: String,
    pub model_dir: String,
    pub available: bool,
    pub size_mb: u32,
}

/// 标点模型信息(给 UI 状态卡)。
#[tauri::command]
pub fn get_punct_model_info() -> PunctModelInfo {
    PunctModelInfo {
        label: local::punct_model_label().to_string(),
        description: local::punct_model_description().to_string(),
        url: local::punct_model_url(),
        sha256: local::punct_model_sha256().to_string(),
        model_dir: local::punct_model_install_path()
            .to_string_lossy()
            .into_owned(),
        available: local::punct_model_is_available(),
        size_mb: local::punct_model_size_mb(),
    }
}

#[derive(Serialize, Clone)]
pub struct PunctDownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

/// 下载标点模型,进度通过 `punct-model-download-progress` 事件推送。
#[tauri::command]
pub async fn download_punct_model(app: AppHandle) -> Result<(), String> {
    local::download_punct_model(move |downloaded, total| {
        let _ = app.emit(
            "punct-model-download-progress",
            PunctDownloadProgress { downloaded, total },
        );
    })
    .await
    .map_err(|e| e.to_string())
}

/// 从本地 tar.bz2 导入标点模型。
#[tauri::command]
pub async fn import_punct_model_tarball(path: String) -> Result<(), String> {
    local::import_punct_tarball(std::path::PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())
}

/// 横向 ASR 测试:传文件 + 多选后端 → 跑识别 → emit "bench-result" 事件。
/// command 一返回意味着调度结束(云端 spawn 出去 + 本地 task 已开始);具体
/// 每个后端的结果通过 event 流式回报,前端 listen 填表格。
#[tauri::command]
pub async fn bench_transcribe_file(
    path: String,
    provider_ids: Vec<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    use crate::asr::bench;
    use std::sync::Arc;

    let cfg = state.snapshot();
    let path_buf = std::path::PathBuf::from(&path);

    // 解码可能耗几百 ms(尤其 mp3),放 spawn_blocking 别堵 tokio runtime
    let wav = tokio::task::spawn_blocking(move || bench::decode_to_pcm16k_mono_wav(&path_buf))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("解码失败:{}", e))?;
    let wav = Arc::new(wav);

    let (cloud_ids, local_ids) = bench::split_cloud_local(&provider_ids);

    // 云端可并行 —— 各开一个 task,谁好了谁先 emit
    for id in cloud_ids {
        let cfg = cfg.clone();
        let wav = wav.clone();
        let app = app.clone();
        tokio::spawn(async move {
            bench::run_one_and_emit(id, cfg, wav, app).await;
        });
    }

    // 本地引擎共享 OfflineRecognizer cache mutex —— 必须串行,否则会反复
    // rebuild 模型,加载开销叠加且无意义。一个 task 顺序跑完所有 local id。
    if !local_ids.is_empty() {
        tokio::spawn(async move {
            for id in local_ids {
                bench::run_one_and_emit(id, cfg.clone(), wav.clone(), app.clone()).await;
            }
        });
    }
    Ok(())
}

/// 把当前识别词典导出为单列 CSV(每行一个词)。前端用 save dialog 保存。
#[tauri::command]
pub fn export_hotwords_csv(state: State<'_, AppState>) -> String {
    let cfg = state.snapshot();
    let mut out = String::from("词\n");
    let mut entries: Vec<&String> = cfg.hotwords.iter().collect();
    entries.sort();
    for w in entries {
        out.push_str(&format!("{}\n", csv_escape(w)));
    }
    out
}

/// 从单列 CSV(每行一个词)解析识别词典,覆盖或合并当前配置。
#[tauri::command]
pub fn import_hotwords_csv(
    csv: String,
    merge: bool,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<String> = if merge {
        state.snapshot().hotwords.iter().cloned().collect()
    } else {
        BTreeSet::new()
    };
    let mut added = 0usize;
    for (i, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // 跳过表头(可选);兼容老 CSV 双列文件(只取第一列)
        if i == 0
            && (line.starts_with("词")
                || line.starts_with("原词")
                || line.to_lowercase().starts_with("from"))
        {
            continue;
        }
        let Some(row) = parse_csv_row(line) else {
            continue;
        };
        if row.is_empty() {
            continue;
        }
        let word = row[0].trim();
        if word.is_empty() {
            continue;
        }
        if set.insert(word.to_string()) {
            added += 1;
        }
    }

    let mut cfg = (*state.snapshot()).clone();
    cfg.hotwords = set.into_iter().collect();
    cfg.save().map_err(|e| e.to_string())?;
    state.replace(cfg);
    Ok(added)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 简易 CSV 行解析（支持引号包围 + 引号转义）。
fn parse_csv_row(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quote {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quote = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quote = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    Some(fields)
}

#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    open_path(&dirs::config_dir().to_string_lossy())
}

fn open_path(path: &str) -> Result<(), String> {
    use std::process::Command;
    let cmd = if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    cmd.map(|_| ()).map_err(|e| e.to_string())
}

/// macOS:查询辅助功能权限是否授予(ad-hoc 签名升级后此权限常会失效)。
/// 复用 handy-keys 已封装的 `AXIsProcessTrusted` 调用,跟 keyboard backend
/// 用同一个权限判定来源,避免一处说"已授权"另一处又说"没权"的不一致。
///
/// **副作用**:每次调用时检测 false → true 转换,自动重启 keyboard backend。
/// 借用前端 AccessibilityBanner 的 window focus listener —— 用户去系统设置
/// 授权完切回 voice-claude,banner 调本 command,后端立即把 backend 拉起来,
/// 不必让用户重启 app。比"每秒 poll OS API" 优雅 —— 事件驱动,无空转。
///
/// 其他平台直接返回 true。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility(app: AppHandle) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    // Lazy 初始化为当前实际状态:避免 static 默认 false 让首次 check
    // (banner mount 时调)总触发 false→true transition,导致启动后冗余 reload
    // (会让 supervisor 把刚 build 好的 manager/listener 拆了重 build 一遍)。
    static LAST_GRANTED: OnceLock<AtomicBool> = OnceLock::new();
    let now = handy_keys::check_accessibility();
    let cell = LAST_GRANTED.get_or_init(|| AtomicBool::new(now));
    let last = cell.swap(now, Ordering::Relaxed);
    if now && !last {
        // 刚从 false → true 转换:用户刚授权 / TCC DB 刚 ready。
        // 后端 backend 八成还没起来,主动 reload 一次让热键立即工作。
        tracing::info!("check_accessibility: 检测到权限恢复,自动启动 backend");
        let cfg = app.state::<AppState>().snapshot();
        if let Err(e) = crate::start_or_reload_keyboard(&app, &cfg) {
            tracing::warn!(error = ?e, "check_accessibility: 自动启动 backend 失败");
        } else if let Err(e) = crate::tray::refresh(&app) {
            tracing::warn!(error = ?e, "check_accessibility: 刷新 tray 失败");
        }
    }
    now
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility(_app: AppHandle) -> bool {
    true
}

/// 跳到系统「隐私与安全性 → 辅助功能」面板。
/// macOS 走 handy-keys 封装(它内部会用 `AXIsProcessTrustedWithOptions` 触发系统
/// prompt 并打开面板,授权后切回 voice-claude 时 banner focus listener 会自动
/// 拉起 backend,无需重启 app)。
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        handy_keys::open_accessibility_settings().map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("仅 macOS 支持".into())
    }
}

// ─── 识别词典自动生成(从外部数据源 + LLM 二次筛选) ──────────────────

/// 列出当前可用的词典数据源(目前只有 Claude Code 历史)。
#[tauri::command]
pub fn list_hotword_sources() -> Vec<crate::hotword_sources::SourceInfo> {
    crate::hotword_sources::list_for_ui()
}

#[derive(serde::Serialize)]
pub struct HotwordCandidate {
    pub word: String,
    pub freq: u32,
    /// LLM 是否推荐加入(false 时前端默认不勾选,但仍展示给用户决定)。
    pub suggested: bool,
}

/// 扫描数据源 → 频率统计 → LLM 二次筛选 → 返回候选给前端。
///
/// - `source_id`:`list_hotword_sources` 里某项的 id(当前 `"claude_code"`)
/// - `days`:扫描最近 N 天的数据
/// - `backend_id`:用哪个 LLM 后端做筛选(直接选 backend,不再绕 polish profile)
///
/// 返回的列表已经过滤掉用户当前 cfg.hotwords 已有的词,前端 modal 直接展示让
/// 用户勾选。 `suggested=true` 是 LLM 推荐的;`false` 是 LLM 没选但本地频率
/// 给到的备选(用户也能在 modal 里手动勾选)。
#[tauri::command]
pub async fn scan_hotword_candidates(
    source_id: String,
    days: u32,
    backend_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<HotwordCandidate>, String> {
    use crate::hotword_sources::{find_source, llm_filter};
    let cfg = state.snapshot();
    let source = find_source(&source_id).ok_or_else(|| format!("未知数据源: {}", source_id))?;
    if !source.available() {
        return Err(format!("数据源 {} 不可用(目录不存在?)", source.label()));
    }
    let backend = cfg
        .backend_by_id(&backend_id)
        .ok_or_else(|| format!("未知 backend: {}", backend_id))?
        .clone();

    // Step 1: 抽取数据源用户文本
    let text = source
        .extract_user_text(days)
        .map_err(|e| format!("读取数据源失败: {}", e))?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: 截取末尾摘录给 LLM。末尾段落更代表用户当前关注的领域,
    // truncate_for_llm 内部默认 30k 字符 —— 云端大模型(Claude / GPT-4 / 32k+
    // OpenRouter 模型)能容下;ollama 用户得保证 num_ctx 够大。
    let excerpt = llm_filter::truncate_for_llm(&text);

    // Step 3: LLM 单层提取 —— 直接把原文交给 LLM,让它从中识别专名 / 术语。
    // 不再做本地分词 + 频率统计的两层结构(本地分词只切英文 token,中文专名
    // 完全没机会进候选;LLM 看到全英文候选列表也跟着只挑英文)。
    //
    // 用独立 timeout —— polish profile 默认 10s 是给"短文本润色"用的,但
    // 让 LLM 处理 ~30k 字符原文 + 列已存在词典(润色任务的 10-30 倍),小模型
    // 在大上下文上 prefill 很慢。300s = 5min 是经验值,xiaomi/mimo-v2-flash
    // 类的中等模型实测 1-3 分钟,GPT-4 Turbo / Claude Sonnet 30s 内。
    const HOTWORD_LLM_TIMEOUT_SECS: u64 = 300;
    let existing_owned = cfg.hotwords.clone();
    tracing::info!(
        excerpt_chars = excerpt.chars().count(),
        existing_count = existing_owned.len(),
        backend = %backend.name,
        mode = %backend.mode,
        model = %backend.model,
        timeout_secs = HOTWORD_LLM_TIMEOUT_SECS,
        "hotwords: 调 LLM 提取"
    );
    let llm_words: Vec<String> = match llm_filter::extract_hotwords(
        &excerpt,
        &existing_owned,
        &backend,
        HOTWORD_LLM_TIMEOUT_SECS,
    )
    .await
    {
        Ok(list) => {
            tracing::info!(returned = list.len(), "hotwords: LLM 提取返回");
            list
        }
        Err(e) => {
            // 把 anyhow chain 全部写出来 —— "call cloud endpoint" 这种顶层
            // context 看不出是 timeout / dns / status / parse,需要 source chain。
            // tracing 单 field 会截到第一行,所以手动 join。
            let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
            tracing::warn!(error_chain = ?chain, "hotwords: LLM 提取失败");
            return Err(format!("LLM 调用失败: {}", chain.join(" → ")));
        }
    };

    // Step 4: 后处理 ——
    //   a. case-insensitive 同词归一:LLM 偶尔同时返回 Claude / claude /
    //      Voice-Claude / voice-claude 这种大小写变体。按 lowercase 分组,
    //      保留 freq 最高的那个 form。
    //   b. blacklist 兜底:LLM 即便 prompt 写了排除标准,偶尔会漏出英文
    //      虚词 / 编程关键字 / 路径片段 / 中文虚词,后端再过一道。
    //   c. 凭空生造的词(原文里 case-insensitive substring count == 0)丢掉。
    //   d. 已存在 cfg.hotwords 的(case-insensitive)直接跳过。
    let existing_lc: std::collections::HashSet<String> =
        cfg.hotwords.iter().map(|w| w.to_lowercase()).collect();
    // freq 比较走 case-insensitive —— LLM 给的 case 跟原文不一定一致
    // (LLM 返回 "Claude" 而原文里全是 "claude" 是常见情况),纯 case-sensitive
    // matches 会让这种合理候选 freq=0 被默默丢掉。
    let text_lc = text.to_lowercase();
    // (form, freq) — 保留 freq 最大的 form
    let mut by_lc: std::collections::HashMap<String, (String, u32)> =
        std::collections::HashMap::new();
    for w in &llm_words {
        let trimmed = w.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_blacklisted(trimmed) {
            continue;
        }
        let lc = trimmed.to_lowercase();
        if existing_lc.contains(&lc) {
            continue;
        }
        let freq = text_lc.matches(&lc).count() as u32;
        if freq == 0 {
            continue;
        }
        by_lc
            .entry(lc)
            .and_modify(|e| {
                if freq > e.1 {
                    *e = (trimmed.to_string(), freq);
                }
            })
            .or_insert((trimmed.to_string(), freq));
    }
    let mut out: Vec<HotwordCandidate> = by_lc
        .into_values()
        .map(|(word, freq)| HotwordCandidate {
            word,
            freq,
            suggested: true,
        })
        .collect();

    // 按 freq 降序;同 freq 按字典序兜底
    out.sort_by(|a, b| b.freq.cmp(&a.freq).then_with(|| a.word.cmp(&b.word)));

    tracing::info!(
        llm_returned = llm_words.len(),
        after_postprocess = out.len(),
        "hotwords: 后处理完成"
    );
    Ok(out)
}

/// 后端兜底黑名单 —— LLM prompt 里已经写了排除标准,但偶尔会漏出常见垃圾
/// 词(虚词 / 编程关键字 / 路径片段 / 命令前缀 / 中文虚词)。这里 case-
/// insensitive 比较;短词(≤2 ASCII)直接拒;含 / 或 \ 的路径片段拒。
fn is_blacklisted(word: &str) -> bool {
    let trimmed = word.trim();
    if trimmed.is_empty() {
        return true;
    }
    // 路径 / URL 片段
    if trimmed.contains('/') || trimmed.contains('\\') {
        return true;
    }
    let lc = trimmed.to_lowercase();
    // 1-2 字符 ASCII 直接拒(噪声多;长度 1 的中文也拒,但中文一般 >=2 byte
    // 长度,这里按 chars().count() 判断更准)
    let char_count = trimmed.chars().count();
    if char_count <= 1 {
        return true;
    }
    if char_count == 2 && trimmed.is_ascii() {
        return true;
    }
    HOTWORD_BLACKLIST.iter().any(|w| *w == lc)
}

/// 常见英文虚词 / 编程关键字 / 命令前缀 / 中文虚词。LLM 偶尔挑出来这些,
/// 后端兜底过滤。case-insensitive 比较(全部 lowercase)。
const HOTWORD_BLACKLIST: &[&str] = &[
    // 英文虚词 / 高频普通词
    "the",
    "and",
    "or",
    "but",
    "if",
    "else",
    "when",
    "for",
    "while",
    "this",
    "that",
    "these",
    "those",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "have",
    "has",
    "had",
    "of",
    "in",
    "on",
    "at",
    "by",
    "to",
    "from",
    "with",
    "as",
    "into",
    "you",
    "your",
    "we",
    "our",
    "they",
    "them",
    "their",
    "it",
    "its",
    "what",
    "which",
    "who",
    "where",
    "why",
    "how",
    "all",
    "any",
    "some",
    "no",
    "not",
    "only",
    "so",
    "than",
    "too",
    "very",
    "can",
    "will",
    "just",
    "should",
    "now",
    "out",
    "up",
    "down",
    "off",
    "over",
    "under",
    "more",
    "most",
    "other",
    "such",
    "may",
    "might",
    "about",
    "after",
    "before",
    "between",
    "during",
    "through",
    "above",
    "below",
    "again",
    "still",
    "also",
    "even",
    "many",
    "much",
    "few",
    "less",
    "than",
    "thus",
    "hence",
    // 编程通用关键字
    "function",
    "return",
    "class",
    "let",
    "const",
    "var",
    "fn",
    "pub",
    "use",
    "mod",
    "import",
    "export",
    "type",
    "struct",
    "enum",
    "impl",
    "trait",
    "self",
    "true",
    "false",
    "null",
    "none",
    "match",
    "case",
    "switch",
    "break",
    "continue",
    "throw",
    "catch",
    "try",
    "finally",
    "async",
    "await",
    "yield",
    "new",
    "delete",
    "void",
    "int",
    "string",
    "bool",
    "float",
    "double",
    "char",
    "byte",
    "long",
    "short",
    "static",
    "final",
    "private",
    "public",
    "protected",
    "abstract",
    "interface",
    "extends",
    "implements",
    "super",
    "default",
    "package",
    "module",
    "namespace",
    "using",
    "begin",
    "end",
    "do",
    "then",
    "loop",
    "iter",
    // 命令 / 工具 前缀
    "git",
    "npm",
    "pnpm",
    "yarn",
    "cargo",
    "make",
    "cmake",
    "docker",
    "kubectl",
    "ssh",
    "scp",
    "curl",
    "wget",
    "ls",
    "cd",
    "rm",
    "mv",
    "cp",
    "cat",
    "echo",
    "grep",
    "find",
    "sed",
    "awk",
    "tar",
    "gzip",
    "zip",
    "ps",
    "top",
    "kill",
    "sudo",
    "chmod",
    "chown",
    "diff",
    "patch",
    "vim",
    "nano",
    "tmux",
    "screen",
    "bash",
    "zsh",
    "sh",
    "node",
    "python",
    "ruby",
    "java",
    // 路径 / 文件名片段
    "src",
    "lib",
    "bin",
    "tmp",
    "var",
    "etc",
    "usr",
    "doc",
    "test",
    "tests",
    "build",
    "dist",
    "target",
    "node_modules",
    "vendor",
    "pkg",
    "cmd",
    "main",
    "index",
    "readme",
    // 中文虚词 / 高频但无信息词
    "这个",
    "那个",
    "什么",
    "怎么",
    "如何",
    "为什么",
    "因为",
    "所以",
    "但是",
    "然后",
    "可能",
    "应该",
    "需要",
    "可以",
    "已经",
    "正在",
    "还是",
    "或者",
    "也许",
    "其实",
    "比如",
    "例如",
    "总之",
    "总的",
    "目前",
    "现在",
    "以前",
    "之前",
    "之后",
    "时候",
    "时间",
    "事情",
    "问题",
    "情况",
    "方面",
    "方式",
    "方法",
    "感觉",
    "觉得",
    "知道",
    "明白",
    "看到",
    "听到",
    "想到",
    "想想",
    "试试",
    "看看",
    "用户",
    "我们",
    "你们",
    "他们",
    "自己",
    "大家",
    "一下",
    "一个",
    "一些",
    "这样",
    "那样",
    "还有",
    "另外",
    // ASR 后端 / 语种通用词(单独可能模糊)
    "test",
    "example",
    "todo",
    "fixme",
    "note",
    "log",
    "logs",
    "debug",
    "info",
    "warn",
    "warning",
    "error",
    "fatal",
    "data",
    "result",
    "value",
    "name",
    "key",
    "size",
    "length",
    "count",
    "item",
    "list",
    "array",
    "map",
    "set",
    "dict",
    "tuple",
    "stream",
    "buffer",
    "input",
    "output",
    "request",
    "response",
    "client",
    "server",
    "config",
    "context",
    "default",
    "options",
    "params",
    "args",
    "argv",
    "env",
    "path",
    "file",
    "dir",
    "directory",
];

/// 把用户在 modal 里勾选的词追加到 cfg.hotwords。前端可以自己合 cfg.hotwords
/// 然后调 save_config —— 提供本命令是为了避免前端做 dedup / save 这种
/// 跟存储格式相关的逻辑,后端集中处理。
#[tauri::command]
pub fn add_hotwords(
    words: Vec<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<usize, String> {
    use std::collections::HashSet;
    let mut new_cfg = (*state.snapshot()).clone();
    // 保留原顺序追加新词,case-insensitive 去重(跟 hotwords 模块保持一致)
    let mut seen: HashSet<String> = new_cfg.hotwords.iter().map(|w| w.to_lowercase()).collect();
    let mut added = 0;
    for w in words {
        let trimmed = w.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lc = trimmed.to_lowercase();
        if seen.insert(lc) {
            new_cfg.hotwords.push(trimmed.to_string());
            added += 1;
        }
    }
    new_cfg.save().map_err(|e| e.to_string())?;
    state.replace(new_cfg);
    let _ = app.emit("config-updated", ());
    Ok(added)
}
