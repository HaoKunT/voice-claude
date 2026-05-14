//! silero-vad 模型管理 + Detector 创建 helper。
//!
//! 模型只一个 .onnx 文件 ~640KB,首次启用 VAD 时自动下载到 config_dir。
//! 跟 SenseVoice 走的下载-校验-存盘流程同构,但单文件不需要 tar/bz2 解压。
//!
//! 用 sherpa-onnx 暴露的 VoiceActivityDetector + SileroVadModelConfig
//! (`vad.rs` in sherpa-onnx 1.13.1 crate)。

use crate::dirs::config_dir;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub const MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
pub const MODEL_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";

pub fn model_path() -> PathBuf {
    config_dir().join("silero_vad.onnx")
}

pub fn is_available() -> bool {
    model_path().is_file()
}

/// 同步下载 silero-vad 模型(640KB,几秒钟)。已存在就跳过。
/// 校验 SHA256,失败留下损坏文件让用户察觉(不写到目标路径)。
pub async fn download_if_needed() -> Result<()> {
    let path = model_path();
    if path.is_file() {
        return Ok(());
    }
    use sha2::{Digest, Sha256};

    tracing::info!(url = MODEL_URL, "首次下载 silero-vad 模型");
    let bytes = reqwest::get(MODEL_URL)
        .await
        .context("下载 silero-vad 模型失败")?
        .bytes()
        .await
        .context("读取 silero-vad 响应体失败")?;

    let actual_sha = hex::encode(Sha256::digest(&bytes));
    if actual_sha != MODEL_SHA256 {
        anyhow::bail!(
            "silero-vad SHA256 校验失败(期望 {},实际 {})",
            MODEL_SHA256,
            actual_sha
        );
    }
    std::fs::create_dir_all(config_dir()).ok();
    std::fs::write(&path, &bytes).context("写入 silero-vad 模型失败")?;
    tracing::info!(path = ?path, bytes = bytes.len(), "silero-vad 模型已下载");
    Ok(())
}

/// 用当前 config 构造一个 silero VoiceActivityDetector。需要 local-asr feature
/// (复用同一份 sherpa-onnx C 库 / ORT)。
///
/// `threshold`:silero 输出概率阈值 0-1,默认 0.5
/// `min_silence_ms`:多长持续静音才算一段说话结束(silero 内部 segment 切分用)
#[cfg(feature = "local-asr")]
pub fn create_detector(
    threshold: f32,
    min_silence_ms: u32,
) -> Result<sherpa_onnx::VoiceActivityDetector> {
    use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

    if !is_available() {
        anyhow::bail!("silero-vad 模型未下载,请先调 download_if_needed");
    }

    let silero = SileroVadModelConfig {
        model: Some(model_path().to_string_lossy().into_owned()),
        threshold,
        min_silence_duration: (min_silence_ms as f32) / 1000.0,
        min_speech_duration: 0.25, // 250ms,过滤咳嗽 / 椅子声
        window_size: 512,          // silero 默认 32ms @ 16kHz
        max_speech_duration: 60.0, // 一段最长说话 60s,超出强制 finalize
    };

    let cfg = VadModelConfig {
        silero_vad: silero,
        sample_rate: 16000,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    };

    VoiceActivityDetector::create(&cfg, /* buffer_size_in_seconds */ 30.0)
        .ok_or_else(|| anyhow::anyhow!("silero-vad 初始化失败"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_in_config_dir() {
        assert!(model_path().to_string_lossy().contains("silero_vad.onnx"));
    }

    #[test]
    fn sha256_format() {
        assert_eq!(MODEL_SHA256.len(), 64);
        assert!(MODEL_URL.starts_with("https://github.com/k2-fsa/"));
    }
}
