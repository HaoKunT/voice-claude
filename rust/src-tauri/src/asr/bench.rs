//! 横向 ASR 测试:传文件 + 多选后端 → 跑识别 → 表格对比。
//!
//! - 解码 wav/mp3/m4a/aac → 16kHz mono PCM16 WAV (内部统一格式,所有后端吃这个)
//! - 云端后端(zhipu/xfyun/volc/openrouter)tokio::spawn 并行
//! - 本地后端(sense_voice/fire_red_aed/fire_red_ctc2/qwen3_asr)共享 OfflineRecognizer
//!   mutex,必须串行
//! - 每个后端跑完 emit "bench-result" 事件,前端表格逐行填
//! - 不入 history,纯 one-shot

use crate::config::Config;
use anyhow::{anyhow, bail, Result};
use std::path::Path;
use std::sync::Arc;

const CLOUD_PROVIDERS: &[&str] = &["zhipu", "xfyun", "volc", "openrouter"];
const LOCAL_ENGINES: &[&str] = &["sense_voice", "fire_red_aed", "fire_red_ctc2", "qwen3_asr"];

pub fn is_cloud_provider(id: &str) -> bool {
    CLOUD_PROVIDERS.contains(&id)
}

pub fn is_local_engine(id: &str) -> bool {
    LOCAL_ENGINES.contains(&id)
}

/// 解码任意 symphonia 支持的文件(wav/mp3/m4a/aac)→ 16kHz mono PCM16 WAV bytes。
/// 内部:解码 → downmix(stereo→mono 取均值) → 线性插值 resample → f32→i16 → wav header。
pub fn decode_to_pcm16k_mono_wav(path: &Path) -> Result<Vec<u8>> {
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("文件不含音频轨道"))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    // 容器层有时不声明 sample_rate / channels(常见于 m4a / aac),等首个
    // decoded buffer 拿到 SignalSpec 再确定 —— 容器层仅作为 hint。
    let mut source_rate: u32 = codec_params.sample_rate.unwrap_or(0);
    let mut channels: usize = codec_params.channels.map(|c| c.count()).unwrap_or(0);

    let mut decoder =
        symphonia::default::get_codecs().make(&codec_params, &DecoderOptions::default())?;

    // 解码累积 f32 mono samples
    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // 跳过坏包
            Err(e) => return Err(e.into()),
        };
        // 第一次拿到 buffer 才知道真实 spec(m4a 容器经常不声明)
        let (rate, ch) = buffer_spec(&decoded);
        if source_rate == 0 {
            source_rate = rate;
        }
        if channels == 0 {
            channels = ch;
        }
        append_mono_f32(decoded, channels, &mut samples)?;
    }

    if samples.is_empty() {
        bail!("解码未得到任何音频样本");
    }
    if source_rate == 0 {
        bail!("无法确定 sample rate");
    }

    // resample 到 16kHz(用 rubato 的 cubic polynomial,带 anti-alias)
    let resampled = if source_rate != 16000 {
        resample_to_16k(&samples, source_rate)?
    } else {
        samples
    };

    // f32 → i16 little-endian PCM bytes
    let mut pcm = Vec::with_capacity(resampled.len() * 2);
    for s in resampled {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    Ok(crate::asr::wav::build_wav(&pcm))
}

/// 从 AudioBufferRef 拿 (rate, channels)。symphonia 的 SignalSpec 在所有
/// AudioBuffer<T> 上一致,这里 match enum 取 inner buffer 的 spec —— m4a
/// / aac 容器层经常不声明,只能等 decoder 第一帧后从这里读。
fn buffer_spec(buf: &symphonia::core::audio::AudioBufferRef<'_>) -> (u32, usize) {
    use symphonia::core::audio::AudioBufferRef;
    macro_rules! pick {
        ($b:ident) => {{
            let s = $b.spec();
            (s.rate, s.channels.count())
        }};
    }
    match buf {
        AudioBufferRef::U8(b) => pick!(b),
        AudioBufferRef::U16(b) => pick!(b),
        AudioBufferRef::U24(b) => pick!(b),
        AudioBufferRef::U32(b) => pick!(b),
        AudioBufferRef::S8(b) => pick!(b),
        AudioBufferRef::S16(b) => pick!(b),
        AudioBufferRef::S24(b) => pick!(b),
        AudioBufferRef::S32(b) => pick!(b),
        AudioBufferRef::F32(b) => pick!(b),
        AudioBufferRef::F64(b) => pick!(b),
    }
}

/// 把任意 sample 格式的 AudioBufferRef 转成 mono f32,append 到 out。
/// stereo 取左右均值;多声道(>2)只取首声道(简化处理)。
fn append_mono_f32(
    buf: symphonia::core::audio::AudioBufferRef<'_>,
    channels: usize,
    out: &mut Vec<f32>,
) -> Result<()> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    macro_rules! handle {
        ($buf:ident, $to_f32:expr) => {{
            let frames = $buf.frames();
            if channels == 1 {
                let ch0 = $buf.chan(0);
                out.extend(ch0.iter().take(frames).map(|&s| ($to_f32)(s)));
            } else if channels >= 2 {
                let l = $buf.chan(0);
                let r = $buf.chan(1);
                for i in 0..frames {
                    out.push((($to_f32)(l[i]) + ($to_f32)(r[i])) * 0.5);
                }
            }
        }};
    }
    match buf {
        AudioBufferRef::U8(b) => handle!(b, |s: u8| (s as f32 - 128.0) / 128.0),
        AudioBufferRef::U16(b) => handle!(b, |s: u16| (s as f32 - 32768.0) / 32768.0),
        AudioBufferRef::U24(b) => handle!(b, |s: symphonia::core::sample::u24| {
            let v: u32 = s.0;
            ((v as f32) - 8_388_608.0) / 8_388_608.0
        }),
        AudioBufferRef::U32(b) => handle!(b, |s: u32| {
            (s as f32 - 2_147_483_648.0) / 2_147_483_648.0
        }),
        AudioBufferRef::S8(b) => handle!(b, |s: i8| s as f32 / 128.0),
        AudioBufferRef::S16(b) => handle!(b, |s: i16| s as f32 / 32768.0),
        AudioBufferRef::S24(b) => handle!(b, |s: symphonia::core::sample::i24| {
            s.inner() as f32 / 8_388_608.0
        }),
        AudioBufferRef::S32(b) => handle!(b, |s: i32| s as f32 / 2_147_483_648.0),
        AudioBufferRef::F32(b) => handle!(b, |s: f32| s),
        AudioBufferRef::F64(b) => handle!(b, |s: f64| s as f32),
    }
    Ok(())
}

/// 用 rubato 把 mono samples resample 到 16kHz。Polynomial cubic 比线性插值
/// 高一档质量,对 ASR 足够;sinc 算法精度最高但 CPU 开销大,bench 场景
/// 不必要。chunk_size 取 1024 平衡延迟和分配次数。
fn resample_to_16k(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    use rubato::{FastFixedIn, PolynomialDegree, Resampler};

    if samples.is_empty() {
        return Ok(Vec::new());
    }
    const CHUNK: usize = 1024;
    let ratio = 16_000.0 / from_rate as f64;
    let mut resampler = FastFixedIn::<f32>::new(ratio, 1.0, PolynomialDegree::Cubic, CHUNK, 1)
        .map_err(|e| anyhow!("rubato init: {}", e))?;

    let input = vec![samples.to_vec()];
    let mut out: Vec<f32> = Vec::with_capacity((samples.len() as f64 * ratio) as usize);
    let mut consumed = 0usize;
    let total = samples.len();
    let mut output_buf = resampler.output_buffer_allocate(true);

    // 分 chunk 跑 —— 最后一块不足 CHUNK 时用 process_partial 收尾
    while consumed + CHUNK <= total {
        let chunk = &input[0][consumed..consumed + CHUNK];
        let in_slice = &[chunk];
        let (in_used, out_filled) = resampler
            .process_into_buffer(in_slice, &mut output_buf, None)
            .map_err(|e| anyhow!("rubato process: {}", e))?;
        out.extend_from_slice(&output_buf[0][..out_filled]);
        consumed += in_used;
    }
    // 尾巴
    if consumed < total {
        let tail: Vec<f32> = input[0][consumed..].to_vec();
        let (_, out_filled) = resampler
            .process_partial_into_buffer(Some(&[tail.as_slice()]), &mut output_buf, None)
            .map_err(|e| anyhow!("rubato tail: {}", e))?;
        out.extend_from_slice(&output_buf[0][..out_filled]);
    }
    let _ = input; // explicit drop of input alloc
    Ok(out)
}

/// 用一个临时 Config 把 asr_provider / local_engine 改成目标 id,然后调原有
/// transcribe 链路。这样 bench 不需要重新实现 8 个后端,共享生产代码。
pub async fn run_one(provider_id: &str, base_cfg: &Config, wav: &[u8]) -> Result<String> {
    let mut tmp = base_cfg.clone();
    if is_local_engine(provider_id) {
        tmp.asr_provider = "local".to_string();
        tmp.local_engine = provider_id.to_string();
    } else if is_cloud_provider(provider_id) {
        tmp.asr_provider = provider_id.to_string();
    } else {
        bail!("未知 provider: {}", provider_id);
    }

    if crate::asr::is_streaming(&tmp.asr_provider) {
        bench_stream(&tmp, wav).await
    } else {
        crate::asr::transcribe_batch(&tmp, wav).await
    }
}

/// 流式后端的 wrapper:把整段 WAV 拆成 ~320ms 的 PCM chunks 喂给 transcribe_stream,
/// drop tx 触发 server 出 final。bench 一次性丢音频,无 partial 实时回调。
async fn bench_stream(cfg: &Config, wav: &[u8]) -> Result<String> {
    use tokio::sync::{mpsc, oneshot};

    // 320ms @ 16kHz 16bit mono = 320*16*2 = 10240 bytes/chunk
    const CHUNK_BYTES: usize = 10240;
    const PACE_MS: u64 = 20; // chunk 之间小 sleep,避免某些云端节流

    let (tx, rx) = mpsc::channel::<Vec<u8>>(16);
    let (ready_tx, ready_rx) = oneshot::channel::<()>();
    let cfg_clone = cfg.clone();
    let task = tokio::spawn(async move {
        crate::asr::transcribe_stream(&cfg_clone, rx, Box::new(|_| {}), ready_tx).await
    });

    // 等 WebSocket 握手就绪
    if ready_rx.await.is_err() {
        // ready 没发就直接挂了 —— 拿任务结果当错误返回
        return task
            .await
            .map_err(|e| anyhow!("{}", e))?
            .map_err(|e| anyhow!("{}", e));
    }

    let pcm = if wav.len() >= 44 { &wav[44..] } else { wav };
    for chunk in pcm.chunks(CHUNK_BYTES) {
        if tx.send(chunk.to_vec()).await.is_err() {
            // ASR task 已自行结束(可能出错),跳出
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(PACE_MS)).await;
    }
    drop(tx);

    task.await
        .map_err(|e| anyhow!("{}", e))?
        .map_err(|e| anyhow!("{}", e))
}

/// 把 provider_ids 拆成两组 —— 云端可并行,本地必须串行(共享 OfflineRecognizer mutex)。
pub fn split_cloud_local(ids: &[String]) -> (Vec<String>, Vec<String>) {
    let mut cloud = Vec::new();
    let mut local = Vec::new();
    for id in ids {
        if is_cloud_provider(id) {
            cloud.push(id.clone());
        } else if is_local_engine(id) {
            local.push(id.clone());
        }
    }
    (cloud, local)
}

/// 单个后端跑完的结果(给前端 emit)。
#[derive(Clone, serde::Serialize)]
pub struct BenchResult {
    pub provider_id: String,
    pub text: String,
    pub error: Option<String>,
    pub ms: i64,
}

/// 共享 wav 数据 + 跑一个 provider + emit 结果。供 command 内 spawn 调。
///
/// 本地 engine 先调 warm_up 把模型加载到 cache,然后才开始计时跑推理 ——
/// 否则冷启动那 3-5s 模型加载会被算进 ms 里,bench 测的就是"加载+推理"
/// 而不是"模型已就绪状态下的推理速度"。云端 provider 没有这个问题
/// (HTTPS 握手 100-200ms 算进去也算用户真实感知)。
pub async fn run_one_and_emit(
    provider_id: String,
    cfg: Arc<Config>,
    wav: Arc<Vec<u8>>,
    app: tauri::AppHandle,
) {
    use tauri::Emitter;
    let _ = app.emit("bench-started", &provider_id);

    if is_local_engine(&provider_id) {
        // 构造临时 cfg(asr_provider="local" + local_engine=本次目标),warm_up 读这俩字段
        let mut tmp = (*cfg).clone();
        tmp.asr_provider = "local".to_string();
        tmp.local_engine = provider_id.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = crate::asr::local::warm_up(&tmp) {
                tracing::debug!(error = ?e, "bench warm_up 失败,继续(后续 transcribe 会再尝试)");
            }
        })
        .await;
    }

    let started = std::time::Instant::now();
    let result = run_one(&provider_id, &cfg, &wav).await;
    let ms = started.elapsed().as_millis() as i64;
    let payload = match result {
        Ok(text) => BenchResult {
            provider_id,
            text,
            error: None,
            ms,
        },
        Err(e) => BenchResult {
            provider_id,
            text: String::new(),
            error: Some(e.to_string()),
            ms,
        },
    };
    let _ = app.emit("bench-result", payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_categorizes() {
        let ids = vec![
            "zhipu".to_string(),
            "sense_voice".to_string(),
            "openrouter".to_string(),
            "fire_red_aed".to_string(),
            "unknown".to_string(),
        ];
        let (cloud, local) = split_cloud_local(&ids);
        assert_eq!(cloud, vec!["zhipu", "openrouter"]);
        assert_eq!(local, vec!["sense_voice", "fire_red_aed"]);
    }

    #[test]
    fn resample_downsamples_44k_to_16k() {
        // 1 秒 44.1kHz 静音 → 16kHz 后约 16000 sample(±少许 polynomial 边界损失)
        let s = vec![0.0f32; 44_100];
        let out = resample_to_16k(&s, 44_100).unwrap();
        let expected = 16_000;
        let diff = (out.len() as i64 - expected as i64).unsigned_abs();
        // rubato cubic polynomial 的 chunk 边界处理会比简单插值多/少几百 sample,
        // 放宽到 500 容差(0.5%)足够覆盖,质量层面无影响
        assert!(diff < 500, "len={} expected≈{}", out.len(), expected);
    }

    #[test]
    fn resample_empty_returns_empty() {
        let out = resample_to_16k(&[], 44_100).unwrap();
        assert!(out.is_empty());
    }
}
