//! 录音 → ASR → 纠错 → 热词 → 输入 主流程。
//! 对应 Go 版的 main.go `handleRecord` / `toggleRecording`。

use crate::asr::{self, is_streaming};
use crate::audio::{to_wav, Recorder};
use crate::{correct, history, input};
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::AppHandle;
use tauri::Emitter;

#[derive(serde::Serialize, Clone)]
struct RecordingStartedPayload<'a> {
    hotkey: &'a str,
    trigger_mode: &'a str,
    double_tap_modifier: &'a str,
}

/// 录音结束方式:正常停止走后续 ASR/AI/输出,取消则丢弃一切
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Stop,
    Cancel,
}

/// 全局录音状态:避免并发录音 + 支持 toggle/cancel
pub struct RecordingState {
    pub active: AtomicBool,
    pub signal_tx: Mutex<Option<tokio::sync::oneshot::Sender<StopReason>>>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            signal_tx: Mutex::new(None),
        }
    }
}

static RECORDING_CELL: OnceLock<RecordingState> = OnceLock::new();

fn recording() -> &'static RecordingState {
    RECORDING_CELL.get_or_init(RecordingState::new)
}

/// Drop guard：任何路径（正常返回 / ? / panic）结束 run 函数时，
/// 停止音量推送 + 关闭悬浮 indicator。
///
/// panel 输出模式的识别结果由独立的 result 窗口显示，indicator 仍会被 hide。
struct RecordingGuard {
    app: AppHandle,
    level_stop: Arc<AtomicBool>,
}

impl Drop for RecordingGuard {
    fn drop(&mut self) {
        self.level_stop.store(true, Ordering::Relaxed);
        // 兜底重置 active：即便 run().await panic 也不会让 active 卡在 true，
        // 否则下一次 toggle 会以为还在录音
        recording().active.store(false, Ordering::Relaxed);
        // ESC 临时热键的注册/注销已经迁到 keyboard backend 内部(状态机进/出
        // recording 状态时由 backend.rs::manage_esc 自动处理),这里不再插手。
        let hide_app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            crate::indicator::hide(&hide_app);
        });
    }
}

fn scopeguard(app: AppHandle, level_stop: Arc<AtomicBool>) -> RecordingGuard {
    RecordingGuard { app, level_stop }
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
    let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<StopReason>();
    *rec_state.signal_tx.lock() = Some(signal_tx);
    rec_state.active.store(true, Ordering::Relaxed);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // run 内部成功路径会在 ASR 完成后自行 emit "recording-stopped"，
        // 这里只在失败路径补发一次（保证 indicator state 机最终能离开
        // recording view；紧接着 guard drop 会 hide 窗口）
        if let Err(e) = run(app_handle.clone(), cfg, signal_rx).await {
            tracing::error!(error = ?e, "录音流程失败");
            let _ = app_handle.emit("recording-stopped", ());
        }
        recording().active.store(false, Ordering::Relaxed);
    });
}

/// 停止当前录音；若未在录音则 no-op。push-to-talk 松开时调，VAD / toggle 也用。
pub fn stop() {
    send_signal(StopReason::Stop);
}

/// 取消当前录音(丢弃录音内容,不走 ASR/AI/输出)。ESC 热键 + indicator 按钮都调它。
pub fn cancel() {
    send_signal(StopReason::Cancel);
}

fn send_signal(reason: StopReason) {
    let had_tx = recording()
        .signal_tx
        .lock()
        .take()
        .map(|tx| tx.send(reason));
    tracing::info!(?reason, send_result = ?had_tx, "recorder: 请求结束录音");
}

async fn run(
    app: AppHandle,
    cfg: Arc<crate::config::Config>,
    signal_rx: tokio::sync::oneshot::Receiver<StopReason>,
) -> Result<()> {
    tracing::info!("开始录音");
    let started_at = std::time::Instant::now();

    // 带上当前热键 + 触发模式,让悬浮窗底部"再按 xxx 结束"提示跟着 config 动态渲染
    let _ = app.emit(
        "recording-started",
        RecordingStartedPayload {
            hotkey: &cfg.hotkey,
            trigger_mode: &cfg.trigger_mode,
            double_tap_modifier: &cfg.double_tap_modifier,
        },
    );
    crate::beep::start();
    // indicator 窗口创建 + tauri-nspanel to_panel() 必须在主线程，不能在 worker。
    // 关上一次 panel 模式遗留的结果窗——否则用户没手动关就按热键,两个窗口会同时显示,
    // 而且结果窗在上面还可能拦住悬浮窗视觉。
    // ESC 临时热键现在由 keyboard backend 自动管理(emit Start 时注册),这里不再注册。
    let show_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::result::hide(&show_app);
        crate::indicator::show(&show_app);
    });

    // guard：无论正常 / 提前 return / panic，都关闭 indicator + stop level task
    let level_stop = Arc::new(AtomicBool::new(false));
    let _guard = scopeguard(app.clone(), Arc::clone(&level_stop));

    let rec = Arc::new(Recorder::new(cfg.gain, &cfg.device_name, cfg.voice_enhance));

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
        // silero 模型按需下载(~640KB,首次进设置/启用 VAD 应当已经触发);
        // 没下载就回退到 RMS,体感差但不崩
        let model_ready = crate::vad::is_available();
        if !model_ready {
            // 后台尝试下载,本次录音先用 RMS 兜底
            tokio::spawn(async {
                if let Err(e) = crate::vad::download_if_needed().await {
                    tracing::warn!(error = ?e, "silero-vad 下载失败,本次 VAD 走 RMS 兜底");
                }
            });
        }
        tokio::spawn(async move {
            if model_ready {
                run_vad_silero(vad_rec, vad_stop_cb, vad_silence_ms, vad_threshold).await;
            } else {
                tracing::info!("silero-vad 模型未就绪,本次 VAD 走 RMS 兜底");
                run_vad_rms(vad_rec, vad_stop_cb, vad_silence_ms, 0.015).await;
            }
        });
    }

    let (reason, raw_text, asr_ms) = if is_streaming(&cfg.asr_provider) {
        run_stream(app.clone(), &rec, &cfg, signal_rx).await?
    } else {
        run_batch(app.clone(), &rec, &cfg, signal_rx).await?
    };

    // 取消路径:悬浮窗直接消失 (guard drop 里 hide),不走 processing/result,不写历史
    if reason == StopReason::Cancel {
        tracing::info!("录音已取消,丢弃识别结果");
        return Ok(());
    }

    // recording-stopped 已经由 run_stream / run_batch 在收到 stop 信号后立刻
    // emit 了 —— 不要在这等 ASR 跑完才发,会让用户盯着"录音中"画面等 5–10s。

    // guard drop 时自动做：level_stop + hide indicator

    if raw_text.trim().is_empty() {
        tracing::warn!("未识别到内容");
        return Ok(());
    }
    tracing::info!(text = %raw_text, "识别结果");

    let profile = cfg.active_profile();
    let backend = cfg.backend_by_id(&profile.backend_id);
    let timeout_secs = cfg.correct_timeout_secs();
    let timeout_ms = (timeout_secs * 1000) as i64;
    // 润色 timing:off profile / 非超时 Err 都视为 0(未实际产生可观测延时);
    // 超时场景例外 —— polish_ms 记 timeout_ms 入 p99,polish_timeout=1 单独统计,
    // 否则 p99 会漏掉"老是卡到上限"的 model 分布
    let polish_start = std::time::Instant::now();
    // profile.backend_id 空 / 找不到 backend → 用户主动选了"关闭",原文直出。
    // 等价于老的 backend.mode = "off",但语义在 profile 层面表达,每个 profile
    // 各自决定要不要润色。
    let polish_result = match backend {
        Some(b) if b.is_active() => {
            correct::correct(&raw_text, profile, b, timeout_secs, &cfg.hotwords).await
        }
        _ => Ok(raw_text.clone()),
    };
    let elapsed_ms = polish_start.elapsed().as_millis() as i64;
    let polish_provider = compute_polish_provider(backend);
    // 统计意义上的"润色生效":有 backend 且 backend 配置有效。空 backend_id /
    // backend.is_active() = false 都视为没跑润色(p99 不计入,polish_model/mode 留空)。
    let profile_active = backend.map(|b| b.is_active()).unwrap_or(false);
    let polish_model = backend.map(|b| b.model.clone()).unwrap_or_default();
    let (corrected, polish_ms, polish_model, polish_mode, polish_timeout) = match polish_result {
        Ok(c) if !profile_active => (c, 0i64, String::new(), String::new(), false),
        Ok(c) => (c, elapsed_ms, polish_model, polish_provider.clone(), false),
        Err(e) => {
            // reqwest timeout 是硬超时(到时间抛 Err),用 elapsed 接近 timeout 作近似判定;
            // 200ms 容错足够区分 timeout 和其他早期 Err(400 / 网络断 / 反序列化失败等)
            let is_timeout = profile_active && elapsed_ms >= timeout_ms.saturating_sub(200);
            if is_timeout {
                tracing::warn!(error = ?e, profile = %profile.name, elapsed_ms, "润色超时,记入 p99");
                (
                    raw_text.clone(),
                    timeout_ms,
                    polish_model,
                    polish_provider.clone(),
                    true,
                )
            } else {
                tracing::warn!(error = ?e, profile = %profile.name, "润色失败,使用原文");
                (raw_text.clone(), 0, String::new(), String::new(), false)
            }
        }
    };
    if corrected != raw_text {
        tracing::info!(text = %corrected, profile = %profile.name, "润色结果");
    }

    // 老的字符串替换路径已删:词典(cfg.hotwords)现在在两条路上发挥作用
    //   ① ASR boosting(本地 / 部分云端 ASR 后端读 cfg.hotwords)
    //   ② LLM 校正 prompt 注入(correct::correct 在上一步用 cfg.hotwords 渲染 glossary 段落)
    // 这里直接把校正结果当 final,不再事后字符串替换。
    let final_text = corrected;

    let duration_ms = started_at.elapsed().as_millis() as i64;
    let stats_provider = cfg.provider_id_for_stats();
    history::save(history::SaveEntry {
        raw: &raw_text,
        corrected: &final_text,
        provider: &stats_provider,
        duration_ms,
        asr_ms,
        polish_ms,
        polish_model: &polish_model,
        polish_mode: &polish_mode,
        polish_timeout,
    });
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
            tracing::info!(text = %final_text, "panel 输出：结果显示在独立结果窗口");
            // 必须主线程：Tauri 的 window show/focus API
            let show_app = app.clone();
            let show_text = final_text.clone();
            let _ = app.run_on_main_thread(move || {
                crate::result::show(&show_app, &show_text);
            });
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

/// 编译时关掉 local-asr feature 时,silero 不可用,直接 fallback 到 RMS。
#[cfg(not(feature = "local-asr"))]
async fn run_vad_silero(
    rec: Arc<Recorder>,
    stop_signal: Arc<AtomicBool>,
    silence_ms: u64,
    _threshold: f32,
) {
    run_vad_rms(rec, stop_signal, silence_ms, 0.015).await;
}

/// silero-vad VAD:用神经网络判断当前是否在说话,持续静音超过 silence_ms 自动停。
/// 比 RMS 门限对气声/键盘噪声鲁棒得多(silero ROC-AUC 0.96 vs WebRTC 0.73)。
///
/// 实现:从 Recorder buffer 周期 peek 增量 PCM 喂给 detector,detector.detected()
/// 是当前是否在说话的硬判定。silence_ms 由外层累计触发(沿用现有语义)。
#[cfg(feature = "local-asr")]
async fn run_vad_silero(
    rec: Arc<Recorder>,
    stop_signal: Arc<AtomicBool>,
    silence_ms: u64,
    threshold: f32,
) {
    const TICK_MS: u64 = 50;
    const SPEECH_START_MS: u64 = 300;

    let detector = match crate::vad::create_detector(threshold, 700) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = ?e, "silero-vad 初始化失败,回退到 RMS");
            run_vad_rms(rec, stop_signal, silence_ms, 0.015).await;
            return;
        }
    };

    tracing::info!(threshold, silence_ms, "VAD: silero 启动");

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(TICK_MS));
    let mut consumed_bytes: usize = 0;
    let mut started_speaking = false;
    let mut speech_accum_ms: u64 = 0;
    let mut silence_accum_ms: u64 = 0;
    let mut log_accum_ms: u64 = 0;

    while !stop_signal.load(Ordering::Relaxed) {
        interval.tick().await;
        let (new_bytes, total) = rec.peek_pcm_since(consumed_bytes);
        consumed_bytes = total;
        if !new_bytes.is_empty() {
            // i16 LE → f32 [-1, 1]
            let f32_samples: Vec<f32> = new_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect();
            detector.accept_waveform(&f32_samples);
        }
        let detected = detector.detected();

        log_accum_ms += TICK_MS;
        if log_accum_ms >= 1000 {
            tracing::debug!(
                detected,
                started_speaking,
                silence_accum_ms,
                "VAD silero tick",
            );
            log_accum_ms = 0;
        }

        if !started_speaking {
            if detected {
                speech_accum_ms += TICK_MS;
                if speech_accum_ms >= SPEECH_START_MS {
                    started_speaking = true;
                    tracing::info!("VAD silero: 检测到说话起点");
                }
            } else {
                speech_accum_ms = 0;
            }
        } else if detected {
            // 类比老 RMS spike 惩罚:detected 时给 silence accumulator 扣 2*TICK_MS
            silence_accum_ms = silence_accum_ms.saturating_sub(2 * TICK_MS);
        } else {
            silence_accum_ms += TICK_MS;
            if silence_accum_ms >= silence_ms {
                tracing::info!(silence_ms, "VAD silero: 静音超时,自动停止");
                stop();
                return;
            }
        }
    }
    tracing::info!(started_speaking, "VAD silero: 随录音一起结束");
}

/// 老 RMS 门限 VAD,作为 silero 不可用(模型没下载/初始化失败)的兜底。
/// 起点判定:累计 300ms 高于阈值才算"开始说话",避免还没开口就停。
async fn run_vad_rms(
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
    signal_rx: tokio::sync::oneshot::Receiver<StopReason>,
) -> Result<(StopReason, String, i64)> {
    let pcm_rx = rec.start_stream();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    // partial 只 emit 给 indicator 预览,不再实时 type 到光标。
    // 老路径"流式 partial 实时打字 + 停止后 backspace 全删 + 再写最终"会在
    // 焦点窗口里"跳"两次,开 LLM 润色时尤其明显(润色后的文本跟原文不同,
    // 全删后又得重写)。统一用"只在 indicator 预览,识别+润色完一次性 type
    // 最终结果"的 UX,跟 macOS 系统 dictation / type4me 等竞品一致。
    let partial_app = app.clone();
    let on_partial = Box::new(move |text: String| {
        let _ = partial_app.emit("asr-partial", &text);
    });

    let cfg_clone = cfg.clone();
    let asr_task = tokio::spawn(async move {
        asr::transcribe_stream(&cfg_clone, pcm_rx, on_partial, ready_tx).await
    });

    // 等 WebSocket 就绪
    let _ = ready_rx.await;
    rec.start()?;

    // 等用户停止或取消
    let reason = signal_rx.await.unwrap_or(StopReason::Stop);
    crate::beep::stop();
    rec.stop_stream();
    rec.stop();

    // partial 不再 type 到光标,所以也无需 backspace 清场 —— 焦点窗口
    // 在录音期间始终干净。

    if reason == StopReason::Cancel {
        // 不再等 ASR 最终结果,任务会因 pcm_rx 关闭自然退出
        asr_task.abort();
        return Ok((reason, String::new(), 0));
    }

    // 用户按了停止 + 不是取消 → 立刻让悬浮窗切 processing view,不要让用户
    // 等 asr_task 收尾包(可能 200ms-1s)才看到状态变化
    let _ = app.emit("recording-stopped", ());

    // asr_ms 跟 run_batch 对齐:只测纯 ASR 调用(等 asr_task 收尾包 + 出
    // final),不含 rec.stop_stream / rec.stop / delete_chars 等基础开销 ——
    // 否则两边一比 stream 数字偏大几十 ms,失去可比性。
    let asr_start = std::time::Instant::now();
    let final_text = asr_task.await??;
    let asr_ms = asr_start.elapsed().as_millis() as i64;
    Ok((reason, final_text, asr_ms))
}

async fn run_batch(
    app: AppHandle,
    rec: &Arc<Recorder>,
    cfg: &crate::config::Config,
    signal_rx: tokio::sync::oneshot::Receiver<StopReason>,
) -> Result<(StopReason, String, i64)> {
    rec.start()?;
    let reason = signal_rx.await.unwrap_or(StopReason::Stop);
    crate::beep::stop();
    let pcm = rec.stop();

    if reason == StopReason::Cancel {
        return Ok((reason, String::new(), 0));
    }

    // 用户按了停止 + 不是取消 → 立刻让悬浮窗切到 processing view,不要让用户
    // 等 transcribe_batch 跑完(本地大模型可能 5–10s)才看到状态变化
    let _ = app.emit("recording-stopped", ());

    let wav = to_wav(&pcm, cfg.voice_enhance);
    if wav.len() < 100 {
        tracing::warn!(wav_bytes = wav.len(), "未录到声音");
        return Ok((reason, String::new(), 0));
    }
    // debug：保存最近一次录音 WAV 到 /tmp，方便用 afplay 听
    let _ = std::fs::write("/tmp/voice-claude-last.wav", &wav);
    tracing::info!(
        wav_bytes = wav.len(),
        path = "/tmp/voice-claude-last.wav",
        "识别中"
    );
    // 批处理 ASR:整个 transcribe_batch 调用即用户等待时长
    let asr_start = std::time::Instant::now();
    let text = asr::transcribe_batch(cfg, &wav).await?;
    let asr_ms = asr_start.elapsed().as_millis() as i64;
    Ok((reason, text, asr_ms))
}

/// 为「状态」页展示推导一个 provider 标识。
///
/// - ollama / openrouter mode:直接用 mode 名(URL 固定,没必要拆 host)
/// - cloud mode:多 base URL 场景下 mode 字面值("cloud")辨识度不够,
///   从 backend.url 提取 host(如 api.groq.com)作为 provider
/// - off / 未知 mode / backend = None:空字符串(不展示徽章)
fn compute_polish_provider(backend: Option<&crate::config::LlmBackend>) -> String {
    use crate::config::{POLISH_MODE_CLOUD, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER};
    let Some(backend) = backend else {
        return String::new();
    };
    match backend.mode.as_str() {
        POLISH_MODE_OLLAMA => "ollama".to_string(),
        POLISH_MODE_OPENROUTER => "openrouter".to_string(),
        POLISH_MODE_CLOUD => host_of(&backend.url).unwrap_or_else(|| "cloud".to_string()),
        _ => String::new(),
    }
}

/// 从 URL 提取 host(含端口),失败返回 None。
/// 不引入 url crate,手写一个够用的:截 "://" 和下一个 '/' 之间。
fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = after_scheme.split('/').next()?.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        LlmBackend, POLISH_MODE_CLOUD, POLISH_MODE_OLLAMA, POLISH_MODE_OPENROUTER,
    };

    fn backend(mode: &str, url: &str) -> LlmBackend {
        let mut b = LlmBackend::default_named("t", "t");
        b.mode = mode.to_string();
        b.url = url.to_string();
        b
    }

    #[test]
    fn provider_ollama_is_literal() {
        let b = backend(POLISH_MODE_OLLAMA, "http://localhost:11434/api/generate");
        assert_eq!(compute_polish_provider(Some(&b)), "ollama");
    }

    #[test]
    fn provider_openrouter_is_literal() {
        let b = backend(POLISH_MODE_OPENROUTER, "https://openrouter.ai/api/v1");
        assert_eq!(compute_polish_provider(Some(&b)), "openrouter");
    }

    #[test]
    fn provider_cloud_uses_host() {
        let b = backend(
            POLISH_MODE_CLOUD,
            "https://api.groq.com/openai/v1/chat/completions",
        );
        assert_eq!(compute_polish_provider(Some(&b)), "api.groq.com");
    }

    #[test]
    fn provider_cloud_fallback_on_bad_url() {
        let b = backend(POLISH_MODE_CLOUD, "");
        assert_eq!(compute_polish_provider(Some(&b)), "cloud");
    }

    #[test]
    fn provider_off_is_empty() {
        let b = backend("off", "");
        assert_eq!(compute_polish_provider(Some(&b)), "");
    }

    #[test]
    fn provider_none_is_empty() {
        assert_eq!(compute_polish_provider(None), "");
    }

    #[test]
    fn host_of_handles_port_and_path() {
        assert_eq!(
            host_of("http://localhost:11434/api"),
            Some("localhost:11434".into())
        );
        assert_eq!(
            host_of("https://api.deepseek.com/v1"),
            Some("api.deepseek.com".into())
        );
        assert_eq!(
            host_of("openrouter.ai/api/v1"),
            Some("openrouter.ai".into())
        );
        assert!(host_of("").is_none());
    }
}
