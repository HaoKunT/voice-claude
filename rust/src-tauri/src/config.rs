//! 应用配置：JSON 存盘，跨平台路径由 dirs 模块提供。
//! 对应 Go 版的 config.go。

use crate::dirs::{config_dir, config_path};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

pub const ASR_PROVIDER_ZHIPU: &str = "zhipu";
pub const ASR_PROVIDER_XFYUN: &str = "xfyun";
pub const ASR_PROVIDER_VOLC: &str = "volc";
pub const ASR_PROVIDER_OPENROUTER: &str = "openrouter";
pub const ASR_PROVIDER_LOCAL: &str = "local";

pub const POLISH_MODE_OFF: &str = "off";
pub const POLISH_MODE_OLLAMA: &str = "ollama";
pub const POLISH_MODE_OPENROUTER: &str = "openrouter";
pub const POLISH_MODE_CLOUD: &str = "cloud";

pub const OUTPUT_MODE_INPUT: &str = "input";
pub const OUTPUT_MODE_CLIPBOARD: &str = "clipboard";
pub const OUTPUT_MODE_PANEL: &str = "panel";

pub const DEFAULT_PROFILE_ID: &str = "default";
pub const DEFAULT_POLISH_PROMPT: &str =
    "你是一个语音识别润色助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
如果原文没有明显错误，直接返回原文。
只输出润色后的文本，不要解释。

原文：{text}";

/// 单个 AI 润色 profile：一套完整的后端配置 + prompt 模板。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishProfile {
    pub id: String,
    pub name: String,
    #[serde(default = "default_polish_mode")]
    pub mode: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_polish_prompt")]
    pub prompt: String,
}

impl PolishProfile {
    pub fn default_named(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            mode: default_polish_mode(),
            url: default_correct_url(),
            model: default_correct_model(),
            api_key: String::new(),
            prompt: default_polish_prompt(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_asr_provider")]
    pub asr_provider: String,
    #[serde(default)]
    pub asr_api_key: String,
    #[serde(default)]
    pub xfyun_app_id: String,
    #[serde(default)]
    pub xfyun_access_key_id: String,
    #[serde(default, rename = "xfyun_access_key_secret")]
    pub xfyun_access_secret: String,
    #[serde(default)]
    pub openrouter_api_key: String,
    /// OpenRouter ASR 模型 slug。常用:
    ///   openai/whisper-large-v3-turbo(默认,便宜)
    ///   openai/gpt-4o-mini-transcribe(新,对非标准语音更鲁棒)
    ///   openai/gpt-4o-transcribe(最好)
    #[serde(default = "default_openrouter_model")]
    pub openrouter_model: String,
    /// OpenRouter ASR 强制 language ISO-639-1 代码,空字符串=服务端自动判定。
    /// Whisper 对气声/耳语的自动判定不稳定(常误识韩语/日语),用户主要说中文
    /// 时强烈建议填 "zh"。
    #[serde(default = "default_openrouter_language")]
    pub openrouter_language: String,
    #[serde(default)]
    pub volc_app_key: String,
    #[serde(default)]
    pub volc_access_token: String,
    #[serde(default = "default_volc_resource_id")]
    pub volc_resource_id: String,
    // 下面 correct_* 是 0.1.1 及之前的字段，保留以便首次启动时迁移到 polish_profiles；
    // 迁移完成后新版写回的 config 里它们会是 default 值，仅作向后兼容占位。
    #[serde(default = "default_polish_mode")]
    pub correct_mode: String,
    #[serde(default = "default_correct_url")]
    pub correct_url: String,
    #[serde(default = "default_correct_model")]
    pub correct_model: String,
    #[serde(default)]
    pub correct_api_key: String,
    // 0.1.2+：多 profile 的真源
    #[serde(default)]
    pub polish_profiles: Vec<PolishProfile>,
    #[serde(default = "default_active_profile_id")]
    pub active_profile_id: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_gain")]
    pub gain: u8,
    #[serde(default)]
    pub device_name: String,
    #[serde(default = "default_correct_timeout")]
    pub correct_timeout: u32,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub hotwords: HashMap<String, String>,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,
    #[serde(default = "default_output_mode")]
    pub output_mode: String,
    #[serde(default)]
    pub push_to_talk: bool,
    /// 气声增强:录音后做 pre-emphasis + compressor + peak normalize,显著
    /// 改善气声(耳语、气声输入)的识别率。对正常说话也无副作用,默认开启。
    #[serde(default = "default_voice_enhance")]
    pub voice_enhance: bool,
    /// 本地 SenseVoice 用 fp32 完整模型(model.onnx, ~894MB)还是 int8 量化
    /// (model.int8.onnx, ~228MB)。fp32 精度更高;实测在 ARM Mac 上推理还
    /// 略快(ORT 对 fp32 走 Accelerate/NEON,int8 没占便宜),代价是内存。
    #[serde(default)]
    pub local_use_fp32_model: bool,
    /// 本地 SenseVoice 用 CoreML execution provider(macOS Apple Neural Engine
    /// 加速)。当前 sherpa-onnx 1.13.x 的 crate 预编译产物用的 ONNX Runtime
    /// 不带 CoreML EP,设了会静默 fallback 到 cpu —— 所以 UI 上暂时不开放
    /// 这个开关。等 crate 升级到带 ORT >=1.15 的预编译版本后再放出来。
    /// 想提前试的可以手动改 config.json。
    #[serde(default)]
    pub local_use_coreml: bool,
}

fn default_asr_provider() -> String {
    ASR_PROVIDER_ZHIPU.into()
}
fn default_volc_resource_id() -> String {
    "volc.seedasr.sauc.duration".into()
}
fn default_polish_mode() -> String {
    POLISH_MODE_OFF.into()
}
fn default_polish_prompt() -> String {
    DEFAULT_POLISH_PROMPT.into()
}
fn default_active_profile_id() -> String {
    DEFAULT_PROFILE_ID.into()
}
fn default_correct_url() -> String {
    "http://localhost:11434/api/generate".into()
}
fn default_correct_model() -> String {
    "qwen2.5:3b".into()
}
fn default_hotkey() -> String {
    #[cfg(target_os = "windows")]
    return "ctrl+shift+f5".into();
    "cmd+shift+f5".into()
}
fn default_gain() -> u8 {
    1
}
fn default_correct_timeout() -> u32 {
    10
}
fn default_log_level() -> String {
    "info".into()
}
fn default_vad_enabled() -> bool {
    false
}
fn default_vad_silence_ms() -> u32 {
    1500
}
/// silero-vad 概率阈值默认 0.5(0-1 范围)。
/// 老 RMS 时代用的是能量值(典型 0.005-0.05),比 silero 阈值小一个数量级。
/// 如果加载到老 config 的 < 0.1 值,migrate_vad_threshold 会重置成 0.5。
fn default_vad_threshold() -> f32 {
    0.5
}
fn default_output_mode() -> String {
    OUTPUT_MODE_INPUT.into()
}
fn default_voice_enhance() -> bool {
    true
}
fn default_openrouter_model() -> String {
    "openai/whisper-large-v3-turbo".into()
}
fn default_openrouter_language() -> String {
    "zh".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            asr_provider: default_asr_provider(),
            asr_api_key: String::new(),
            xfyun_app_id: String::new(),
            xfyun_access_key_id: String::new(),
            xfyun_access_secret: String::new(),
            openrouter_api_key: String::new(),
            openrouter_model: default_openrouter_model(),
            openrouter_language: default_openrouter_language(),
            volc_app_key: String::new(),
            volc_access_token: String::new(),
            volc_resource_id: default_volc_resource_id(),
            correct_mode: default_polish_mode(),
            correct_url: default_correct_url(),
            correct_model: default_correct_model(),
            correct_api_key: String::new(),
            polish_profiles: vec![PolishProfile::default_named(DEFAULT_PROFILE_ID, "默认")],
            active_profile_id: default_active_profile_id(),
            hotkey: default_hotkey(),
            gain: default_gain(),
            device_name: String::new(),
            correct_timeout: default_correct_timeout(),
            log_level: default_log_level(),
            hotwords: HashMap::new(),
            vad_enabled: default_vad_enabled(),
            vad_silence_ms: default_vad_silence_ms(),
            vad_threshold: default_vad_threshold(),
            output_mode: default_output_mode(),
            push_to_talk: false,
            voice_enhance: default_voice_enhance(),
            local_use_fp32_model: false,
            local_use_coreml: false,
        }
    }
}

impl Config {
    /// 从磁盘加载配置；文件不存在或解析失败时返回默认值。
    /// 加载后自动跑老字段 → polish_profiles 的迁移。
    pub fn load() -> Self {
        let path = config_path();
        let mut cfg: Self = match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        cfg.migrate_polish_profiles();
        cfg.migrate_vad_threshold();
        cfg
    }

    /// silero VAD 阈值是 0-1 概率,老 RMS 时代用的是能量值(典型 0.005-0.05)。
    /// 加载老 config 时 < 0.1 视为遗留 RMS 值,重置成 0.5(silero 默认)。
    fn migrate_vad_threshold(&mut self) {
        if self.vad_threshold > 0.0 && self.vad_threshold < 0.1 {
            tracing::info!(
                old = self.vad_threshold,
                "vad_threshold 是老 RMS 能量值,迁移到 silero 概率默认 0.5"
            );
            self.vad_threshold = 0.5;
        }
    }

    /// 首次升级到多 profile 版本时，把老的 correct_* 字段迁成一个「默认」profile。
    fn migrate_polish_profiles(&mut self) {
        if !self.polish_profiles.is_empty() {
            return; // 已有 profiles，无需迁移
        }
        let api_key =
            if self.correct_mode == POLISH_MODE_OPENROUTER && !self.openrouter_api_key.is_empty() {
                self.openrouter_api_key.clone()
            } else {
                self.correct_api_key.clone()
            };
        let profile = PolishProfile {
            id: DEFAULT_PROFILE_ID.into(),
            name: "默认".into(),
            mode: if self.correct_mode.is_empty() {
                POLISH_MODE_OFF.into()
            } else {
                self.correct_mode.clone()
            },
            url: if self.correct_url.is_empty() {
                default_correct_url()
            } else {
                self.correct_url.clone()
            },
            model: if self.correct_model.is_empty() {
                default_correct_model()
            } else {
                self.correct_model.clone()
            },
            api_key,
            prompt: DEFAULT_POLISH_PROMPT.into(),
        };
        self.polish_profiles = vec![profile];
        self.active_profile_id = DEFAULT_PROFILE_ID.into();
    }

    /// 返回当前活跃 profile；若 active_profile_id 找不到，回退到第一个。
    pub fn active_profile(&self) -> &PolishProfile {
        self.polish_profiles
            .iter()
            .find(|p| p.id == self.active_profile_id)
            .or_else(|| self.polish_profiles.first())
            .expect("polish_profiles 至少应有一个 profile")
    }

    /// 写入磁盘（pretty JSON，权限 0o600）。
    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(config_dir()).ok();
        let data = serde_json::to_string_pretty(self).context("serialize config")?;
        let path = config_path();
        fs::write(&path, &data).with_context(|| format!("write {:?}", path))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&path, perms).ok();
        }
        Ok(())
    }

    /// 纠错超时时间，0 或负值时返回 10 秒。
    pub fn correct_timeout_secs(&self) -> u64 {
        if self.correct_timeout == 0 {
            10
        } else {
            self.correct_timeout as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let c = Config::default();
        assert_eq!(c.asr_provider, "zhipu");
        assert_eq!(c.correct_mode, "off");
        assert_eq!(c.hotkey, "cmd+shift+f5");
        assert_eq!(c.gain, 1);
    }

    #[test]
    fn correct_timeout_fallback() {
        let c = Config {
            correct_timeout: 0,
            ..Config::default()
        };
        assert_eq!(c.correct_timeout_secs(), 10);
        let c30 = Config {
            correct_timeout: 30,
            ..Config::default()
        };
        assert_eq!(c30.correct_timeout_secs(), 30);
    }

    #[test]
    fn serializes_roundtrip() {
        let c = Config::default();
        let json = serde_json::to_string(&c).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.asr_provider, c.asr_provider);
        assert_eq!(parsed.hotkey, c.hotkey);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let json = r#"{"asr_provider": "volc"}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.asr_provider, "volc");
        assert_eq!(c.correct_mode, "off"); // default 填充
        assert_eq!(c.gain, 1);
    }
}
