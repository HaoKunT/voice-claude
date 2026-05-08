//! 音频录制：cpal 跨平台录音，支持设备枚举、增益、WAV 打包、流式推送。
//! 对应 Go 版的 audio.go。

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// 16k 16bit mono PCM，和 Go 版一致
pub const SAMPLE_RATE: u32 = 16000;
pub const CHANNELS: u16 = 1;
pub const BITS_PER_SAMPLE: u16 = 16;

#[derive(Debug, Clone)]
pub struct CaptureDevice {
    pub name: String,
}

/// 枚举所有录音设备。
pub fn list_capture_devices() -> Result<Vec<CaptureDevice>> {
    let host = cpal::default_host();
    let devices = host.input_devices().context("list input devices")?;
    Ok(devices
        .filter_map(|d| d.name().ok().map(|name| CaptureDevice { name }))
        .collect())
}

pub struct Recorder {
    gain: u8,
    device_name: String,
    buffer: Arc<Mutex<Vec<u8>>>,
    stream_tx: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    current_level: Arc<AtomicU32>, // float32 bits of RMS level (0.0-1.0)
    stream: Arc<Mutex<Option<cpal::Stream>>>,
}

// cpal::Stream 不是 Send/Sync，但 Mutex 保证的是"同时只有一个 thread 用"。
// 在我们场景下 Stream 由主线程创建和销毁，不跨线程移动。
// 使用 SendWrapper 或者在 Mutex 外设计能规避。
// 暂时通过 unsafe impl 绕过，后续如果 cpal 支持 Send 再去掉。
unsafe impl Send for Recorder {}
unsafe impl Sync for Recorder {}

impl Recorder {
    #[allow(clippy::arc_with_non_send_sync)] // Recorder 上已经 unsafe impl Send/Sync，stream 由固定线程持有
    pub fn new(gain: u8, device_name: &str) -> Self {
        Self {
            gain: gain.max(1),
            device_name: device_name.to_string(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream_tx: Arc::new(Mutex::new(None)),
            current_level: Arc::new(AtomicU32::new(0)),
            stream: Arc::new(Mutex::new(None)),
        }
    }

    /// 当前归一化 RMS 音量（0-1），供波形动画读取。
    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.current_level.load(Ordering::Relaxed))
    }

    /// 开始流式录音：返回 PCM 块接收端。调用方通过 drop Receiver 或 stop_stream 停止。
    pub fn start_stream(&self) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(32);
        *self.stream_tx.lock() = Some(tx);
        rx
    }

    /// 开始录音（必须在 start_stream 之后才能流式推送）。
    ///
    /// 兼容各种麦克风：用设备默认 config（通常 44.1/48kHz 立体声 F32），
    /// 在回调里线性降采样到 16kHz mono PCM16。
    pub fn start(&self) -> Result<()> {
        let host = cpal::default_host();
        let device = if self.device_name.is_empty() {
            host.default_input_device()
                .ok_or_else(|| anyhow!("无可用默认录音设备"))?
        } else {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == self.device_name).unwrap_or(false))
                .unwrap_or_else(|| host.default_input_device().expect("no default input"))
        };
        let device_name = device.name().unwrap_or_default();

        let supported = device.default_input_config().context("get input config")?;
        let sample_format = supported.sample_format();
        let device_rate = supported.sample_rate().0;
        let device_channels = supported.channels();

        // 直接用设备 native config，避免 "stream configuration is not supported"
        let config: cpal::StreamConfig = supported.clone().into();

        tracing::info!(
            device = %device_name,
            format = ?sample_format,
            rate = device_rate,
            channels = device_channels,
            gain = self.gain,
            "录音设备",
        );

        let buffer = Arc::clone(&self.buffer);
        let stream_tx = Arc::clone(&self.stream_tx);
        let level = Arc::clone(&self.current_level);
        let gain = self.gain;

        let err_fn = |err| tracing::error!(?err, "cpal stream error");

        // 降采样累积器（device_rate → 16kHz 线性插值的简化版：等间隔取样）
        let stride = (device_rate as f64 / SAMPLE_RATE as f64).max(1.0);
        let state = Arc::new(Mutex::new(ResampleState::new()));

        let stream = match sample_format {
            SampleFormat::I16 => {
                let state = Arc::clone(&state);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mono = to_mono_f32_from_i16(data, device_channels);
                        push_resampled(&mono, stride, &state, gain, &buffer, &stream_tx, &level);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::F32 => {
                let state = Arc::clone(&state);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let mono = to_mono_f32_from_f32(data, device_channels);
                        push_resampled(&mono, stride, &state, gain, &buffer, &stream_tx, &level);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let state = Arc::clone(&state);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mono = to_mono_f32_from_u16(data, device_channels);
                        push_resampled(&mono, stride, &state, gain, &buffer, &stream_tx, &level);
                    },
                    err_fn,
                    None,
                )?
            }
            other => anyhow::bail!("不支持的采样格式: {:?}", other),
        };

        stream.play().context("start stream")?;
        *self.stream.lock() = Some(stream);
        Ok(())
    }

    /// 关闭流式 channel（录音仍继续，直到 stop）。
    pub fn stop_stream(&self) {
        *self.stream_tx.lock() = None;
    }

    /// 停止录音并返回累积的 PCM。
    pub fn stop(&self) -> Vec<u8> {
        if let Some(s) = self.stream.lock().take() {
            drop(s);
        }
        let mut guard = self.buffer.lock();
        std::mem::take(&mut *guard)
    }
}

/// 降采样状态：记录累积的"相位"，按 stride 间隔取样。
struct ResampleState {
    phase: f64,
}

impl ResampleState {
    fn new() -> Self {
        Self { phase: 0.0 }
    }
}

fn to_mono_f32_from_f32(data: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let ch = channels as usize;
    let n = data.len() / ch;
    let mut mono = Vec::with_capacity(n);
    for i in 0..n {
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += data[i * ch + c];
        }
        mono.push(sum / ch as f32);
    }
    mono
}

fn to_mono_f32_from_i16(data: &[i16], channels: u16) -> Vec<f32> {
    let mut floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
    if channels > 1 {
        floats = to_mono_f32_from_f32(&floats, channels);
    }
    floats
}

fn to_mono_f32_from_u16(data: &[u16], channels: u16) -> Vec<f32> {
    let mut floats: Vec<f32> = data
        .iter()
        .map(|&u| {
            let s: i16 = u.to_sample();
            s as f32 / 32768.0
        })
        .collect();
    if channels > 1 {
        floats = to_mono_f32_from_f32(&floats, channels);
    }
    floats
}

fn push_resampled(
    mono_in: &[f32],
    stride: f64,
    state: &Arc<Mutex<ResampleState>>,
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    // 等间隔抽样（最近邻）到 16kHz
    let mut out = Vec::<i16>::with_capacity((mono_in.len() as f64 / stride) as usize + 1);
    let mut st = state.lock();
    let mut sum_sq = 0.0f64;
    let mut n_out = 0usize;
    while st.phase < mono_in.len() as f64 {
        let idx = st.phase as usize;
        if idx >= mono_in.len() {
            break;
        }
        let v = (mono_in[idx].clamp(-1.0, 1.0) * 32767.0) as i32 * gain as i32;
        let s = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out.push(s);
        let f = s as f64 / 32768.0;
        sum_sq += f * f;
        n_out += 1;
        st.phase += stride;
    }
    st.phase -= mono_in.len() as f64;
    drop(st);

    if out.is_empty() {
        return;
    }
    let mut bytes = Vec::with_capacity(out.len() * 2);
    for s in &out {
        bytes.extend_from_slice(&s.to_le_bytes());
    }

    if n_out > 0 {
        let rms = (sum_sq / n_out as f64).sqrt();
        let lvl = rms.min(1.0) as f32;
        level.store(lvl.to_bits(), Ordering::Relaxed);
    }
    buffer.lock().extend_from_slice(&bytes);
    if let Some(tx) = stream_tx.lock().as_ref() {
        let _ = tx.try_send(bytes);
    }
}

/// 把累积的 PCM 静音裁剪后打包成 WAV（对应 Go 版 Recorder.ToWAV）。
/// 裁剪首尾低于阈值的样本，保底最少 0.5 秒。
pub fn to_wav(pcm: &[u8]) -> Vec<u8> {
    let trimmed = trim_silence(pcm);
    crate::asr::wav::build_wav(trimmed)
}

/// 去掉首尾静音，保底最少 0.5 秒。
/// 小端 PCM S16，无通道交织（我们只做单声道）。
fn trim_silence(pcm: &[u8]) -> &[u8] {
    const THRESHOLD: i16 = 30;
    const MIN_SAMPLES: usize = SAMPLE_RATE as usize / 2; // 0.5 秒

    if pcm.len() < 4 {
        return pcm;
    }
    let sample_count = pcm.len() / 2;

    let read = |i: usize| -> i16 { i16::from_le_bytes([pcm[i * 2], pcm[i * 2 + 1]]) };

    let mut start = 0;
    while start < sample_count && read(start).unsigned_abs() < THRESHOLD as u16 {
        start += 1;
    }
    if start >= sample_count {
        return pcm; // 全静音，别裁了
    }
    let mut end = sample_count;
    while end > start && read(end - 1).unsigned_abs() < THRESHOLD as u16 {
        end -= 1;
    }

    let trimmed_count = end - start;
    if trimmed_count < MIN_SAMPLES {
        return pcm; // 裁剪后太短，用原始数据
    }

    &pcm[start * 2..end * 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence_pcm(n: usize) -> Vec<u8> {
        vec![0u8; n * 2]
    }

    fn loud_pcm(n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n * 2);
        for _ in 0..n {
            v.extend_from_slice(&5000i16.to_le_bytes());
        }
        v
    }

    #[test]
    fn trim_all_silence_returns_original() {
        let pcm = silence_pcm(100);
        let out = trim_silence(&pcm);
        assert_eq!(out.len(), pcm.len());
    }

    #[test]
    fn trim_preserves_loud_middle() {
        // 1秒静音 + 1秒声音 + 1秒静音 = 3 秒
        let mut pcm = silence_pcm(16000);
        pcm.extend(loud_pcm(16000));
        pcm.extend(silence_pcm(16000));
        let out = trim_silence(&pcm);
        assert_eq!(out.len(), 16000 * 2); // 只剩中间 1 秒
    }

    #[test]
    fn trim_too_short_uses_original() {
        // 100 毫秒声音，低于 0.5 秒保底
        let pcm = loud_pcm(1600);
        let out = trim_silence(&pcm);
        assert_eq!(out.len(), pcm.len());
    }
}
