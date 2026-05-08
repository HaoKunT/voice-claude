//! 本地 SenseVoice ASR（离线）。
//! 对应 Go 版的 local_asr.go。
//!
//! 目前 stub：sherpa-onnx 的 Rust 绑定在不同平台上打包复杂，暂时只声明接口。
//! 后续可用 `sherpa-rs` 或直接 FFI 调用预编译 C 库接入。

use anyhow::{bail, Result};

pub async fn transcribe(_wav: &[u8]) -> Result<String> {
    bail!("本地 SenseVoice 暂未实现，请选择其他后端");
}

/// 检查本地模型是否可用（stub）。
pub fn is_available() -> bool {
    false
}
