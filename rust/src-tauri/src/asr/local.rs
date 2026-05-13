//! 本地 SenseVoice ASR（离线）。
//! 对应 Go 版的 local_asr.go（+nosherpa 时是 stub）。
//!
//! 启用 feature `local-asr` 时，通过 `sherpa-onnx` crate（1.13）调用 SenseVoice 模型；
//! 默认不启用，与 Go CI `-tags nosherpa` 发布产物行为等价（避免 C 库编译开销）。

use crate::dirs::config_dir;
use anyhow::Result;
use std::path::PathBuf;

/// SenseVoice 模型目录（沿用 Go 版命名）
pub const MODEL_DIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

/// 模型下载 URL + SHA256
pub const MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";
pub const MODEL_SHA256: &str = "8148030f23c4bc0848239c80b635f3a0a1c275a2ae7ae37469bbe2341aa96d3f";

/// 模型安装根目录。
pub fn model_path() -> PathBuf {
    config_dir().join(MODEL_DIR)
}

/// 模型是否已下载。
pub fn is_available() -> bool {
    model_path().join("model.int8.onnx").is_file()
}

/// 主识别接口。
#[cfg(feature = "local-asr")]
pub async fn transcribe(cfg: &crate::config::Config, wav: &[u8]) -> Result<String> {
    use anyhow::anyhow;

    if !is_available() {
        anyhow::bail!("SenseVoice 模型未下载,请先在设置里下载");
    }

    let signature = build_signature(cfg);
    let hotwords_path = if cfg.hotwords.is_empty() {
        None
    } else {
        Some(write_hotwords_file(&cfg.hotwords)?)
    };
    let model_filename = if cfg.local_use_fp32_model {
        "model.onnx"
    } else {
        "model.int8.onnx"
    };
    let provider = if cfg.local_use_coreml {
        "coreml"
    } else {
        "cpu"
    };
    let model_file = model_path()
        .join(model_filename)
        .to_string_lossy()
        .into_owned();
    if !std::path::Path::new(&model_file).is_file() {
        anyhow::bail!("SenseVoice 模型文件 {} 不存在,请重下载模型", model_filename);
    }
    let tokens_file = model_path()
        .join("tokens.txt")
        .to_string_lossy()
        .into_owned();

    let wav_vec = wav.to_vec();

    // 在独立线程跑 native 推理,避免阻塞 tokio runtime。with_recognizer 闭包
    // 在 cache mutex 持有期间执行,这样不需要 clone OfflineRecognizer
    // (sherpa-onnx 没实现 Clone)。voice-claude 设计上同一时刻只有一次录音,
    // 不会发生需要并发解码的情况,串行 mutex 没有性能损失。
    tokio::task::spawn_blocking(move || -> Result<String> {
        with_recognizer(
            &signature,
            &model_file,
            &tokens_file,
            provider,
            hotwords_path.as_deref(),
            |recognizer| {
                let (samples, sample_rate) = wav_bytes_to_samples(&wav_vec)?;
                let stream = recognizer.create_stream();
                stream.accept_waveform(sample_rate, &samples);
                recognizer.decode(&stream);
                let result = stream.get_result().ok_or_else(|| anyhow!("识别结果为空"))?;
                Ok(result.text)
            },
        )
    })
    .await?
}

#[cfg(not(feature = "local-asr"))]
pub async fn transcribe(_cfg: &crate::config::Config, _wav: &[u8]) -> Result<String> {
    anyhow::bail!(
        "本地 SenseVoice 未启用。请用 `cargo build --features local-asr` 重新编译,或选择其他 ASR 后端"
    )
}

// ============================================================================
// Recognizer 缓存 —— OfflineRecognizer 是 Send + Sync,可以静态持有,
// 复用避免反复 228MB / 894MB 的 ONNX 加载。配置(模型精度 / provider / 热词)
// 变了用 signature 失效重建。
// ============================================================================

#[cfg(feature = "local-asr")]
fn build_signature(cfg: &crate::config::Config) -> String {
    use std::collections::BTreeMap;
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    cfg.local_use_fp32_model.hash(&mut h);
    cfg.local_use_coreml.hash(&mut h);
    let sorted: BTreeMap<&String, &String> = cfg.hotwords.iter().collect();
    for (k, v) in &sorted {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    format!("{:x}", h.finish())
}

#[cfg(feature = "local-asr")]
fn write_hotwords_file(
    hotwords: &std::collections::HashMap<String, String>,
) -> Result<std::path::PathBuf> {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<&str> = BTreeSet::new();
    for (k, v) in hotwords {
        let k = k.trim();
        let v = v.trim();
        if !k.is_empty() {
            set.insert(k);
        }
        if !v.is_empty() {
            set.insert(v);
        }
    }
    // sherpa-onnx hotwords_file 格式:每行一个词。SenseVoice 训练时用 char-level
    // tokenize,直接写中文/英文词都能被 internal tokenizer 处理。
    let content: String = set.into_iter().collect::<Vec<_>>().join("\n");
    let path = config_dir().join("local_hotwords.txt");
    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(feature = "local-asr")]
struct CachedRecognizer {
    signature: String,
    inner: sherpa_onnx::OfflineRecognizer,
}

#[cfg(feature = "local-asr")]
fn cache() -> &'static parking_lot::Mutex<Option<CachedRecognizer>> {
    use std::sync::OnceLock;
    static CELL: OnceLock<parking_lot::Mutex<Option<CachedRecognizer>>> = OnceLock::new();
    CELL.get_or_init(|| parking_lot::Mutex::new(None))
}

/// 在 mutex 守卫内执行 f(&recognizer)。配置变了重建并缓存。
#[cfg(feature = "local-asr")]
fn with_recognizer<F, R>(
    signature: &str,
    model_file: &str,
    tokens_file: &str,
    provider: &str,
    hotwords_path: Option<&std::path::Path>,
    f: F,
) -> Result<R>
where
    F: FnOnce(&sherpa_onnx::OfflineRecognizer) -> Result<R>,
{
    use anyhow::anyhow;
    use sherpa_onnx::OfflineRecognizerConfig;
    let mut guard = cache().lock();

    let needs_rebuild = match guard.as_ref() {
        Some(cached) if cached.signature == signature => false,
        Some(cached) => {
            tracing::info!(
                old = %cached.signature,
                new = %signature,
                "SenseVoice 配置变化,重建 recognizer"
            );
            true
        }
        None => {
            tracing::info!(
                provider,
                model = model_file,
                "首次构建 SenseVoice recognizer"
            );
            true
        }
    };

    if needs_rebuild {
        // drop 老的(释放 ONNX session 内存)再建新的,峰值内存不叠加
        *guard = None;
        let mut sconf = OfflineRecognizerConfig::default();
        sconf.model_config.sense_voice.model = Some(model_file.to_string());
        sconf.model_config.sense_voice.language = Some("zh".to_string());
        sconf.model_config.sense_voice.use_itn = true;
        sconf.model_config.tokens = Some(tokens_file.to_string());
        sconf.model_config.num_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8) as i32)
            .unwrap_or(2);
        sconf.model_config.provider = Some(provider.to_string());
        sconf.decoding_method = Some("greedy_search".to_string());
        if let Some(p) = hotwords_path {
            sconf.hotwords_file = Some(p.to_string_lossy().into_owned());
            sconf.hotwords_score = 1.5;
        }
        let recognizer = sherpa_onnx::OfflineRecognizer::create(&sconf)
            .ok_or_else(|| anyhow!("SenseVoice 初始化失败"))?;
        *guard = Some(CachedRecognizer {
            signature: signature.to_string(),
            inner: recognizer,
        });
    } else {
        tracing::debug!("SenseVoice recognizer 命中缓存");
    }

    let cached = guard.as_ref().expect("just inserted or kept");
    f(&cached.inner)
}

/// 下载模型。流式拉 + 字节级进度回调 (downloaded, total)。
/// 完整流程：GET stream → 累积到 Vec → 校验 SHA256 → 解压（install_from_bytes）。
pub async fn download_model<F: Fn(u64, u64) + Send + 'static>(on_progress: F) -> Result<()> {
    use anyhow::Context;

    let response = reqwest::get(MODEL_URL)
        .await
        .context("下载 SenseVoice 模型失败")?;
    let total = response.content_length().unwrap_or(0);

    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("读取下载流失败")?;
        buf.extend_from_slice(&chunk);
        on_progress(buf.len() as u64, total);
    }

    install_from_bytes(&buf)?;
    // 解压完成后把进度拉满（前端看到 100% 再退出下载中状态）
    let final_size = buf.len() as u64;
    on_progress(final_size, final_size.max(total));
    Ok(())
}

/// 从本地已下好的 tar.bz2 文件导入模型（国内下载失败的兜底路径）。
/// 和 download_model 共用校验 + 解压逻辑。
pub async fn import_tarball(path: PathBuf) -> Result<()> {
    use anyhow::Context;
    tokio::task::spawn_blocking(move || -> Result<()> {
        let bytes = std::fs::read(&path).with_context(|| format!("读取 {:?} 失败", path))?;
        install_from_bytes(&bytes)
    })
    .await?
}

/// 校验 SHA256 → 解压 tar.bz2 到临时目录 → 原子替换到最终目录。
fn install_from_bytes(bytes: &[u8]) -> Result<()> {
    use anyhow::{bail, Context};
    use bzip2::read::MultiBzDecoder;
    use sha2::{Digest, Sha256};
    use std::fs;
    use tar::Archive;

    let dest_dir = config_dir();
    fs::create_dir_all(&dest_dir).ok();

    let actual_sha = {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    };
    if actual_sha != MODEL_SHA256 {
        bail!(
            "SHA256 校验失败（期望 {}，实际 {}），文件可能损坏",
            MODEL_SHA256,
            actual_sha
        );
    }

    let tmp_dir = tempdir_in(&dest_dir)?;
    let cursor = std::io::Cursor::new(bytes);
    let bz = MultiBzDecoder::new(cursor);
    let mut archive = Archive::new(bz);
    for entry in archive.entries().context("读取 tar 失败")? {
        let mut entry = entry.context("读取 tar entry 失败")?;
        let path = entry.path().context("entry path")?;
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let target = tmp_dir.join(&stripped);
        let safe_root = fs::canonicalize(&tmp_dir).unwrap_or_else(|_| tmp_dir.clone());
        if let Ok(canonical) = fs::canonicalize(target.parent().unwrap_or(&tmp_dir)) {
            if !canonical.starts_with(&safe_root) {
                bail!("非法路径: {:?}", stripped);
            }
        }
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).ok();
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).ok();
            }
            let mut out =
                fs::File::create(&target).with_context(|| format!("写入 {:?}", target))?;
            std::io::copy(&mut entry, &mut out).with_context(|| format!("copy to {:?}", target))?;
        }
    }

    let final_dir = dest_dir.join(MODEL_DIR);
    fs::remove_dir_all(&final_dir).ok();
    fs::rename(&tmp_dir, &final_dir).context("移动模型目录失败")?;
    Ok(())
}

fn tempdir_in(parent: &std::path::Path) -> Result<PathBuf> {
    use std::fs;
    for _ in 0..10 {
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let p = parent.join(format!("sense-voice-tmp-{}", ts));
        if fs::create_dir(&p).is_ok() {
            return Ok(p);
        }
    }
    anyhow::bail!("无法创建临时目录")
}

/// 从 WAV 字节解析 float32 PCM。
#[allow(dead_code)]
fn wav_bytes_to_samples(wav: &[u8]) -> Result<(Vec<f32>, i32)> {
    if wav.len() < 44 {
        anyhow::bail!("WAV 数据过短");
    }
    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]) as i32;
    let pcm = &wav[44..];
    let n = pcm.len() / 2;
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let s = i16::from_le_bytes([pcm[i * 2], pcm[i * 2 + 1]]);
        samples.push(s as f32 / 32768.0);
    }
    Ok((samples, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_includes_dir_name() {
        assert!(model_path().to_string_lossy().contains(MODEL_DIR));
    }

    #[test]
    fn constants_match_go_version() {
        assert_eq!(MODEL_SHA256.len(), 64);
        assert!(MODEL_URL.starts_with("https://github.com/k2-fsa/sherpa-onnx/"));
    }

    #[test]
    fn parse_wav_roundtrip() {
        // 构造一个 16k 单声道 WAV
        let pcm = vec![0u8, 0u8, 0x00u8, 0x10u8]; // 两个 sample: 0 和 4096
        let wav = crate::asr::wav::build_wav(&pcm);
        let (samples, rate) = wav_bytes_to_samples(&wav).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(samples.len(), 2);
    }
}
