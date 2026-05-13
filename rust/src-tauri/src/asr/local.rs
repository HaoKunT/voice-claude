//! 本地 ASR 多引擎(离线)。
//!
//! 启用 feature `local-asr` 时,通过 `sherpa-onnx` crate 调用本地模型。
//! 默认 ON。
//!
//! 支持 4 个引擎,用户可在设置页切换:
//!   - SenseVoice    226MB  多语言 NAR,默认快档
//!   - FireRed-AED   1.4GB  中文 SOTA(WenetSpeech 4.76)
//!   - FireRed-CTC2  496MB  FireRedASR2 CTC 单文件,中等精度
//!   - Qwen3-ASR     837MB  LLM-based 自回归,首字延迟更慢
//!
//! 所有引擎共用同一套下载 / 校验 / 解压 / recognizer 缓存逻辑;
//! 区别在 `LocalEngine::build_model_config()` 里给 OfflineModelConfig 的
//! 不同子字段赋值。

use crate::dirs::config_dir;
use anyhow::Result;
use std::path::PathBuf;

// ============================================================================
// LocalEngine 枚举 + 元数据
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalEngine {
    SenseVoice,
    FireRedAed,
    FireRedCtc2,
    Qwen3Asr,
}

const RELEASE_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

impl LocalEngine {
    /// 全部引擎列表(给前端 dropdown 用)。
    pub const ALL: &'static [Self] = &[
        Self::SenseVoice,
        Self::FireRedAed,
        Self::FireRedCtc2,
        Self::Qwen3Asr,
    ];

    /// 从 config 字符串 id 解析,未知/空字符串回退到 SenseVoice。
    pub fn from_id(id: &str) -> Self {
        match id {
            "fire_red_aed" => Self::FireRedAed,
            "fire_red_ctc2" => Self::FireRedCtc2,
            "qwen3_asr" => Self::Qwen3Asr,
            _ => Self::SenseVoice,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::SenseVoice => "sense_voice",
            Self::FireRedAed => "fire_red_aed",
            Self::FireRedCtc2 => "fire_red_ctc2",
            Self::Qwen3Asr => "qwen3_asr",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::SenseVoice => "SenseVoice(默认 / 多语言)",
            Self::FireRedAed => "FireRedASR-AED-L(中文 SOTA)",
            Self::FireRedCtc2 => "FireRedASR2-CTC(轻量 CTC)",
            Self::Qwen3Asr => "Qwen3-ASR-0.6B(LLM-ASR)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::SenseVoice => "226MB,中英日韩粤多语言 NAR 模型,推理快,日常默认推荐",
            Self::FireRedAed => "1.4GB,中文 SOTA(AISHELL 0.55 / WenetSpeech 4.76),encoder-decoder 不流式但 NAR 推理快",
            Self::FireRedCtc2 => "496MB,FireRedASR2 CTC 单文件,精度低于 AED 但模型小",
            Self::Qwen3Asr => "837MB,Qwen3 LLM 改的自回归 ASR,首字延迟比 NAR 慢一档",
        }
    }

    /// 模型解压后的目录名(也是 release artifact 名去掉扩展名)。
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::SenseVoice => "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
            Self::FireRedAed => "sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16",
            Self::FireRedCtc2 => "sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25",
            Self::Qwen3Asr => "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25",
        }
    }

    pub fn model_url(&self) -> String {
        format!("{}/{}.tar.bz2", RELEASE_BASE, self.dir_name())
    }

    pub fn sha256(&self) -> &'static str {
        match self {
            Self::SenseVoice => "8148030f23c4bc0848239c80b635f3a0a1c275a2ae7ae37469bbe2341aa96d3f",
            Self::FireRedAed => "1b158e9d46715ed1cd387402b125de26f2e09bf2cb73926414b7fbd74d1973e2",
            Self::FireRedCtc2 => "1da8b737ecc5e29f36759a4460c754863e7c919a4ba325aea187331fbfc83274",
            Self::Qwen3Asr => "393f8a14e2f5fb96746aaab342997a40641001fbd5bf9592a080a8329178ee96",
        }
    }

    pub fn install_path(&self) -> PathBuf {
        config_dir().join(self.dir_name())
    }

    /// 检查解压后的关键模型文件是否在,作为"模型已下载"的判据。
    /// 各引擎的关键文件不同(单文件 vs encoder+decoder vs Qwen3 四件套)。
    pub fn is_available(&self) -> bool {
        let dir = self.install_path();
        match self {
            Self::SenseVoice => dir.join("model.int8.onnx").is_file(),
            Self::FireRedAed => {
                dir.join("encoder.int8.onnx").is_file() && dir.join("decoder.int8.onnx").is_file()
            }
            Self::FireRedCtc2 => dir.join("model.int8.onnx").is_file(),
            Self::Qwen3Asr => {
                dir.join("encoder.int8.onnx").is_file()
                    && dir.join("decoder.int8.onnx").is_file()
                    && dir.join("conv_frontend.onnx").is_file()
                    && dir.join("tokenizer").is_dir()
            }
        }
    }

    /// 解压包大小(MB),给 UI 标识。准确到 50 MB 即可。
    pub fn approx_size_mb(&self) -> u32 {
        match self {
            Self::SenseVoice => 226,
            Self::FireRedAed => 1400,
            Self::FireRedCtc2 => 496,
            Self::Qwen3Asr => 837,
        }
    }
}

// ============================================================================
// 公开 API
// ============================================================================

/// 当前 cfg 选中引擎的安装路径(给前端 LocalEnginePanel 显示)。
pub fn engine_install_path(engine: LocalEngine) -> PathBuf {
    engine.install_path()
}

/// 当前 cfg 选中的 engine 是否就绪。
pub fn engine_is_available(engine: LocalEngine) -> bool {
    engine.is_available()
}

// 兼容旧 commands(前端调 is_sense_voice_available 时还能 work)。
// 等前端切到 get_local_engine_info(id) 后这层薄 wrapper 可以删。
pub fn is_available() -> bool {
    LocalEngine::SenseVoice.is_available()
}

pub fn model_path() -> PathBuf {
    LocalEngine::SenseVoice.install_path()
}

pub const MODEL_URL_PREFIX: &str = RELEASE_BASE;

/// 主识别接口。从 cfg.local_engine 选 engine。
#[cfg(feature = "local-asr")]
pub async fn transcribe(cfg: &crate::config::Config, wav: &[u8]) -> Result<String> {
    use anyhow::anyhow;

    let engine = LocalEngine::from_id(&cfg.local_engine);
    if !engine.is_available() {
        anyhow::bail!("{} 模型未下载,请先在设置里下载", engine.label());
    }

    let signature = build_signature(cfg, engine);
    let hotwords_path = if cfg.hotwords.is_empty() {
        None
    } else {
        Some(write_hotwords_file(&cfg.hotwords)?)
    };

    // CoreML 当前 sherpa-onnx 1.13.x shared 模式下 ORT 1.24.4 已支持。
    // 但这里默认还是 cpu —— UI 上的开关临时屏蔽,留 config.local_use_coreml
    // 给手动开发者用(改 config.json 即开)。
    let provider = if cfg.local_use_coreml {
        "coreml"
    } else {
        "cpu"
    };

    let wav_vec = wav.to_vec();

    // 在独立线程跑 native 推理,避免阻塞 tokio runtime。with_recognizer 闭包
    // 在 cache mutex 持有期间执行,这样不需要 clone OfflineRecognizer
    // (sherpa-onnx 没实现 Clone)。voice-claude 设计上同一时刻只有一次录音,
    // 不会发生需要并发解码的情况,串行 mutex 没有性能损失。
    tokio::task::spawn_blocking(move || -> Result<String> {
        with_recognizer(
            engine,
            &signature,
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
        "本地 ASR 未启用。请用 `cargo build --features local-asr` 重新编译,或选择其他 ASR 后端"
    )
}

// ============================================================================
// Recognizer 缓存
// ============================================================================

#[cfg(feature = "local-asr")]
fn build_signature(cfg: &crate::config::Config, engine: LocalEngine) -> String {
    use std::collections::BTreeMap;
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    engine.id().hash(&mut h);
    cfg.local_use_coreml.hash(&mut h);
    // SenseVoice 的 fp32 切换:其他引擎当前未暴露这个开关,但保留进哈希
    // 防止"切了 SenseVoice fp32 后切到 FireRed 再切回"误命中
    cfg.local_use_fp32_model.hash(&mut h);
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
    engine: LocalEngine,
    signature: &str,
    provider: &str,
    hotwords_path: Option<&std::path::Path>,
    f: F,
) -> Result<R>
where
    F: FnOnce(&sherpa_onnx::OfflineRecognizer) -> Result<R>,
{
    use anyhow::anyhow;
    let mut guard = cache().lock();

    let needs_rebuild = match guard.as_ref() {
        Some(cached) if cached.signature == signature => false,
        Some(cached) => {
            tracing::info!(
                old = %cached.signature,
                new = %signature,
                "本地 ASR 配置变化,重建 recognizer"
            );
            true
        }
        None => {
            tracing::info!(
                provider,
                engine = engine.id(),
                "首次构建本地 ASR recognizer"
            );
            true
        }
    };

    if needs_rebuild {
        // drop 老的(释放 ONNX session 内存)再建新的,峰值内存不叠加
        *guard = None;
        let sconf = build_recognizer_config(engine, provider, hotwords_path);
        let recognizer = sherpa_onnx::OfflineRecognizer::create(&sconf)
            .ok_or_else(|| anyhow!("{} 初始化失败", engine.label()))?;
        *guard = Some(CachedRecognizer {
            signature: signature.to_string(),
            inner: recognizer,
        });
    } else {
        tracing::debug!(engine = engine.id(), "本地 ASR recognizer 命中缓存");
    }

    let cached = guard.as_ref().expect("just inserted or kept");
    f(&cached.inner)
}

/// 把每个 engine 的具体模型字段填进 OfflineRecognizerConfig。
#[cfg(feature = "local-asr")]
fn build_recognizer_config(
    engine: LocalEngine,
    provider: &str,
    hotwords_path: Option<&std::path::Path>,
) -> sherpa_onnx::OfflineRecognizerConfig {
    use sherpa_onnx::OfflineRecognizerConfig;

    let dir = engine.install_path();
    let path_str = |p: PathBuf| p.to_string_lossy().into_owned();
    let join = |name: &str| path_str(dir.join(name));

    let mut sconf = OfflineRecognizerConfig::default();
    sconf.model_config.num_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8) as i32)
        .unwrap_or(2);
    sconf.model_config.provider = Some(provider.to_string());
    sconf.decoding_method = Some("greedy_search".to_string());

    // tokens 路径:大多数引擎都在 dir/tokens.txt;Qwen3 用 tokenizer 目录
    if !matches!(engine, LocalEngine::Qwen3Asr) {
        sconf.model_config.tokens = Some(join("tokens.txt"));
    }

    match engine {
        LocalEngine::SenseVoice => {
            // SenseVoice 提供 int8 + fp32 两份模型文件,fp32 在 ARM Mac 上反而更快
            let model_filename = if false {
                // local_use_fp32_model 的开关现在只对 SenseVoice 有意义,这里默认走
                // int8 兼顾内存。要切 fp32 改 cfg + 重建 recognizer(签名会变)。
                "model.onnx"
            } else {
                "model.int8.onnx"
            };
            sconf.model_config.sense_voice.model = Some(join(model_filename));
            sconf.model_config.sense_voice.language = Some("zh".to_string());
            sconf.model_config.sense_voice.use_itn = true;
        }
        LocalEngine::FireRedAed => {
            sconf.model_config.fire_red_asr.encoder = Some(join("encoder.int8.onnx"));
            sconf.model_config.fire_red_asr.decoder = Some(join("decoder.int8.onnx"));
        }
        LocalEngine::FireRedCtc2 => {
            sconf.model_config.fire_red_asr_ctc.model = Some(join("model.int8.onnx"));
        }
        LocalEngine::Qwen3Asr => {
            sconf.model_config.qwen3_asr.conv_frontend = Some(join("conv_frontend.onnx"));
            sconf.model_config.qwen3_asr.encoder = Some(join("encoder.int8.onnx"));
            sconf.model_config.qwen3_asr.decoder = Some(join("decoder.int8.onnx"));
            sconf.model_config.qwen3_asr.tokenizer = Some(path_str(dir.join("tokenizer")));
        }
    }

    if let Some(p) = hotwords_path {
        sconf.hotwords_file = Some(p.to_string_lossy().into_owned());
        sconf.hotwords_score = 1.5;
    }

    sconf
}

// ============================================================================
// 模型下载 + 解压
// ============================================================================

/// 下载指定 engine 的模型。流式拉 + 字节级进度回调 (downloaded, total)。
pub async fn download_engine<F: Fn(u64, u64) + Send + 'static>(
    engine: LocalEngine,
    on_progress: F,
) -> Result<()> {
    use anyhow::Context;

    let url = engine.model_url();
    let response = reqwest::get(&url)
        .await
        .with_context(|| format!("下载 {} 失败", engine.label()))?;
    let total = response.content_length().unwrap_or(0);

    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("读取下载流失败")?;
        buf.extend_from_slice(&chunk);
        on_progress(buf.len() as u64, total);
    }

    install_from_bytes(engine, &buf)?;
    let final_size = buf.len() as u64;
    on_progress(final_size, final_size.max(total));
    Ok(())
}

/// 兼容老 command:固定下 SenseVoice。等前端切到 download_local_engine 后删。
pub async fn download_model<F: Fn(u64, u64) + Send + 'static>(on_progress: F) -> Result<()> {
    download_engine(LocalEngine::SenseVoice, on_progress).await
}

/// 从本地已下好的 tar.bz2 文件导入模型(国内下载失败的兜底路径)。
pub async fn import_engine_tarball(engine: LocalEngine, path: PathBuf) -> Result<()> {
    use anyhow::Context;
    tokio::task::spawn_blocking(move || -> Result<()> {
        let bytes = std::fs::read(&path).with_context(|| format!("读取 {:?} 失败", path))?;
        install_from_bytes(engine, &bytes)
    })
    .await?
}

/// 兼容老 command:固定 SenseVoice。
pub async fn import_tarball(path: PathBuf) -> Result<()> {
    import_engine_tarball(LocalEngine::SenseVoice, path).await
}

/// 校验 SHA256 → 解压 tar.bz2 到临时目录 → 原子替换到最终目录。
fn install_from_bytes(engine: LocalEngine, bytes: &[u8]) -> Result<()> {
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
    let expected_sha = engine.sha256();
    if actual_sha != expected_sha {
        bail!(
            "{} SHA256 校验失败(期望 {},实际 {}),文件可能损坏",
            engine.label(),
            expected_sha,
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

    let final_dir = dest_dir.join(engine.dir_name());
    fs::remove_dir_all(&final_dir).ok();
    fs::rename(&tmp_dir, &final_dir).context("移动模型目录失败")?;
    Ok(())
}

fn tempdir_in(parent: &std::path::Path) -> Result<PathBuf> {
    use std::fs;
    for _ in 0..10 {
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let p = parent.join(format!("local-engine-tmp-{}", ts));
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
    fn engine_id_roundtrip() {
        for &e in LocalEngine::ALL {
            assert_eq!(LocalEngine::from_id(e.id()), e);
        }
    }

    #[test]
    fn engine_dir_unique() {
        use std::collections::HashSet;
        let dirs: HashSet<_> = LocalEngine::ALL.iter().map(|e| e.dir_name()).collect();
        assert_eq!(dirs.len(), LocalEngine::ALL.len());
    }

    #[test]
    fn sha256_format() {
        for &e in LocalEngine::ALL {
            assert_eq!(e.sha256().len(), 64, "{} sha256", e.id());
        }
    }

    #[test]
    fn url_format() {
        for &e in LocalEngine::ALL {
            let url = e.model_url();
            assert!(url.starts_with("https://github.com/k2-fsa/sherpa-onnx/"));
            assert!(url.ends_with(".tar.bz2"));
        }
    }

    #[test]
    fn unknown_id_falls_back_to_sense_voice() {
        assert_eq!(LocalEngine::from_id(""), LocalEngine::SenseVoice);
        assert_eq!(LocalEngine::from_id("unknown"), LocalEngine::SenseVoice);
    }

    #[test]
    fn parse_wav_roundtrip() {
        let pcm = vec![0u8, 0u8, 0x00u8, 0x10u8];
        let wav = crate::asr::wav::build_wav(&pcm);
        let (samples, rate) = wav_bytes_to_samples(&wav).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(samples.len(), 2);
    }
}
