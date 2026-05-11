//! 录音 → ASR → 纠错 → 热词 → 输入 主流程。
//! 对应 Go 版的 main.go `handleRecord` / `toggleRecording`。

use crate::asr::{self, is_streaming};
use crate::audio::{to_wav, Recorder};
use crate::{correct, history, hotwords, input};
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::AppHandle;
use tauri::Emitter;

/// 全局录音状态：避免并发录音 + 支持 toggle
pub struct RecordingState {
    pub active: AtomicBool,
    pub stop_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            stop_tx: Mutex::new(None),
        }
    }
}

static RECORDING_CELL: OnceLock<RecordingState> = OnceLock::new();

fn recording() -> &'static RecordingState {
    RECORDING_CELL.get_or_init(RecordingState::new)
}

/// Drop guard：任何路径（正常返回 / ? / panic）结束 run 函数时，
/// 停止音量推送 + 关闭悬浮 indicator（除非 keep_indicator 设为 true）。
struct RecordingGuard {
    app: AppHandle,
    level_stop: Arc<AtomicBool>,
    /// panel 输出模式下，成功识别后把它 store(true)，guard drop 时就不 hide 悬浮窗
    keep_indicator: Arc<AtomicBool>,
}

impl Drop for RecordingGuard {
    fn drop(&mut self) {
        self.level_stop.store(true, Ordering::Relaxed);
        if self.keep_indicator.load(Ordering::Relaxed) {
            return;
        }
        let hide_app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            crate::indicator::hide(&hide_app);
        });
    }
}

fn scopeguard(
    app: AppHandle,
    level_stop: Arc<AtomicBool>,
    keep_indicator: Arc<AtomicBool>,
) -> RecordingGuard {
    RecordingGuard {
        app,
        level_stop,
        keep_indicator,
    }
}

/// 切换录音状态（toggle 模式）：首次调用开始，再次调用停止。
pub fn toggle(app: AppHandle, cfg: Arc<crate::config::Config>) {
    if recording().active.load(Ordering::Relaxed) {
        stop();
    } else {
        start(app, cfg);
    }
}

/// 开始录音；若已在录音则 no-op。push-to-talk 按下时调。
pub fn start(app: AppHandle, cfg: Arc<crate::config::Config>) {
    let rec_state = recording();
    if rec_state.active.load(Ordering::Relaxed) {
        return;
    }
    tracing::info!("recorder: 请求开始录音");
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    *rec_state.stop_tx.lock() = Some(stop_tx);
    rec_state.active.store(true, Ordering::Relaxed);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // run 内部成功路径会在 ASR 完成后自行 emit "recording-stopped"，
        // 这里只在失败路径补发一次（保证 indicator state 机最终能离开
        // recording view；紧接着 guard drop 会 hide 窗口）
        if let Err(e) = run(app_handle.clone(), cfg, stop_rx).await {
            tracing::error!(error = ?e, "录音流程失败");
            let _ = app_handle.emit("recording-stopped", ());
        }
        recording().active.store(false, Ordering::Relaxed);
    });
}

/// 停止当前录音；若未在录音则 no-op。push-to-talk 松开时调，VAD / toggle 也用。
pub fn stop() {
    let had_tx = recording().stop_tx.lock().take().map(|tx| tx.send(()));
    tracing::info!(send_result = ?had_tx, "recorder: 请求停止录音");
}

async fn run(
    app: AppHandle,
    cfg: Arc<crate::config::Config>,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    tracing::info!("开始录音");
    let started_at = std::time::Instant::now();

    // 带上当前热键，让悬浮窗底部"再按 xxx 结束"提示跟着 config 动态渲染
    #[derive(serde::Serialize, Clone)]
    struct RecordingStartedPayload<'a> {
        hotkey: &'a str,
    }
    let _ = app.emit(
        "recording-started",
        RecordingStartedPayload {
            hotkey: &cfg.hotkey,
        },
    );
    crate::beep::start();
    // indicator 窗口创建 + tauri-nspanel to_panel() 必须在主线程，不能在 worker
    let show_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::indicator::show(&show_app);
    });

    // guard：无论正常 / 提前 return / panic，都关闭 indicator + stop level task
    let level_stop = Arc::new(AtomicBool::new(false));
    let keep_indicator = Arc::new(AtomicBool::new(false));
    let _guard = scopeguard(
        app.clone(),
        Arc::clone(&level_stop),
        Arc::clone(&keep_indicator),
    );

    let rec = Arc::new(Recorder::new(cfg.gain, &cfg.device_name));

    // 启动音量推送任务：30fps emit audio-level 给波形悬浮窗
    let level_rec = Arc::clone(&rec);
    let level_app = app.clone();
    let level_stop_cb = Arc::clone(&level_stop);
    let _level_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33));
        while !level_stop_cb.load(Ordering::Relaxed) {
            interval.tick().await;
            let lvl = level_rec.current_level();
            let _ = level_app.emit("audio-level", lvl);
        }
    });

    // 启动 VAD 任务：检测说话起点之后，连续静音超过阈值时长就触发停止
    if cfg.vad_enabled {
        let vad_rec = Arc::clone(&rec);
        let vad_stop_cb = Arc::clone(&level_stop);
        let vad_silence_ms = cfg.vad_silence_ms as u64;
        let vad_threshold = cfg.vad_threshold;
        tokio::spawn(async move {
            run_vad(vad_rec, vad_stop_cb, vad_silence_ms, vad_threshold).await;
        });
    }

    let raw_text = if is_streaming(&cfg.asr_provider) {
        run_stream(app.clone(), &rec, &cfg, stop_rx).await?
    } else {
        run_batch(&rec, &cfg, stop_rx).await?
    };

    // 录音 + ASR 已结束，进入后处理（润色 / 热词）阶段；panel 模式下
    // 悬浮窗切到 processing view。必须在 asr-final-text 之前发，否则 event
    // 顺序错位会让用户看到悬浮窗最终停在 processing（而不是 result）
    let _ = app.emit("recording-stopped", ());

    // guard drop 时自动做：level_stop + hide indicator

    if raw_text.trim().is_empty() {
        tracing::warn!("未识别到内容");
        return Ok(());
    }
    tracing::info!(text = %raw_text, "识别结果");

    let profile = cfg.active_profile();
    let corrected = match correct::correct(&raw_text, profile, cfg.correct_timeout_secs()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = ?e, profile = %profile.name, "润色失败，使用原文");
            raw_text.clone()
        }
    };
    if corrected != raw_text {
        tracing::info!(text = %corrected, profile = %profile.name, "润色结果");
    }

    let final_text = hotwords::apply(&corrected, &cfg.hotwords);
    if final_text != corrected {
        tracing::info!(text = %final_text, "热词替换");
    }

    let duration_ms = started_at.elapsed().as_millis() as i64;
    history::save(&raw_text, &final_text, &cfg.asr_provider, duration_ms);
    let _ = app.emit("history-updated", ());
    // 刷新托盘菜单的"最近 5 条"（必须主线程：Tauri 的 Menu/TrayIcon API）
    let refresh_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = crate::tray::refresh(&refresh_app);
    });

    use crate::config::{OUTPUT_MODE_CLIPBOARD, OUTPUT_MODE_PANEL};
    use tauri_plugin_clipboard_manager::ClipboardExt;
    match cfg.output_mode.as_str() {
        OUTPUT_MODE_PANEL => {
            tracing::info!(text = %final_text, "panel 输出：结果显示在悬浮窗");
            keep_indicator.store(true, Ordering::Relaxed);
            let _ = app.emit("asr-final-text", final_text.clone());
        }
        OUTPUT_MODE_CLIPBOARD => {
            tracing::info!(text = %final_text, "剪贴板输出");
            if let Err(e) = app.clipboard().write_text(final_text.clone()) {
                tracing::warn!(error = ?e, "剪贴板写入失败，fallback 键盘输入");
                if let Err(e2) = input::type_text(&final_text) {
                    tracing::error!(error = ?e2, "键盘输入也失败");
                }
            }
        }
        _ => {
            tracing::info!(text = %final_text, "键盘输入");
            if let Err(e) = input::type_text(&final_text) {
                tracing::error!(error = ?e, "键盘输入失败");
            }
        }
    }
    Ok(())
}

/// VAD：检测到说话起点后，连续 silence_ms 毫秒低于 threshold 自动触发停止。
/// 起点判定：累计 300ms 高于阈值才算"开始说话"，避免用户还没开口就停。
///
/// 诊断日志：
///   - info：启动参数、说话起点、静音触发、退出原因
///   - debug：每 1s 打印一次 RMS 和状态（开 log_level=debug 看）
async fn run_vad(
    rec: Arc<Recorder>,
    stop_signal: Arc<AtomicBool>,
    silence_ms: u64,
    threshold: f32,
) {
    const TICK_MS: u64 = 50;
    const SPEECH_START_MS: u64 = 300;

    tracing::info!(threshold, silence_ms, "VAD: 启动");

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(TICK_MS));
    let mut started_speaking = false;
    let mut speech_accum_ms: u64 = 0;
    let mut silence_accum_ms: u64 = 0;
    let mut max_observed_rms: f32 = 0.0;
    let mut log_accum_ms: u64 = 0;

    while !stop_signal.load(Ordering::Relaxed) {
        interval.tick().await;
        let lvl = rec.current_level();
        if lvl > max_observed_rms {
            max_observed_rms = lvl;
        }
        let above = lvl >= threshold;

        log_accum_ms += TICK_MS;
        if log_accum_ms >= 1000 {
            tracing::debug!(
                rms = lvl,
                max_rms = max_observed_rms,
                threshold,
                started_speaking,
                silence_accum_ms,
                "VAD tick",
            );
            log_accum_ms = 0;
        }

        if !started_speaking {
            if above {
                speech_accum_ms += TICK_MS;
                if speech_accum_ms >= SPEECH_START_MS {
                    started_speaking = true;
                    tracing::info!(rms = lvl, max_rms = max_observed_rms, "VAD: 检测到说话起点",);
                }
            } else {
                speech_accum_ms = 0;
            }
        } else if above {
            // spike 惩罚而不是清零。环境噪音偶尔越阈（键盘/鼠标/椅子声），
            // 旧逻辑会把积累的静音时间一次清 0，导致永远积不到 silence_ms。
            // 现改成每次 above 扣 2*TICK_MS，静音积累时每 tick +TICK_MS —
            // 偶发 spike 不致命，长时说话时会压回 0。
            silence_accum_ms = silence_accum_ms.saturating_sub(2 * TICK_MS);
        } else {
            silence_accum_ms += TICK_MS;
            if silence_accum_ms >= silence_ms {
                tracing::info!(
                    silence_ms,
                    max_rms = max_observed_rms,
                    "VAD: 静音超时，自动停止"
                );
                stop();
                return;
            }
        }
    }
    tracing::info!(
        started_speaking,
        max_rms = max_observed_rms,
        "VAD: 随录音一起结束",
    );
}

async fn run_stream(
    app: AppHandle,
    rec: &Arc<Recorder>,
    cfg: &crate::config::Config,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<String> {
    let pcm_rx = rec.start_stream();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    // 启动 ASR 任务
    let partial_app = app.clone();
    let partial_state = Arc::new(Mutex::new(0usize)); // 已输入的 rune 数
    let partial_state_cb = Arc::clone(&partial_state);

    let on_partial = Box::new(move |text: String| {
        let _ = partial_app.emit("asr-partial", &text);
        // 实时输入：先退格再 Type
        let prev = {
            let mut g = partial_state_cb.lock();
            let p = *g;
            *g = text.chars().count();
            p
        };
        let _ = input::delete_chars(prev);
        let _ = input::type_text(&text);
    });

    let cfg_clone = cfg.clone();
    let asr_task = tokio::spawn(async move {
        asr::transcribe_stream(&cfg_clone, pcm_rx, on_partial, ready_tx).await
    });

    // 等 WebSocket 就绪
    let _ = ready_rx.await;
    rec.start()?;

    // 等用户停止
    let _ = stop_rx.await;
    crate::beep::stop();
    rec.stop_stream();
    rec.stop();

    let final_text = asr_task.await??;
    // 把之前输入的中间结果全部删掉
    let prev = *partial_state.lock();
    let _ = input::delete_chars(prev);
    Ok(final_text)
}

async fn run_batch(
    rec: &Arc<Recorder>,
    cfg: &crate::config::Config,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<String> {
    rec.start()?;
    let _ = stop_rx.await;
    crate::beep::stop();
    let pcm = rec.stop();
    let wav = to_wav(&pcm);
    if wav.len() < 100 {
        tracing::warn!(wav_bytes = wav.len(), "未录到声音");
        return Ok(String::new());
    }
    // debug：保存最近一次录音 WAV 到 /tmp，方便用 afplay 听
    let _ = std::fs::write("/tmp/voice-claude-last.wav", &wav);
    tracing::info!(
        wav_bytes = wav.len(),
        path = "/tmp/voice-claude-last.wav",
        "识别中"
    );
    asr::transcribe_batch(cfg, &wav).await
}
