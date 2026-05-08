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
    pub fn start(&self) -> Result<()> {
        let host = cpal::default_host();
        let device = if self.device_name.is_empty() {
            host.default_input_device().ok_or_else(|| anyhow!("无可用默认录音设备"))?
        } else {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == self.device_name).unwrap_or(false))
                .unwrap_or_else(|| host.default_input_device().expect("no default input"))
        };
        let device_name = device.name().unwrap_or_default();

        let supported = device
            .default_input_config()
            .context("get input config")?;
        let sample_format = supported.sample_format();

        // 我们要求 16kHz mono s16，如果设备不支持，让 cpal 自动选择后续手动重采样过滤
        let config = cpal::StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        tracing::info!(device = %device_name, format = ?sample_format, gain = self.gain, "录音设备");

        let buffer = Arc::clone(&self.buffer);
        let stream_tx = Arc::clone(&self.stream_tx);
        let level = Arc::clone(&self.current_level);
        let gain = self.gain;

        let err_fn = |err| tracing::error!(?err, "cpal stream error");

        let stream = match sample_format {
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| handle_input_i16(data, gain, &buffer, &stream_tx, &level),
                err_fn,
                None,
            )?,
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| handle_input_f32(data, gain, &buffer, &stream_tx, &level),
                err_fn,
                None,
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| handle_input_u16(data, gain, &buffer, &stream_tx, &level),
                err_fn,
                None,
            )?,
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

fn handle_input_i16(
    data: &[i16],
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    let mut bytes = Vec::with_capacity(data.len() * 2);
    let mut sum_sq = 0.0f64;
    for &s in data {
        let v = (s as i32 * gain as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
        let f = s as f64 / 32768.0;
        sum_sq += f * f;
    }
    push_common(bytes, data.len(), sum_sq, gain, buffer, stream_tx, level);
}

fn handle_input_f32(
    data: &[f32],
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    let mut bytes = Vec::with_capacity(data.len() * 2);
    let mut sum_sq = 0.0f64;
    for &f in data {
        let clamped = f.clamp(-1.0, 1.0);
        let s = (clamped * 32767.0) as i32 * gain as i32;
        let s = s.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        bytes.extend_from_slice(&s.to_le_bytes());
        sum_sq += (clamped as f64) * (clamped as f64);
    }
    push_common(bytes, data.len(), sum_sq, gain, buffer, stream_tx, level);
}

fn handle_input_u16(
    data: &[u16],
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    let mut bytes = Vec::with_capacity(data.len() * 2);
    let mut sum_sq = 0.0f64;
    for &u in data {
        // 用 cpal::Sample 的 to_sample::<i16>() 转换
        let s: i16 = u.to_sample();
        let v = (s as i32 * gain as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
        let f = s as f64 / 32768.0;
        sum_sq += f * f;
    }
    push_common(bytes, data.len(), sum_sq, gain, buffer, stream_tx, level);
}

fn push_common(
    bytes: Vec<u8>,
    sample_count: usize,
    sum_sq: f64,
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    if sample_count > 0 {
        let rms = (sum_sq / sample_count as f64).sqrt() * gain as f64;
        let lvl = rms.min(1.0) as f32;
        level.store(lvl.to_bits(), Ordering::Relaxed);
    }
    buffer.lock().extend_from_slice(&bytes);
    if let Some(tx) = stream_tx.lock().as_ref() {
        let _ = tx.try_send(bytes);
    }
}

/// 把累积的 PCM 加 WAV 头（对应 Go 版的 Recorder.ToWAV）。
pub fn to_wav(pcm: &[u8]) -> Vec<u8> {
    crate::asr::wav::build_wav(pcm)
}
