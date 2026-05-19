//! 本地 ASR 多引擎(离线)。
//!
//! feature `local-asr` 默认 ON,通过 `sherpa-onnx` crate 调用。
//!
//! 3 个引擎(用户在设置页切换):
//!   - SenseVoice  226MB  多语言 NAR,日常默认 / 极速
//!   - FireRed-AED 1.4GB  中文 SOTA / 准度最高(标点需独立模型)
//!   - Qwen3-ASR   837MB  LLM 体系 / 自带标点(易脑补)
//!
//! 共用下载 / 校验 / 解压 / recognizer 缓存,只在 build_recognizer_config
//! 里按 engine 给 OfflineModelConfig 不同子字段赋值。

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
    Qwen3Asr,
}

const RELEASE_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

impl LocalEngine {
    /// 全部引擎列表(给前端 dropdown 用)。
    pub const ALL: &'static [Self] = &[Self::SenseVoice, Self::FireRedAed, Self::Qwen3Asr];

    /// 从 config 字符串 id 解析,未知/空字符串回退到 SenseVoice。
    pub fn from_id(id: &str) -> Self {
        match id {
            "fire_red_aed" => Self::FireRedAed,
            "qwen3_asr" => Self::Qwen3Asr,
            _ => Self::SenseVoice,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::SenseVoice => "sense_voice",
            Self::FireRedAed => "fire_red_aed",
            Self::Qwen3Asr => "qwen3_asr",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::SenseVoice => "SenseVoice · 极速",
            Self::FireRedAed => "FireRedASR-AED-L · 中文最准",
            Self::Qwen3Asr => "Qwen3-ASR · LLM 体系",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::SenseVoice => "多语言 NAR · 推理 ~200-400ms · 占用 ~300MB · 日常默认推荐",
            Self::FireRedAed => "中文 SOTA(AISHELL 0.55) · 推理 2-4s · 占用 ~2GB · 标点需独立模型",
            Self::Qwen3Asr => "LLM 体系 · 自带标点 · 推理 4-6s · 占用 ~1.5GB · 易脑补幻觉",
        }
    }

    /// 模型解压后的目录名(也是 release artifact 名去掉扩展名)。
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::SenseVoice => "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
            Self::FireRedAed => "sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16",
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
            Self::Qwen3Asr => "393f8a14e2f5fb96746aaab342997a40641001fbd5bf9592a080a8329178ee96",
        }
    }

    pub fn install_path(&self) -> PathBuf {
        config_dir().join(self.dir_name())
    }

    /// 关键模型文件是否在 → 作为"模型已下载"的判据。
    pub fn is_available(&self) -> bool {
        let dir = self.install_path();
        match self {
            Self::SenseVoice => dir.join("model.int8.onnx").is_file(),
            Self::FireRedAed => {
                dir.join("encoder.int8.onnx").is_file() && dir.join("decoder.int8.onnx").is_file()
            }
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
            Self::Qwen3Asr => 837,
        }
    }

    /// 该引擎自身是否输出标点。SenseVoice / Qwen3-ASR(LLM)自带;
    /// FireRedASR-AED-L 训练集(AISHELL/WenetSpeech)无标点,需后处理挂 PunctModel。
    pub fn has_native_punctuation(&self) -> bool {
        matches!(self, Self::SenseVoice | Self::Qwen3Asr)
    }

    /// transcribe 之后是否要跑 PunctModel(用户配了模型时才生效)。
    pub fn needs_punctuation_postprocess(&self) -> bool {
        !self.has_native_punctuation()
    }
}

// ============================================================================
// 标点模型 (sherpa-onnx ct-transformer zh+en)
// ============================================================================
//
// 仅 FireRedASR 系列(AED / CTC2)输出无标点,跑这个模型补全。SenseVoice /
// Qwen3-ASR 已带标点,跳过。模型可选下载 —— 没装时 FireRed 输出原样无标点。

const PUNCT_DIR_NAME: &str = "sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12";
const PUNCT_SHA256: &str = "50f73f8cccffc2303999fda28b785ffcffbd7ea442c47385c30b9d045ee6afc3";
const PUNCT_LABEL: &str = "中英标点模型(ct-transformer)";
const PUNCT_DESCRIPTION: &str =
    "279MB,FireRedASR 系列输出无标点,挂这个模型补全。SenseVoice / Qwen3-ASR 自带标点,不需要装。";
const PUNCT_SIZE_MB: u32 = 279;

pub fn punct_model_url() -> String {
    format!(
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/punctuation-models/{}.tar.bz2",
        PUNCT_DIR_NAME
    )
}

pub fn punct_model_dir_name() -> &'static str {
    PUNCT_DIR_NAME
}

pub fn punct_model_sha256() -> &'static str {
    PUNCT_SHA256
}

pub fn punct_model_label() -> &'static str {
    PUNCT_LABEL
}

pub fn punct_model_description() -> &'static str {
    PUNCT_DESCRIPTION
}

pub fn punct_model_size_mb() -> u32 {
    PUNCT_SIZE_MB
}

pub fn punct_model_install_path() -> PathBuf {
    config_dir().join(PUNCT_DIR_NAME)
}

pub fn punct_model_is_available() -> bool {
    punct_model_install_path().join("model.onnx").is_file()
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
    let needs_punct = engine.needs_punctuation_postprocess() && punct_model_is_available();

    // 在独立线程跑 native 推理,避免阻塞 tokio runtime。with_recognizer 闭包
    // 在 cache mutex 持有期间执行,这样不需要 clone OfflineRecognizer
    // (sherpa-onnx 没实现 Clone)。voice-claude 设计上同一时刻只有一次录音,
    // 不会发生需要并发解码的情况,串行 mutex 没有性能损失。
    tokio::task::spawn_blocking(move || -> Result<String> {
        let raw = with_recognizer(
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
        )?;
        if needs_punct {
            Ok(add_punctuation_blocking(&raw))
        } else {
            Ok(raw)
        }
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
// 模型生命周期:启动预热 / 配置切换卸载
// ============================================================================

/// 同步加载当前 cfg 选中的本地引擎 (+ FireRed 系列的标点模型) 到内存 cache。
/// 调用方应在后台线程跑(spawn_blocking),避免阻塞 tokio 调度。
/// 失败时返回 Err 仅供日志记录;预热是 best-effort,不该影响主功能。
#[cfg(feature = "local-asr")]
pub fn warm_up(cfg: &crate::config::Config) -> Result<()> {
    let engine = LocalEngine::from_id(&cfg.local_engine);
    if !engine.is_available() {
        tracing::debug!(engine = engine.id(), "warm_up: 模型未下载,跳过");
        return Ok(());
    }

    let signature = build_signature(cfg, engine);
    let provider = if cfg.local_use_coreml {
        "coreml"
    } else {
        "cpu"
    };
    let hotwords_path = if cfg.hotwords.is_empty() {
        None
    } else {
        Some(write_hotwords_file(&cfg.hotwords)?)
    };

    // 闭包返回 () 即可 —— with_recognizer 内部会按 signature 重建/复用 cache,
    // 我们不需要真识别,只为让 OfflineRecognizer 进 cache 准备好首次推理
    with_recognizer(
        engine,
        &signature,
        provider,
        hotwords_path.as_deref(),
        |_| Ok(()),
    )?;
    tracing::info!(engine = engine.id(), "warm_up: ASR 模型已预热");

    // FireRed 系列输出无标点,需要 ct-transformer 标点模型 —— 也预热一下
    if engine.needs_punctuation_postprocess() && punct_model_is_available() {
        // 空字符串调用一次触发 punct cache 构建,失败也无所谓(返回原文)
        let _ = add_punctuation_blocking("。");
        tracing::info!("warm_up: 标点模型已预热");
    }
    Ok(())
}

#[cfg(not(feature = "local-asr"))]
pub fn warm_up(_cfg: &crate::config::Config) -> Result<()> {
    Ok(())
}

/// 卸载 ASR + 标点 cache,drop 掉 OfflineRecognizer / OfflinePunctuation 释放
/// ONNX session 占用的内存(SenseVoice ~300MB / FireRed ~2GB / Qwen3 ~1GB)。
/// 用户从 local 切到云端时调,避免内存继续占着。
#[cfg(feature = "local-asr")]
pub fn unload() {
    let asr_dropped = cache().lock().take().is_some();
    let punct_dropped = punct_cache().lock().take().is_some();
    if asr_dropped || punct_dropped {
        tracing::info!(asr = asr_dropped, punct = punct_dropped, "本地模型已卸载");
    }
}

#[cfg(not(feature = "local-asr"))]
pub fn unload() {}

// ============================================================================
// Recognizer 缓存
// ============================================================================

#[cfg(feature = "local-asr")]
fn build_signature(cfg: &crate::config::Config, engine: LocalEngine) -> String {
    use std::collections::BTreeSet;
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    engine.id().hash(&mut h);
    cfg.local_use_coreml.hash(&mut h);
    // hotwords 是 Vec<String>;按内容哈希(去重 + 排序确保稳定)
    let sorted: BTreeSet<&String> = cfg.hotwords.iter().collect();
    for w in &sorted {
        w.hash(&mut h);
    }
    format!("{:x}", h.finish())
}

#[cfg(feature = "local-asr")]
fn write_hotwords_file(hotwords: &[String]) -> Result<std::path::PathBuf> {
    use std::collections::BTreeSet;
    // 去重 + 排序后写盘,sherpa-onnx 一行一词
    let set: BTreeSet<&str> = hotwords
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
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

/// 标点模型缓存:第一次用时构造,之后命中缓存。失败/未装时返回原文,不抛错。
#[cfg(feature = "local-asr")]
fn punct_cache() -> &'static parking_lot::Mutex<Option<sherpa_onnx::OfflinePunctuation>> {
    use std::sync::OnceLock;
    static CELL: OnceLock<parking_lot::Mutex<Option<sherpa_onnx::OfflinePunctuation>>> =
        OnceLock::new();
    CELL.get_or_init(|| parking_lot::Mutex::new(None))
}

/// 给 text 加标点。模型未装 / 加载失败 / add 失败时返回原文,不抛错 ——
/// 标点是锦上添花,不应该让整个 ASR 流程因为它崩。
#[cfg(feature = "local-asr")]
fn add_punctuation_blocking(text: &str) -> String {
    use sherpa_onnx::{OfflinePunctuation, OfflinePunctuationConfig};

    if !punct_model_is_available() {
        return text.to_string();
    }
    let mut guard = punct_cache().lock();
    if guard.is_none() {
        let model_path = punct_model_install_path()
            .join("model.onnx")
            .to_string_lossy()
            .into_owned();
        let mut sconf = OfflinePunctuationConfig::default();
        sconf.model.ct_transformer = Some(model_path);
        sconf.model.num_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(4) as i32)
            .unwrap_or(2);
        match OfflinePunctuation::create(&sconf) {
            Some(p) => {
                tracing::info!("标点模型已加载");
                *guard = Some(p);
            }
            None => {
                tracing::warn!("标点模型初始化失败,跳过标点");
                return text.to_string();
            }
        }
    }
    let p = guard.as_ref().unwrap();
    match p.add_punctuation(text) {
        Some(s) => s,
        None => {
            tracing::warn!("标点 add_punctuation 失败,返回原文");
            text.to_string()
        }
    }
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
            sconf.model_config.sense_voice.model = Some(join("model.int8.onnx"));
            sconf.model_config.sense_voice.language = Some("zh".to_string());
            sconf.model_config.sense_voice.use_itn = true;
        }
        LocalEngine::FireRedAed => {
            sconf.model_config.fire_red_asr.encoder = Some(join("encoder.int8.onnx"));
            sconf.model_config.fire_red_asr.decoder = Some(join("decoder.int8.onnx"));
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

/// 校验 SHA256 → 解压 tar.bz2 到临时目录 → 原子替换到 config_dir/<dir_name>。
/// LocalEngine 和 PunctModel 共用这一套。
fn install_tarball(bytes: &[u8], expected_sha: &str, dir_name: &str, label: &str) -> Result<()> {
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
    if actual_sha != expected_sha {
        bail!(
            "{} SHA256 校验失败(期望 {},实际 {}),文件可能损坏",
            label,
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

    let final_dir = dest_dir.join(dir_name);
    fs::remove_dir_all(&final_dir).ok();
    fs::rename(&tmp_dir, &final_dir).context("移动模型目录失败")?;
    Ok(())
}

fn install_from_bytes(engine: LocalEngine, bytes: &[u8]) -> Result<()> {
    install_tarball(bytes, engine.sha256(), engine.dir_name(), engine.label())
}

/// 下载标点模型。流式拉 + 进度回调。
pub async fn download_punct_model<F: Fn(u64, u64) + Send + 'static>(on_progress: F) -> Result<()> {
    use anyhow::Context;

    let url = punct_model_url();
    let response = reqwest::get(&url)
        .await
        .with_context(|| format!("下载 {} 失败", PUNCT_LABEL))?;
    let total = response.content_length().unwrap_or(0);

    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("读取下载流失败")?;
        buf.extend_from_slice(&chunk);
        on_progress(buf.len() as u64, total);
    }

    install_tarball(&buf, PUNCT_SHA256, PUNCT_DIR_NAME, PUNCT_LABEL)?;
    let final_size = buf.len() as u64;
    on_progress(final_size, final_size.max(total));
    Ok(())
}

/// 从本地 tar.bz2 导入标点模型(国内下载兜底)。
pub async fn import_punct_tarball(path: PathBuf) -> Result<()> {
    use anyhow::Context;
    tokio::task::spawn_blocking(move || -> Result<()> {
        let bytes = std::fs::read(&path).with_context(|| format!("读取 {:?} 失败", path))?;
        install_tarball(&bytes, PUNCT_SHA256, PUNCT_DIR_NAME, PUNCT_LABEL)
    })
    .await?
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
