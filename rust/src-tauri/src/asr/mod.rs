//! ASR 后端模块：每个后端一个子模块，共享基础工具。

pub mod local;
pub mod openrouter;
pub mod volc;
pub mod wav;
pub mod xfyun;
pub mod zhipu;

use crate::config::{
    Config, ASR_PROVIDER_LOCAL, ASR_PROVIDER_OPENROUTER, ASR_PROVIDER_VOLC, ASR_PROVIDER_XFYUN,
};
use anyhow::Result;
use tokio::sync::mpsc;

/// 流式 ASR 接口：边录边识别。
///
/// - `pcm_rx` 接收 PCM 块，调用方关闭 channel 表示录音结束
/// - `on_partial` 每次中间结果回调
/// - `ready` 连接就绪后通知调用方可以开始推送 PCM
pub async fn transcribe_stream(
    cfg: &Config,
    pcm_rx: mpsc::Receiver<Vec<u8>>,
    on_partial: Box<dyn Fn(String) + Send + Sync>,
    ready: tokio::sync::oneshot::Sender<()>,
) -> Result<String> {
    match cfg.asr_provider.as_str() {
        ASR_PROVIDER_XFYUN => xfyun::transcribe_stream(cfg, pcm_rx, on_partial, ready).await,
        ASR_PROVIDER_VOLC => volc::transcribe_stream(cfg, pcm_rx, on_partial, ready).await,
        _ => anyhow::bail!("provider {} 不支持流式", cfg.asr_provider),
    }
}

/// 批处理 ASR：录完再识别。
pub async fn transcribe_batch(cfg: &Config, wav: &[u8]) -> Result<String> {
    match cfg.asr_provider.as_str() {
        ASR_PROVIDER_LOCAL => local::transcribe(cfg, wav).await,
        ASR_PROVIDER_OPENROUTER => openrouter::transcribe(cfg, wav).await,
        _ => zhipu::transcribe(cfg, wav).await,
    }
}

/// 判断当前 provider 是否支持流式。
pub fn is_streaming(provider: &str) -> bool {
    matches!(provider, ASR_PROVIDER_XFYUN | ASR_PROVIDER_VOLC)
}
