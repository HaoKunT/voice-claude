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

pub const CORRECT_MODE_OFF: &str = "off";
pub const CORRECT_MODE_OLLAMA: &str = "ollama";
pub const CORRECT_MODE_OPENROUTER: &str = "openrouter";
pub const CORRECT_MODE_CLOUD: &str = "cloud";

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
    #[serde(default)]
    pub volc_app_key: String,
    #[serde(default)]
    pub volc_access_token: String,
    #[serde(default = "default_volc_resource_id")]
    pub volc_resource_id: String,
    #[serde(default = "default_correct_mode")]
    pub correct_mode: String,
    #[serde(default = "default_correct_url")]
    pub correct_url: String,
    #[serde(default = "default_correct_model")]
    pub correct_model: String,
    #[serde(default)]
    pub correct_api_key: String,
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
}

fn default_asr_provider() -> String {
    ASR_PROVIDER_ZHIPU.into()
}
fn default_volc_resource_id() -> String {
    "volc.seedasr.sauc.duration".into()
}
fn default_correct_mode() -> String {
    CORRECT_MODE_OFF.into()
}
fn default_correct_url() -> String {
    "http://localhost:11434/api/generate".into()
}
fn default_correct_model() -> String {
    "qwen2.5:3b".into()
}
fn default_hotkey() -> String {
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
    true
}
fn default_vad_silence_ms() -> u32 {
    1500
}
fn default_vad_threshold() -> f32 {
    0.015
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
            volc_app_key: String::new(),
            volc_access_token: String::new(),
            volc_resource_id: default_volc_resource_id(),
            correct_mode: default_correct_mode(),
            correct_url: default_correct_url(),
            correct_model: default_correct_model(),
            correct_api_key: String::new(),
            hotkey: default_hotkey(),
            gain: default_gain(),
            device_name: String::new(),
            correct_timeout: default_correct_timeout(),
            log_level: default_log_level(),
            hotwords: HashMap::new(),
            vad_enabled: default_vad_enabled(),
            vad_silence_ms: default_vad_silence_ms(),
            vad_threshold: default_vad_threshold(),
        }
    }
}

impl Config {
    /// 从磁盘加载配置；文件不存在或解析失败时返回默认值。
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
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
