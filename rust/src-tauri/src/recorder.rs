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
/// 停止音量推送 + 关闭悬浮 indicator。
struct RecordingGuard {
    app: AppHandle,
    level_stop: Arc<AtomicBool>,
}

impl Drop for RecordingGuard {
    fn drop(&mut self) {
        self.level_stop.store(true, Ordering::Relaxed);
        let hide_app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            crate::indicator::hide(&hide_app);
        });
    }
}

fn scopeguard(app: AppHandle, level_stop: Arc<AtomicBool>) -> RecordingGuard {
    RecordingGuard { app, level_stop }
}

/// 切换录音状态：首次调用开始，再次调用停止。
pub fn toggle(app: AppHandle, cfg: Arc<crate::config::Config>) {
    let rec_state = recording();
    if rec_state.active.load(Ordering::Relaxed) {
        // 正在录音，第二次按热键 → 停止
        let had_tx = rec_state.stop_tx.lock().take().map(|tx| tx.send(()));
        tracing::info!(send_result = ?had_tx, "toggle: 请求停止录音");
        return;
    }

    tracing::info!("toggle: 请求开始录音");
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    *rec_state.stop_tx.lock() = Some(stop_tx);
    rec_state.active.store(true, Ordering::Relaxed);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app_handle.clone(), cfg, stop_rx).await {
            tracing::error!(error = ?e, "录音流程失败");
        }
        recording().active.store(false, Ordering::Relaxed);
        let _ = app_handle.emit("recording-stopped", ());
    });
}

async fn run(
    app: AppHandle,
    cfg: Arc<crate::config::Config>,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    tracing::info!("开始录音");
    let started_at = std::time::Instant::now();

    let _ = app.emit("recording-started", ());
    crate::beep::start();
    // indicator 窗口创建 + tauri-nspanel to_panel() 必须在主线程，不能在 worker
    let show_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::indicator::show(&show_app);
    });

    // guard：无论正常 / 提前 return / panic，都关闭 indicator + stop level task
    let level_stop = Arc::new(AtomicBool::new(false));
    let _guard = scopeguard(app.clone(), Arc::clone(&level_stop));

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

    let raw_text = if is_streaming(&cfg.asr_provider) {
        run_stream(app.clone(), &rec, &cfg, stop_rx).await?
    } else {
        run_batch(&rec, &cfg, stop_rx).await?
    };

    // guard drop 时自动做：level_stop + hide indicator

    if raw_text.trim().is_empty() {
        tracing::warn!("未识别到内容");
        return Ok(());
    }
    tracing::info!(text = %raw_text, "识别结果");

    let corrected = match correct::correct(&raw_text, &cfg).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = ?e, "纠错失败，使用原文");
            raw_text.clone()
        }
    };
    if corrected != raw_text {
        tracing::info!(text = %corrected, "纠错结果");
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

    tracing::info!(text = %final_text, "输入文字");
    if let Err(e) = input::type_text(&final_text) {
        tracing::error!(error = ?e, "键盘输入失败");
    }
    Ok(())
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
