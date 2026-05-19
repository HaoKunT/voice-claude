//! 应用配置：JSON 存盘，跨平台路径由 dirs 模块提供。
//! 对应 Go 版的 config.go。

use crate::dirs::{config_dir, config_path};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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

/// 热键触发模式。新字段,默认 "toggle"(跟当前 push_to_talk = false 行为一致)。
/// push_to_talk 字段保留但 UI 不再绑定 —— 等大家迁移完后(0.3+)删。
pub const TRIGGER_MODE_TOGGLE: &str = "toggle";
pub const TRIGGER_MODE_PTT: &str = "push_to_talk";
pub const TRIGGER_MODE_DOUBLE_TAP_HOLD: &str = "double_tap_hold";

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
    /// 识别词典:让 ASR 识别准 + 给 LLM 校正注入领域上下文。
    ///
    /// 老版本是 `HashMap<String, String>` 字符串替换映射;0.3.x 改成关键词列表,
    /// 替换路径删除,词典同时喂两条线:
    ///   ① sherpa-onnx ASR boosting(让模型识别准 —— "克劳德"不会被识别成"克老的")
    ///   ② Profile prompt 里 `{glossary}` 占位符注入(给 LLM 跨语种 / 写法映射的上下文)
    ///
    /// 老 dict 格式(`HashMap<String, String>`)在 `Config::load` 里自动迁移:
    ///   - keys + values 去重进 Vec(都进 ASR boosting 名单)
    ///   - 非平凡映射(k != v)拼成"X → Y"段落追加到默认 profile prompt 末尾
    #[serde(default)]
    pub hotwords: Vec<String>,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,
    #[serde(default = "default_output_mode")]
    pub output_mode: String,
    /// **deprecated**:UI 已不再绑定,改用 `trigger_mode`。保留只为反序列化老
    /// config 不失败。0.3+ 删。
    #[serde(default)]
    pub push_to_talk: bool,
    /// 触发方式:`toggle` / `push_to_talk` / `double_tap_hold`。
    /// 老 config 没这个字段时 serde default 为 `toggle`(跟当前默认行为一致)。
    /// 老 push_to_talk = true 用户首次进设置页 dropdown 默认 toggle,需手动选 PTT
    /// (用户已确认不做自动迁移)。
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
    /// double_tap_hold 模式下要双击的 modifier 键名。值跟 handy-keys 风格对齐
    /// (下划线分词):`right_option` / `left_option` / `right_ctrl` / ... / `fn`(macOS)。
    /// 默认 `right_option`(跟 macOS 系统 dictation "双击 Fn" 风格接近)。
    #[serde(default = "default_double_tap_modifier")]
    pub double_tap_modifier: String,
    /// 气声增强:录音后做 pre-emphasis + compressor + peak normalize,显著
    /// 改善气声(耳语、气声输入)的识别率。对正常说话也无副作用,默认开启。
    #[serde(default = "default_voice_enhance")]
    pub voice_enhance: bool,
    /// 本地 ASR 引擎选择:sense_voice / fire_red_aed / qwen3_asr。
    /// 未知字符串回退到 sense_voice。每个引擎独立模型目录,切换需对应模型已下载。
    #[serde(default = "default_local_engine")]
    pub local_engine: String,
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
fn default_local_engine() -> String {
    "sense_voice".into()
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
fn default_trigger_mode() -> String {
    TRIGGER_MODE_TOGGLE.into()
}
fn default_double_tap_modifier() -> String {
    "right_option".into()
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
            hotwords: Vec::new(),
            vad_enabled: default_vad_enabled(),
            vad_silence_ms: default_vad_silence_ms(),
            vad_threshold: default_vad_threshold(),
            output_mode: default_output_mode(),
            push_to_talk: false,
            trigger_mode: default_trigger_mode(),
            double_tap_modifier: default_double_tap_modifier(),
            voice_enhance: default_voice_enhance(),
            local_engine: default_local_engine(),
            local_use_coreml: false,
        }
    }
}

/// 从 raw JSON 里提取老 hotwords dict,原地改成 array,返回 (k, v) mapping 给
/// `Config::migrate_hotwords_mapping` 用 —— 把 mapping 拼到默认 profile prompt 末尾,
/// LLM 校正阶段还能做跨语种 / 写法映射(原 ASR 后字符串替换的等价能力)。
///
/// 老格式:`HashMap<String, String>` 字符串替换映射;
/// 新格式:`Vec<String>` 关键词列表(同一份喂 ASR boosting + LLM prompt)。
fn take_legacy_hotwords_mapping(raw: &mut serde_json::Value) -> Vec<(String, String)> {
    let Some(obj) = raw.as_object_mut() else {
        return Vec::new();
    };
    // 只处理 dict 形态;array / 缺失都跳过(已是新格式或新装用户)
    let is_dict = matches!(obj.get("hotwords"), Some(serde_json::Value::Object(_)));
    if !is_dict {
        return Vec::new();
    }
    let Some(serde_json::Value::Object(map)) = obj.remove("hotwords") else {
        return Vec::new();
    };

    use std::collections::BTreeSet;
    let mut keywords: BTreeSet<String> = BTreeSet::new();
    let mut mapping: Vec<(String, String)> = Vec::new();
    for (k, v) in map.iter() {
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        keywords.insert(k.to_string());
        let Some(v_str) = v.as_str() else { continue };
        let v_str = v_str.trim();
        if v_str.is_empty() {
            continue;
        }
        keywords.insert(v_str.to_string());
        if k != v_str {
            mapping.push((k.to_string(), v_str.to_string()));
        }
    }
    let arr: Vec<serde_json::Value> = keywords
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    obj.insert("hotwords".into(), serde_json::Value::Array(arr));

    mapping.sort();
    mapping
}

impl Config {
    /// 从磁盘加载配置；文件不存在或解析失败时返回默认值。
    /// 加载后自动跑老字段 → polish_profiles / hotwords 的迁移。
    pub fn load() -> Self {
        let path = config_path();
        // 两步反序列化:先到 Value,提取并迁移老 hotwords dict 后再 → Config。        // 这样 deserializer 不用自定义,迁移逻辑(把 mapping 拼到 default profile prompt)
        // 也能拿到完整 Config 状态。
        let raw_data = match fs::read_to_string(&path) {
            Ok(data) => data,
            Err(_) => return Self::default(),
        };
        let mut raw_value: serde_json::Value = match serde_json::from_str(&raw_data) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = ?e, "config.json 解析失败,使用默认值");
                return Self::default();
            }
        };
        let legacy_mapping = take_legacy_hotwords_mapping(&mut raw_value);
        let mut cfg: Self = serde_json::from_value(raw_value).unwrap_or_default();
        cfg.migrate_polish_profiles();
        cfg.migrate_vad_threshold();
        cfg.migrate_hotwords_mapping(legacy_mapping);
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

    /// 老 hotwords dict 的 k→v mapping 迁移到默认 profile prompt 末尾,
    /// 让 LLM 能在校正阶段做跨语种映射(原字符串替换路径已删除)。
    /// 幂等:prompt 已经含"以下词语映射"段落时不重复追加。
    fn migrate_hotwords_mapping(&mut self, mapping: Vec<(String, String)>) {
        if mapping.is_empty() {
            return;
        }
        let Some(profile) = self
            .polish_profiles
            .iter_mut()
            .find(|p| p.id == DEFAULT_PROFILE_ID)
        else {
            return;
        };
        if profile.prompt.contains("以下词语映射") {
            return; // 已经迁移过
        }
        let mut block = String::from("\n\n以下词语映射(右侧为正式写法):\n");
        for (k, v) in &mapping {
            block.push_str(&format!("- {} → {}\n", k, v));
        }
        profile.prompt.push_str(&block);
        tracing::info!(
            count = mapping.len(),
            "迁移老 hotwords k→v mapping 到默认 profile prompt"
        );
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

    /// 写入 history / 状态统计用的 provider id。
    /// 云端后端直接返回 cfg.asr_provider;选了 "local" 时展开成具体本地引擎 id
    /// (sense_voice / fire_red_aed / fire_red_ctc2 / qwen3_asr),让 4 个本地
    /// 引擎在统计里分开,而不是混作一坨 "local"。
    pub fn provider_id_for_stats(&self) -> String {
        if self.asr_provider == ASR_PROVIDER_LOCAL {
            if self.local_engine.is_empty() {
                "sense_voice".to_string()
            } else {
                self.local_engine.clone()
            }
        } else {
            self.asr_provider.clone()
        }
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

    #[test]
    fn take_legacy_hotwords_mapping_converts_dict_to_array() {
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{"hotwords": {"克劳德": "Claude", "voice-claude": "voice-claude", "": "X", "FireRed": ""}}"#,
        )
        .unwrap();
        let mapping = take_legacy_hotwords_mapping(&mut v);
        // 非平凡 mapping(k != v 且都非空)只有 "克劳德 → Claude"
        assert_eq!(mapping, vec![("克劳德".into(), "Claude".into())]);
        // hotwords 已就地改成 array,内容是去重 + 去空(空 key / 空 value 跳过)
        let arr = v["hotwords"].as_array().unwrap();
        let words: Vec<&str> = arr.iter().filter_map(|x| x.as_str()).collect();
        assert!(words.contains(&"克劳德"));
        assert!(words.contains(&"Claude"));
        assert!(words.contains(&"voice-claude"));
        assert!(words.contains(&"FireRed")); // 即便 value 为空,key 仍进列表
        assert!(!words.contains(&"")); // 空 key 跳过
    }

    #[test]
    fn take_legacy_hotwords_mapping_skips_array_format() {
        // 已是 array 格式不再迁移
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"hotwords": ["Claude", "voice-claude"]}"#).unwrap();
        let mapping = take_legacy_hotwords_mapping(&mut v);
        assert!(mapping.is_empty());
        // 数组保持不动
        let arr = v["hotwords"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn migrate_hotwords_mapping_appends_to_default_profile() {
        let mut c = Config::default();
        c.migrate_hotwords_mapping(vec![
            ("克劳德".into(), "Claude".into()),
            ("吉他布".into(), "GitHub".into()),
        ]);
        let p = c
            .polish_profiles
            .iter()
            .find(|p| p.id == DEFAULT_PROFILE_ID)
            .unwrap();
        assert!(p.prompt.contains("以下词语映射"));
        assert!(p.prompt.contains("克劳德 → Claude"));
        assert!(p.prompt.contains("吉他布 → GitHub"));

        // 幂等:再调一次不重复追加
        let prompt_before = p.prompt.clone();
        c.migrate_hotwords_mapping(vec![("X".into(), "Y".into())]);
        let p2 = c
            .polish_profiles
            .iter()
            .find(|p| p.id == DEFAULT_PROFILE_ID)
            .unwrap();
        assert_eq!(p2.prompt, prompt_before);
    }
}
