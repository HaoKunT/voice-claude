//! 小米 MiMo ASR(`mimo-v2.5-asr`,HTTP 批处理)。
//!
//! 协议是 OpenAI Chat Completions 兼容 + `input_audio` modality + `asr_options`
//! 扩展字段 —— message.content[0] 放 `data:audio/wav;base64,...` data URL,
//! 顶层加 `asr_options.language`。
//!
//! Endpoint 两种模式:
//!
//! | 模式     | base_url               | model            | 说明                          |
//! |----------|------------------------|------------------|-------------------------------|
//! | `public` | `api.xiaomimimo.com`   | `mimo-v2.5-asr`  | 官方公开服务,默认            |
//! | `custom` | 用户自填 `mimo_base_url` | 用户自填 `mimo_model` | 自部署 / 自托管 endpoint     |
//!
//! `custom` 模式存在的原因:MiMo-V2.5-ASR 模型权重是开源的
//! (<https://huggingface.co/XiaomiMiMo/MiMo-V2.5-ASR>),用户可以用
//! vLLM / sglang 等推理框架自部署成同样的 OpenAI 兼容 endpoint;只要
//! request body shape 一致,base_url + model 改两个字段就能复用整套
//! 调用代码,不必为自托管再写一份后端。
//!
//! 文档:<https://platform.xiaomimimo.com/docs/zh-CN/api/audio/Speech-Recognition>
//!
//! 音频上限:文档说 base64 ≤ 10MB。我们送的是 16kHz mono PCM16 WAV,
//! 30s 约 960KB → base64 1.3MB,远低于上限,不切段。

use crate::config::Config;
use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const PUBLIC_BASE_URL: &str = "https://api.xiaomimimo.com/v1/chat/completions";
const PUBLIC_MODEL: &str = "mimo-v2.5-asr";

pub const MIMO_ENDPOINT_PUBLIC: &str = "public";
pub const MIMO_ENDPOINT_CUSTOM: &str = "custom";

#[derive(Serialize)]
struct InputAudio {
    /// data URL: `data:audio/wav;base64,<base64>`
    data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'static str>,
}

#[derive(Serialize)]
struct ContentItem {
    #[serde(rename = "type")]
    kind: &'static str,
    input_audio: InputAudio,
}

#[derive(Serialize)]
struct Message {
    role: &'static str,
    content: Vec<ContentItem>,
}

#[derive(Serialize)]
struct AsrOptions<'a> {
    language: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    asr_options: AsrOptions<'a>,
}

/// OpenAI 风格 choices[0].message.content;ASR 模式下 content 是纯文本。
#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    message: ChoiceMessage,
}

#[derive(Deserialize, Default)]
struct ChoiceMessage {
    #[serde(default)]
    content: String,
}

pub async fn transcribe(cfg: &Config, wav: &[u8]) -> Result<String> {
    if cfg.mimo_api_key.is_empty() {
        anyhow::bail!("请配置 MiMo API Key");
    }
    let (url, model) = resolve_endpoint(cfg)?;
    let language = effective_language(cfg.mimo_language.as_str());

    let wav_b64 = base64::engine::general_purpose::STANDARD.encode(wav);
    let data_url = format!("data:audio/wav;base64,{}", wav_b64);

    let body = ChatRequest {
        model,
        messages: vec![Message {
            role: "user",
            content: vec![ContentItem {
                kind: "input_audio",
                input_audio: InputAudio {
                    data: data_url,
                    // format 字段非必填(data URL 已带 MIME),给 wav 兜底
                    format: Some("wav"),
                },
            }],
        }],
        asr_options: AsrOptions { language },
    };
    tracing::info!(endpoint = %cfg.mimo_endpoint, model, language, "mimo ASR 请求");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let resp = client
        .post(url)
        .bearer_auth(&cfg.mimo_api_key)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("mimo API {}: {}", status, body);
    }
    let parsed: ChatResponse = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("parse response: {} (body: {})", e, body))?;
    if let Some(e) = parsed.error {
        anyhow::bail!("mimo error: {}", e);
    }
    let text = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();
    Ok(text)
}

/// 根据 cfg.mimo_endpoint 决定 (URL, model)。
///
/// - `public`(默认):用公开服务的常量
/// - `custom`:用 cfg.mimo_base_url + cfg.mimo_model;两者都不能为空
/// - 其他未知值兜底到 public
fn resolve_endpoint(cfg: &Config) -> Result<(String, &str)> {
    match cfg.mimo_endpoint.as_str() {
        MIMO_ENDPOINT_CUSTOM => {
            let base = cfg.mimo_base_url.trim();
            let model = cfg.mimo_model.trim();
            if base.is_empty() {
                anyhow::bail!("自定义 endpoint 需要填 base_url");
            }
            if model.is_empty() {
                anyhow::bail!("自定义 endpoint 需要填 model id");
            }
            Ok((base.to_string(), model))
        }
        // public 或未知值
        _ => Ok((PUBLIC_BASE_URL.to_string(), PUBLIC_MODEL)),
    }
}

/// language 字段非空时直接用;空字符串 → "auto"(文档支持 auto / zh / en)。
fn effective_language(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        "auto"
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(endpoint: &str, base: &str, model: &str) -> Config {
        Config {
            mimo_endpoint: endpoint.into(),
            mimo_base_url: base.into(),
            mimo_model: model.into(),
            ..Config::default()
        }
    }

    #[test]
    fn endpoint_public_uses_default_constants() {
        let cfg = cfg_with(MIMO_ENDPOINT_PUBLIC, "", "");
        let (url, model) = resolve_endpoint(&cfg).unwrap();
        assert_eq!(url, PUBLIC_BASE_URL);
        assert_eq!(model, PUBLIC_MODEL);
    }

    #[test]
    fn endpoint_unknown_falls_back_to_public() {
        let cfg = cfg_with("garbage", "", "");
        let (url, model) = resolve_endpoint(&cfg).unwrap();
        assert_eq!(url, PUBLIC_BASE_URL);
        assert_eq!(model, PUBLIC_MODEL);
    }

    #[test]
    fn endpoint_custom_uses_user_fields() {
        let cfg = cfg_with(
            MIMO_ENDPOINT_CUSTOM,
            "https://example.com/v1/chat",
            "my-model",
        );
        let (url, model) = resolve_endpoint(&cfg).unwrap();
        assert_eq!(url, "https://example.com/v1/chat");
        assert_eq!(model, "my-model");
    }

    #[test]
    fn endpoint_custom_requires_base_url() {
        let cfg = cfg_with(MIMO_ENDPOINT_CUSTOM, "", "m");
        assert!(resolve_endpoint(&cfg).is_err());
    }

    #[test]
    fn endpoint_custom_requires_model() {
        let cfg = cfg_with(MIMO_ENDPOINT_CUSTOM, "https://x", "");
        assert!(resolve_endpoint(&cfg).is_err());
    }

    #[test]
    fn empty_language_becomes_auto() {
        assert_eq!(effective_language(""), "auto");
        assert_eq!(effective_language("  "), "auto");
        assert_eq!(effective_language("zh"), "zh");
    }

    #[test]
    fn request_body_serializes_to_expected_shape() {
        let body = ChatRequest {
            model: PUBLIC_MODEL,
            messages: vec![Message {
                role: "user",
                content: vec![ContentItem {
                    kind: "input_audio",
                    input_audio: InputAudio {
                        data: "data:audio/wav;base64,AAAA".into(),
                        format: Some("wav"),
                    },
                }],
            }],
            asr_options: AsrOptions { language: "zh" },
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "mimo-v2.5-asr");
        assert_eq!(json["messages"][0]["content"][0]["type"], "input_audio");
        assert_eq!(
            json["messages"][0]["content"][0]["input_audio"]["data"],
            "data:audio/wav;base64,AAAA"
        );
        assert_eq!(json["asr_options"]["language"], "zh");
    }
}
