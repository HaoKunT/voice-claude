//! 本地 SenseVoice ASR（离线）。
//! 对应 Go 版的 local_asr.go。
//!
//! 现阶段是 **stub**——和 Go 版 `-tags nosherpa` 的 CI 发布产物行为等价。
//! 完整 native 接入需要：
//!   1. 引入 `sherpa-onnx` crate（或自行 FFI 到 sherpa-onnx C 库）
//!   2. 处理 build.rs 链接脚本 + 预编译二进制分发
//!   3. macOS 的 dylib + Windows 的 dll 分平台打包
//!
//! 这些工作风险较大（一个 build.rs 失败可能让整个 Rust 项目编译中断），
//! 留到独立 iteration 完成。当前状态：
//! - 用户在设置选"本地 SenseVoice"后会看到"暂未实现"提示
//! - 下载模型 / 校验 SHA256 / 解压 tar.bz2 等基础设施也一并延后
//!
//! 等价替代：用户可通过讯飞/豆包/智谱/OpenRouter 任一后端获得识别能力。

use crate::dirs::config_dir;
use anyhow::{bail, Result};
use std::path::PathBuf;

/// SenseVoice 模型目录（沿用 Go 版命名）
pub const MODEL_DIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

/// 模型下载 URL + SHA256（沿用 Go 版常量，后续接入时复用）
pub const MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";
pub const MODEL_SHA256: &str = "8148030f23c4bc0848239c80b635f3a0a1c275a2ae7ae37469bbe2341aa96d3f";

/// 模型安装根目录。
pub fn model_path() -> PathBuf {
    config_dir().join(MODEL_DIR)
}

/// 主识别接口（stub）。
pub async fn transcribe(_wav: &[u8]) -> Result<String> {
    bail!("本地 SenseVoice 暂未在 Rust 版实现，请选择其他 ASR 后端（讯飞/豆包/智谱/OpenRouter）")
}

/// 模型是否已下载。
pub fn is_available() -> bool {
    let model_file = model_path().join("model.int8.onnx");
    model_file.is_file()
}

/// 下载模型（stub）。
/// 完整实现应：GET MODEL_URL → TeeReader → bz2 解压 → tar 解包 →
/// SHA256 校验 → 原子移动到 model_path()。
pub async fn download_model(_on_progress: impl Fn(f32) + Send + 'static) -> Result<()> {
    bail!("本地 SenseVoice 下载暂未实现，后续 iteration 接入")
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
