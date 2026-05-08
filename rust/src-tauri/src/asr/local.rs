//! 本地 SenseVoice ASR（离线）。
//! 对应 Go 版的 local_asr.go（+nosherpa 时是 stub）。
//!
//! 启用 feature `local-asr` 时，通过 `sherpa-onnx` crate 调用 SenseVoice 模型；
//! 未启用时（默认）与 Go CI `-tags nosherpa` 的发布产物行为等价。

use crate::dirs::config_dir;
use anyhow::Result;
use std::path::PathBuf;

/// SenseVoice 模型目录（沿用 Go 版命名）
pub const MODEL_DIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

/// 模型下载 URL + SHA256
pub const MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";
pub const MODEL_SHA256: &str = "8148030f23c4bc0848239c80b635f3a0a1c275a2ae7ae37469bbe2341aa96d3f";

pub fn model_path() -> PathBuf {
    config_dir().join(MODEL_DIR)
}

/// 模型是否已下载。
pub fn is_available() -> bool {
    let model_file = model_path().join("model.int8.onnx");
    model_file.is_file()
}

/// 主识别接口。
///
/// sherpa-onnx 1.13 Rust API 和 Go API 在字段命名 / Option wrapping / 构造方式上差异较大：
///   - `OfflineSenseVoiceModelConfig` 用 `model: Option<String>` + `use_itn: bool`
///   - `OfflineRecognizer::create(&cfg) -> Option<Self>`（注意是 Option 不是 Result）
///   - `stream.get_result() -> Option<OfflineRecognitionResult>`
///
/// 完整接入需要仔细对齐 API 字段 + 测试真实模型推理。作为独立 iteration 完成。
/// 现阶段即使开启 feature 也返回未实现提示，保证默认编译和 CI 发布产物稳定。
pub async fn transcribe(_wav: &[u8]) -> Result<String> {
    anyhow::bail!(
        "本地 SenseVoice 尚未完全接入 sherpa-onnx 1.13 Rust API，请选择其他 ASR 后端（讯飞/豆包/智谱/OpenRouter）"
    )
}

/// 下载模型（stub，完整实现见 Go 版 DownloadSenseVoiceModel）。
pub async fn download_model(_on_progress: impl Fn(f32) + Send + 'static) -> Result<()> {
    anyhow::bail!("本地 SenseVoice 下载暂未实现，后续 iteration 接入")
}

/// 从 WAV 字节解析 float32 PCM。为 sherpa-onnx native 接入做的工具函数。
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
}
