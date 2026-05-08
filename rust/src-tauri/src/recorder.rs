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

/// 切换录音状态：首次调用开始，再次调用停止。
pub fn toggle(app: AppHandle, cfg: Arc<crate::config::Config>) {
    let rec_state = recording();
    if rec_state.active.load(Ordering::Relaxed) {
        // 正在录音，停止
        if let Some(tx) = rec_state.stop_tx.lock().take() {
            let _ = tx.send(());
        }
        return;
    }

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    *rec_state.stop_tx.lock() = Some(stop_tx);
    rec_state.active.store(true, Ordering::Relaxed);

    let app_handle = app.clone();
    tokio::spawn(async move {
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

    let _ = app.emit("recording-started", ());

    let rec = Arc::new(Recorder::new(cfg.gain, &cfg.device_name));
    let raw_text = if is_streaming(&cfg.asr_provider) {
        run_stream(app.clone(), &rec, &cfg, stop_rx).await?
    } else {
        run_batch(&rec, &cfg, stop_rx).await?
    };

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

    history::save(&raw_text, &final_text, &cfg.asr_provider);
    let _ = app.emit("history-updated", ());

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
    let pcm = rec.stop();
    let wav = to_wav(&pcm);
    if wav.len() < 100 {
        tracing::warn!(wav_bytes = wav.len(), "未录到声音");
        return Ok(String::new());
    }
    tracing::info!(wav_bytes = wav.len(), "识别中");
    asr::transcribe_batch(cfg, &wav).await
}
