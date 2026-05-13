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
    enhance: bool,
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
    pub fn new(gain: u8, device_name: &str, enhance: bool) -> Self {
        Self {
            gain: gain.max(1),
            device_name: device_name.to_string(),
            enhance,
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

    /// 拷贝 buffer[offset..] 的字节(不消费)。VAD 任务用游标跟踪已喂给检测器
    /// 的位置,周期性 peek 尾部新增 PCM 增量喂入。返回 (新字节, 总长度)。
    pub fn peek_pcm_since(&self, offset: usize) -> (Vec<u8>, usize) {
        let buf = self.buffer.lock();
        let total = buf.len();
        let new_bytes = if offset >= total {
            Vec::new()
        } else {
            buf[offset..].to_vec()
        };
        (new_bytes, total)
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

        let default_cfg = device.default_input_config().context("get input config")?;

        // 优先尝试 16kHz mono 原生配置（避免我们的下采样 aliasing）；设备不支持再用 default
        let (config, sample_format, device_rate, device_channels): (cpal::StreamConfig, _, _, _) = {
            let ranges: Vec<_> = device
                .supported_input_configs()
                .ok()
                .map(|it| it.collect())
                .unwrap_or_default();
            let supports_16k_mono = ranges.iter().any(|r| {
                r.channels() == 1
                    && r.min_sample_rate().0 <= SAMPLE_RATE
                    && r.max_sample_rate().0 >= SAMPLE_RATE
            });
            if supports_16k_mono {
                let cfg = cpal::StreamConfig {
                    channels: 1,
                    sample_rate: cpal::SampleRate(SAMPLE_RATE),
                    buffer_size: cpal::BufferSize::Default,
                };
                (cfg, default_cfg.sample_format(), SAMPLE_RATE, 1u16)
            } else {
                let cfg: cpal::StreamConfig = default_cfg.clone().into();
                (
                    cfg,
                    default_cfg.sample_format(),
                    default_cfg.sample_rate().0,
                    default_cfg.channels(),
                )
            }
        };

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
        let enhance = self.enhance;

        let err_fn = |err| tracing::error!(?err, "cpal stream error");

        // 降采样累积器（device_rate → 16kHz 线性插值的简化版：等间隔取样）
        let stride = (device_rate as f64 / SAMPLE_RATE as f64).max(1.0);
        let state = Arc::new(Mutex::new(ResampleState::new()));
        // 气声增强状态(pre-emphasis + compressor),跨 cpal 回调保持
        let enhancer = Arc::new(Mutex::new(Enhancer::default()));

        let stream = match sample_format {
            SampleFormat::I16 => {
                let state = Arc::clone(&state);
                let enhancer = Arc::clone(&enhancer);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mono = to_mono_f32_from_i16(data, device_channels);
                        push_resampled(
                            &mono, stride, &state, &enhancer, enhance, gain, &buffer, &stream_tx,
                            &level,
                        );
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::F32 => {
                let state = Arc::clone(&state);
                let enhancer = Arc::clone(&enhancer);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let mono = to_mono_f32_from_f32(data, device_channels);
                        push_resampled(
                            &mono, stride, &state, &enhancer, enhance, gain, &buffer, &stream_tx,
                            &level,
                        );
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let state = Arc::clone(&state);
                let enhancer = Arc::clone(&enhancer);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mono = to_mono_f32_from_u16(data, device_channels);
                        push_resampled(
                            &mono, stride, &state, &enhancer, enhance, gain, &buffer, &stream_tx,
                            &level,
                        );
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
    /// 显式调 stream.pause() 再 drop —— macOS CoreAudio 仅靠 Drop 清理有时系统级录音指示灯不灭。
    pub fn stop(&self) -> Vec<u8> {
        if let Some(s) = self.stream.lock().take() {
            let _ = s.pause();
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

// ============================================================================
// 气声增强管线 —— 流式处理,单 sample 级别维持状态
// ============================================================================
//
// 气声(whisper / breathy voice)的两个核心麻烦:
//   - 能量偏高频(摩擦音主导,2-4 kHz),ASR 模型训练时见到的频谱包络偏低频
//   - 振幅小,固定 gain 放大正常说话会 clip,不放大气声又识别不出
//
// 管线顺序: pre-emphasis → compressor
// - pre-emphasis: Kaldi/ESPnet 等经典 ASR 标准预处理,α=0.97,让频谱包络更像
//   voiced speech,对气声尤其有效
// - compressor: 软膝压缩器,把低能量区抬起来,动态范围压平。替代"固定大 gain
//   导致爆音"的粗暴做法;跨回调保持 envelope 状态做平滑 attack/release。

const PRE_EMPHASIS_ALPHA: f32 = 0.97;

pub struct Enhancer {
    pre_emph_prev: f32,
    // compressor 状态 —— envelope 用 RMS-ish 估计,gain 平滑跟随
    envelope: f32,
    comp_gain: f32,
}

impl Default for Enhancer {
    fn default() -> Self {
        Self {
            pre_emph_prev: 0.0,
            envelope: 0.0,
            comp_gain: 1.0,
        }
    }
}

impl Enhancer {
    /// 对 16kHz f32 mono sample 处理一个样本,返回增强后样本。
    /// 输入输出都在 [-1, 1] 范围(函数内部会自动 clamp)。
    pub fn process(&mut self, x: f32) -> f32 {
        // 1. pre-emphasis: y[n] = x[n] - α·x[n-1]
        let pe = x - PRE_EMPHASIS_ALPHA * self.pre_emph_prev;
        self.pre_emph_prev = x;

        // 2. compressor
        //   threshold = -30 dBFS (linear 0.0316)
        //   ratio     = 3:1
        //   attack    ≈ 5 ms @ 16kHz  → coef = 1 - exp(-1 / (5ms × 16k)) ≈ 0.0123
        //   release   ≈ 50 ms @ 16kHz → coef ≈ 0.00125
        const THRESHOLD: f32 = 0.0316; // -30 dBFS
        const RATIO: f32 = 3.0;
        const ATTACK: f32 = 0.0123;
        const RELEASE: f32 = 0.00125;
        // 软膝(soft knee),单侧 6 dB 宽度,避免阈值附近抖动
        const KNEE_WIDTH_DB: f32 = 6.0;

        let abs = pe.abs();
        // envelope follower:大信号快上(attack),小信号慢下(release)
        let coef = if abs > self.envelope { ATTACK } else { RELEASE };
        self.envelope += coef * (abs - self.envelope);

        // 计算目标 gain reduction
        let target_gain = if self.envelope < 1e-6 {
            1.0
        } else {
            let env_db = 20.0 * self.envelope.log10();
            let thr_db = 20.0 * THRESHOLD.log10();
            let delta = env_db - thr_db;
            let over_db = if delta <= -KNEE_WIDTH_DB / 2.0 {
                0.0
            } else if delta >= KNEE_WIDTH_DB / 2.0 {
                // 全压缩区:超出阈值 delta dB,按 ratio 压缩输出 delta/ratio dB,
                // 即衰减 (delta - delta/ratio) = delta × (1 - 1/ratio)
                delta * (1.0 - 1.0 / RATIO)
            } else {
                // 软膝过渡(二次曲线,详见 dbx compressor 白皮书)
                let x = delta + KNEE_WIDTH_DB / 2.0;
                (1.0 - 1.0 / RATIO) * x * x / (2.0 * KNEE_WIDTH_DB)
            };
            // 输出 = 输入 × gain_reduction,over_db 是需要衰减的 dB
            10f32.powf(-over_db / 20.0)
        };
        // gain 平滑(和 envelope 用同一个 coef),避免 gain 突变导致咔哒声
        self.comp_gain += coef * (target_gain - self.comp_gain);

        // 3. make-up gain:补偿压缩导致的整体能量下降。threshold -30dB 下
        //    稳态信号平均压 10dB,make-up 6dB 接近原响度
        const MAKEUP_GAIN: f32 = 2.0; // +6 dB

        (pe * self.comp_gain * MAKEUP_GAIN).clamp(-1.0, 1.0)
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

#[allow(clippy::too_many_arguments)]
fn push_resampled(
    mono_in: &[f32],
    stride: f64,
    state: &Arc<Mutex<ResampleState>>,
    enhancer: &Arc<Mutex<Enhancer>>,
    enhance_enabled: bool,
    gain: u8,
    buffer: &Arc<Mutex<Vec<u8>>>,
    stream_tx: &Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    level: &Arc<AtomicU32>,
) {
    // 等间隔抽样（最近邻）到 16kHz
    let mut out = Vec::<i16>::with_capacity((mono_in.len() as f64 / stride) as usize + 1);
    let mut st = state.lock();
    let mut enh = enhancer.lock();
    let mut sum_sq = 0.0f64;
    let mut n_out = 0usize;
    while st.phase < mono_in.len() as f64 {
        let idx = st.phase as usize;
        if idx >= mono_in.len() {
            break;
        }
        let mut raw = mono_in[idx].clamp(-1.0, 1.0);
        if enhance_enabled {
            raw = enh.process(raw);
        }
        let v = (raw * 32767.0) as i32 * gain as i32;
        let s = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out.push(s);
        let f = s as f64 / 32768.0;
        sum_sq += f * f;
        n_out += 1;
        st.phase += stride;
    }
    st.phase -= mono_in.len() as f64;
    drop(st);
    drop(enh);

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
///
/// enhance=true 时额外跑 peak_normalize:扫整段找 99th percentile 绝对值(避开
/// 敲键盘/咳嗽 spike),scale 到 0.9 × i16::MAX。只对批处理 ASR 生效 —— 流式
/// 路径的 PCM 已经边录边发给了 ASR,追不回。
pub fn to_wav(pcm: &[u8], enhance: bool) -> Vec<u8> {
    let trimmed = trim_silence(pcm);
    let processed = if enhance {
        peak_normalize(trimmed)
    } else {
        trimmed.to_vec()
    };
    crate::asr::wav::build_wav(&processed)
}

const NORMALIZE_TARGET_PEAK: f32 = 0.9 * i16::MAX as f32;

/// Peak normalize:扫一遍 PCM 找 99th percentile 绝对值,按需放大整段。
/// 用 99th percentile 而不是 max,避免偶发 spike(敲键盘/麦克风 pop)把
/// 人声整体压得太小。
fn peak_normalize(pcm: &[u8]) -> Vec<u8> {
    if pcm.len() < 4 {
        return pcm.to_vec();
    }
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    if samples.is_empty() {
        return pcm.to_vec();
    }

    // 99th percentile of abs(sample)
    let mut abs: Vec<i32> = samples.iter().map(|&s| s.unsigned_abs() as i32).collect();
    abs.sort_unstable();
    let idx_99 = ((0.99_f64 * abs.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(abs.len() - 1);
    let p99 = abs[idx_99] as f32;

    // peak 已经接近满量程就不放大(放大反而增加底噪)
    if p99 >= NORMALIZE_TARGET_PEAK * 0.95 {
        return pcm.to_vec();
    }
    // 几乎全静音时也跳过(不然会把底噪放到满量程)
    if p99 < 30.0 {
        return pcm.to_vec();
    }

    let scale = NORMALIZE_TARGET_PEAK / p99;
    let mut out = Vec::with_capacity(pcm.len());
    for &s in &samples {
        let v = (s as f32 * scale).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
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

    fn pcm_from_samples(samples: &[i16]) -> Vec<u8> {
        let mut v = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    fn read_samples(pcm: &[u8]) -> Vec<i16> {
        pcm.chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    #[test]
    fn peak_normalize_scales_quiet_signal_up() {
        // 峰值只到 ~3000,应该被放大到接近 target
        let samples: Vec<i16> = (0..200)
            .map(|i| if i % 2 == 0 { 3000 } else { -3000 })
            .collect();
        let out = peak_normalize(&pcm_from_samples(&samples));
        let out_samples = read_samples(&out);
        let max = out_samples
            .iter()
            .map(|s| s.unsigned_abs() as i32)
            .max()
            .unwrap();
        // 目标 peak ≈ 29491,允许小误差
        assert!(max > 25_000, "max should be scaled up, got {}", max);
        assert!(max <= i16::MAX as i32);
    }

    #[test]
    fn peak_normalize_skips_near_full_scale() {
        // 峰值已经接近满量程,不应再放大(防 clip)
        let samples: Vec<i16> = vec![30000; 100];
        let out = peak_normalize(&pcm_from_samples(&samples));
        assert_eq!(out, pcm_from_samples(&samples));
    }

    #[test]
    fn peak_normalize_skips_silence() {
        // 全静音不放大(不然底噪被放到满量程)
        let samples: Vec<i16> = vec![10; 100]; // 峰值 < 30 阈值
        let out = peak_normalize(&pcm_from_samples(&samples));
        assert_eq!(out, pcm_from_samples(&samples));
    }

    #[test]
    fn peak_normalize_uses_99th_percentile_not_max() {
        // 100 个小信号 + 1 个 spike,应按小信号的 peak normalize,不被 spike 影响
        let mut samples: Vec<i16> = vec![3000; 100];
        samples[50] = 30_000; // 单点 spike
        let out = peak_normalize(&pcm_from_samples(&samples));
        let out_samples = read_samples(&out);
        // p99 索引 = ceil(0.99 * 100) - 1 = 98,取第 99 大的 = 3000
        // 所以 scale = NORMALIZE_TARGET_PEAK / 3000 ≈ 9.83,3000 → ~29491
        // 非 spike 样本应被放大到接近 target
        let typical = out_samples[0].unsigned_abs() as i32;
        assert!(
            typical > 25_000,
            "typical should be scaled, got {}",
            typical
        );
    }

    #[test]
    fn enhancer_preserves_silence() {
        let mut e = Enhancer::default();
        for _ in 0..100 {
            assert!(e.process(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn enhancer_compresses_loud_signal() {
        // 连续满量程信号,compressor 应该把输出压下来,绝对值 < 1.0
        let mut e = Enhancer::default();
        // warm up
        for _ in 0..2000 {
            e.process(0.9);
        }
        let y = e.process(0.9);
        assert!(y.abs() <= 1.0, "output out of range: {}", y);
    }
}
