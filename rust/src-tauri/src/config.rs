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
pub const DEFAULT_BACKEND_ID: &str = "default";
pub const DEFAULT_POLISH_PROMPT: &str =
    "你是一个语音识别润色助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
如果原文没有明显错误，直接返回原文。
只输出润色后的文本，不要解释。

原文：{text}";

/// 一个 LLM 后端连接：mode + url + model + api_key,跟 prompt / 业务逻辑解耦。
///
/// 多个 PolishProfile 可以共享同一个 backend(只换 prompt);hotword 自动生成
/// 阶段也复用 active profile 的 backend。0.4 起从 PolishProfile 抽出来,老配置
/// 在 `Config::load` 阶段自动迁移(profile 的连接字段去重抽成 backend)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackend {
    pub id: String,
    pub name: String,
    #[serde(default = "default_backend_mode")]
    pub mode: String,
    #[serde(default = "default_correct_url")]
    pub url: String,
    #[serde(default = "default_correct_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
}

impl LlmBackend {
    pub fn default_named(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            mode: default_backend_mode(),
            url: default_correct_url(),
            model: default_correct_model(),
            api_key: String::new(),
        }
    }

    /// 0.4 之后 backend.mode 不再含 "off" —— "关掉润色"由 profile.backend_id == ""
    /// 表达。这里仍兜底跳过 "off" / 空 mode,防御用户手改 config.json 或老迁移漏跑
    /// 的场景,避免路由到 `llm_client::call` 后 `bail!`。
    pub fn is_active(&self) -> bool {
        !(self.mode == POLISH_MODE_OFF || self.mode.is_empty())
    }
}

/// 单个 AI 润色 profile：prompt 模板 + 引用一个 LLM 后端。
///
/// `template_id` = `Some(id)` 表示这个 profile 是「内置模板」类型,prompt
/// 文本走 `profile_templates::effective_prompt()` 从 Rust registry 实时读 ——
/// 升级新版本应用 → 老用户的 builtin profile 自动用上新 prompt,不用动 config。
/// 用户想改:前端"复制为自定义版本" → 清空 template_id 并把当前 prompt 物化到
/// `prompt` 字段,从此 profile 变 custom,内容不再随版本变化。
///
/// `backend_id` 引用 `Config.llm_backends` 里的某条:
///   - 非空且能在 `llm_backends` 里找到 → 用此 backend 跑润色
///   - 空字符串 `""` = 用户主动选了"关闭",ASR 原文直出(profile/prompt 仍保留,
///     切回有效 backend 即可恢复)
///   - 非空但找不到对应 backend → 异常态(用户手改 config 删错了),运行时按"关闭"
///     处理,不报错
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub backend_id: String,
    #[serde(default = "default_polish_prompt")]
    pub prompt: String,
    /// 内置模板 id;`None` = 自定义 profile,prompt 字段用户自由编辑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
}

impl PolishProfile {
    pub fn default_named(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            backend_id: DEFAULT_BACKEND_ID.into(),
            prompt: default_polish_prompt(),
            template_id: None,
        }
    }

    pub fn is_builtin(&self) -> bool {
        self.template_id.is_some()
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
    /// 0.4+:LLM 后端连接配置(从 PolishProfile 抽出来,多个 profile 共享)。
    /// 老 config(profile 自带 mode/url/model/api_key)在 `Config::load` 阶段
    /// 自动迁移成 backend。
    #[serde(default)]
    pub llm_backends: Vec<LlmBackend>,
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
/// 新建 LlmBackend 时默认的 mode。0.4 起不再用 "off"(profile 用 backend_id="" 表
/// 达"关闭"),默认给 ollama —— 用户大概率从这里入门(本地零成本),改成云端再
/// 填 url/api_key。
fn default_backend_mode() -> String {
    POLISH_MODE_OLLAMA.into()
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
            llm_backends: vec![LlmBackend::default_named(DEFAULT_BACKEND_ID, "默认后端")],
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

/// 从 raw JSON 里把老 PolishProfile 的 mode/url/model/api_key 抽成独立 LlmBackend。
///
/// 触发条件:`raw.llm_backends` 为空(或缺失) **且** 至少有一个 profile 带这些老
/// 字段。按 `(mode, url, model, api_key)` 元组去重,生成 backend id。每个 profile
/// 写回 `backend_id`,删除老的 mode/url/model/api_key 字段(让 `Config` 反序列化
/// 时不报警)。
fn migrate_legacy_polish_backends(raw: &mut serde_json::Value) {
    let Some(obj) = raw.as_object_mut() else {
        return;
    };
    // 已有 llm_backends 非空 → 已是新格式,不动
    if let Some(serde_json::Value::Array(arr)) = obj.get("llm_backends") {
        if !arr.is_empty() {
            return;
        }
    }
    let Some(serde_json::Value::Array(profiles)) = obj.get_mut("polish_profiles") else {
        return;
    };

    use std::collections::BTreeMap;
    // 去重表:(mode, url, model, api_key) → backend_id
    let mut dedup: BTreeMap<(String, String, String, String), String> = BTreeMap::new();
    let mut backends: Vec<serde_json::Value> = Vec::new();
    let mut next_idx = 0usize;

    fn take_str(profile: &mut serde_json::Map<String, serde_json::Value>, key: &str) -> String {
        profile
            .remove(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    for profile in profiles.iter_mut() {
        let Some(profile_obj) = profile.as_object_mut() else {
            continue;
        };
        // profile 已经是新格式(有 backend_id 且无 mode 等)→ 跳过
        let has_legacy = profile_obj.contains_key("mode")
            || profile_obj.contains_key("url")
            || profile_obj.contains_key("model")
            || profile_obj.contains_key("api_key");
        if !has_legacy {
            continue;
        }
        let mode = take_str(profile_obj, "mode");
        let url = take_str(profile_obj, "url");
        let model = take_str(profile_obj, "model");
        let api_key = take_str(profile_obj, "api_key");
        let mode_eff = if mode.is_empty() {
            POLISH_MODE_OFF.to_string()
        } else {
            mode
        };

        let key = (mode_eff.clone(), url.clone(), model.clone(), api_key.clone());
        let backend_id = dedup
            .entry(key)
            .or_insert_with(|| {
                let id = if next_idx == 0 {
                    DEFAULT_BACKEND_ID.to_string()
                } else {
                    format!("backend_{}", next_idx)
                };
                next_idx += 1;
                let name = derive_backend_name(&mode_eff, &model, &url);
                let backend = serde_json::json!({
                    "id": id,
                    "name": name,
                    "mode": mode_eff,
                    "url": url,
                    "model": model,
                    "api_key": api_key,
                });
                backends.push(backend);
                id
            })
            .clone();
        profile_obj.insert("backend_id".into(), serde_json::Value::String(backend_id));
    }

    if !backends.is_empty() {
        obj.insert("llm_backends".into(), serde_json::Value::Array(backends));
    }
}

/// 给迁移出来的 backend 取个能让用户区分的默认名(mode + model 主要信息)。
fn derive_backend_name(mode: &str, model: &str, url: &str) -> String {
    if !model.is_empty() {
        return format!("{} ({})", mode, model);
    }
    if !url.is_empty() {
        return format!("{} ({})", mode, url);
    }
    mode.to_string()
}

/// 0.4 起 backend.mode 不再有 "off"。"关闭润色"由 profile.backend_id == ""
/// 表达 —— 每个 profile 自己决定要不要润色,跟 backend 配置解耦。
///
/// 老 config 加载时:
///   - 找出所有 mode == "off" / 空 mode 的 backend id
///   - 把所有引用这些 id 的 profile 的 backend_id 清空("")—— 用户切到这些 profile
///     就等于"关闭润色",ASR 原文直出
///   - 把这些 backend 的 mode 重写成 ollama + 默认 url/model,UI dropdown 不再
///     需要"off"选项,backend 永远代表"配了哪种连接"
///
/// 在 `migrate_legacy_polish_backends`(profile→backend 拆分)之后执行,
/// 让两个迁移可以串起来:profile 自带 mode=off → 拆出 off backend → 这里再
/// 把这些 backend 的 mode 改成 ollama,引用它的 profile backend_id 清空。
fn migrate_polish_off_to_empty_backend_id(raw: &mut serde_json::Value) {
    let Some(obj) = raw.as_object_mut() else {
        return;
    };

    // Step 1:扫一遍 backends,收集所有 mode = "off" / 空 mode 的 id
    use std::collections::BTreeSet;
    let mut off_ids: BTreeSet<String> = BTreeSet::new();
    if let Some(serde_json::Value::Array(backends)) = obj.get("llm_backends") {
        for b in backends {
            let Some(id) = b.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let mode = b.get("mode").and_then(|v| v.as_str()).unwrap_or("");
            if mode == POLISH_MODE_OFF || mode.is_empty() {
                off_ids.insert(id.to_string());
            }
        }
    }

    // Step 2:profile.backend_id 指向 off backend → 清空,表示"关闭"
    if !off_ids.is_empty() {
        if let Some(serde_json::Value::Array(profiles)) = obj.get_mut("polish_profiles") {
            for p in profiles.iter_mut() {
                let Some(map) = p.as_object_mut() else {
                    continue;
                };
                let bid = map
                    .get("backend_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if off_ids.contains(bid) {
                    map.insert(
                        "backend_id".into(),
                        serde_json::Value::String(String::new()),
                    );
                }
            }
        }
    }

    // Step 3:重写所有 mode = "off" 的 backend(无论 Step 2 是否清了 profile),
    // 让 UI BackendCard 的 mode dropdown 不再需要 "off" 选项。
    // url / model 为空时填默认值,用户接着改更顺手。
    if let Some(serde_json::Value::Array(backends)) = obj.get_mut("llm_backends") {
        for b in backends.iter_mut() {
            let Some(map) = b.as_object_mut() else {
                continue;
            };
            let mode_is_off = map
                .get("mode")
                .and_then(|v| v.as_str())
                .map(|s| s == POLISH_MODE_OFF || s.is_empty())
                .unwrap_or(true);
            if !mode_is_off {
                continue;
            }
            map.insert(
                "mode".into(),
                serde_json::Value::String(POLISH_MODE_OLLAMA.into()),
            );
            if map
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                map.insert(
                    "url".into(),
                    serde_json::Value::String(default_correct_url()),
                );
            }
            if map
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                map.insert(
                    "model".into(),
                    serde_json::Value::String(default_correct_model()),
                );
            }
        }
    }
}


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
        migrate_legacy_polish_backends(&mut raw_value);
        migrate_polish_off_to_empty_backend_id(&mut raw_value);
        let legacy_mapping = take_legacy_hotwords_mapping(&mut raw_value);
        let mut cfg: Self = serde_json::from_value(raw_value).unwrap_or_default();
        cfg.migrate_polish_profiles();
        cfg.migrate_vad_threshold();
        cfg.migrate_hotwords_mapping(legacy_mapping);
        cfg.migrate_builtin_template_ids();
        cfg.ensure_default_backend();
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

    /// 首次升级到多 profile 版本时，把老的 correct_* 字段迁成一个「默认」profile +
    /// 一个「默认」backend。该函数在 `migrate_legacy_polish_backends` 之后执行,
    /// 覆盖的是 0.1.x ↓ 这种连 polish_profiles 都没有的更老格式。
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
        // 0.4 起 backend.mode 不再有 "off"。老 correct_mode = "off" 表示用户当时关着润色 →
        // profile.backend_id 设成 "" 表"关闭",backend.mode 用默认 ollama 占位
        // (用户开了之后还得自己挑 mode,但起码 dropdown 不报错)。
        let legacy_off = self.correct_mode == POLISH_MODE_OFF || self.correct_mode.is_empty();
        let mode = if legacy_off {
            POLISH_MODE_OLLAMA.to_string()
        } else {
            self.correct_mode.clone()
        };
        let url = if self.correct_url.is_empty() {
            default_correct_url()
        } else {
            self.correct_url.clone()
        };
        let model = if self.correct_model.is_empty() {
            default_correct_model()
        } else {
            self.correct_model.clone()
        };

        // 老格式没 backend → 创建一个,id = DEFAULT_BACKEND_ID
        if self.llm_backends.is_empty() {
            self.llm_backends = vec![LlmBackend {
                id: DEFAULT_BACKEND_ID.into(),
                name: derive_backend_name(&mode, &model, &url),
                mode,
                url,
                model,
                api_key,
            }];
        }
        let profile = PolishProfile {
            id: DEFAULT_PROFILE_ID.into(),
            name: "默认".into(),
            // legacy_off → 用空 backend_id 表"关闭";否则指向新建的默认 backend
            backend_id: if legacy_off {
                String::new()
            } else {
                DEFAULT_BACKEND_ID.into()
            },
            prompt: DEFAULT_POLISH_PROMPT.into(),
            template_id: None,
        };
        self.polish_profiles = vec![profile];
        self.active_profile_id = DEFAULT_PROFILE_ID.into();
    }

    /// 安全网:`llm_backends` 为空(用户手改 config.json 删光了)时塞一个默认占位
    /// backend,避免运行时找不到 backend_id panic。
    ///
    /// **不**回填空 backend_id —— 0.4 起 `backend_id == ""` 是 profile "关闭润色"
    /// 的合法 sentinel,不能当作未配置回填掉。只修非空但找不到对应 backend 的 ID
    /// (用户手改删错了引用)→ 视为异常态,清空成"关闭",运行时不报错。
    fn ensure_default_backend(&mut self) {
        if self.llm_backends.is_empty() {
            self.llm_backends = vec![LlmBackend::default_named(DEFAULT_BACKEND_ID, "默认后端")];
        }
        for profile in self.polish_profiles.iter_mut() {
            if profile.backend_id.is_empty() {
                continue; // "" 是合法 "关闭" sentinel,保留
            }
            let exists = self
                .llm_backends
                .iter()
                .any(|b| b.id == profile.backend_id);
            if !exists {
                tracing::warn!(
                    profile = %profile.id,
                    bad_backend_id = %profile.backend_id,
                    "profile 引用了不存在的 backend,清空 backend_id 当作'关闭'处理"
                );
                profile.backend_id.clear();
            }
        }
    }

    /// 把老 profile 里跟内置模板(当前/历史版本)文本对得上的 prompt 自动转成 builtin:
    /// 设置 `template_id` + 清空 `prompt` 字段。这样升级模板内容老用户能立刻吃上。
    /// 用户手动魔改过的 prompt(normalize 后跟任何已知模板都不等)保留为 custom,不动。
    fn migrate_builtin_template_ids(&mut self) {
        for profile in self.polish_profiles.iter_mut() {
            if profile.template_id.is_some() {
                continue; // 已经是 builtin
            }
            if let Some(id) = crate::profile_templates::detect_template_id(&profile.prompt) {
                tracing::info!(
                    profile = %profile.id,
                    template = %id,
                    "把 profile 的 prompt 迁移到内置模板"
                );
                profile.template_id = Some(id.into());
                profile.prompt.clear();
            }
        }
    }

    /// 返回当前活跃 profile；若 active_profile_id 找不到，回退到第一个。
    pub fn active_profile(&self) -> &PolishProfile {
        self.polish_profiles
            .iter()
            .find(|p| p.id == self.active_profile_id)
            .or_else(|| self.polish_profiles.first())
            .expect("polish_profiles 至少应有一个 profile")
    }

    /// 按 id 查 backend;找不到返回 None。
    pub fn backend_by_id(&self, id: &str) -> Option<&LlmBackend> {
        self.llm_backends.iter().find(|b| b.id == id)
    }

    /// 当前 active profile 引用的 backend(找不到 → 第一个 backend → None)。
    pub fn active_backend(&self) -> Option<&LlmBackend> {
        let profile = self.active_profile();
        self.backend_by_id(&profile.backend_id)
            .or_else(|| self.llm_backends.first())
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

    #[test]
    fn migrate_legacy_polish_backends_extracts_and_dedupes() {
        // 老 cfg:两个 profile 跑同一个 backend(mode/url/model/api_key 完全一样)→ 合一份
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "polish_profiles": [
                {"id":"a","name":"A","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-1","prompt":"X"},
                {"id":"b","name":"B","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-1","prompt":"Y"}
              ]
            }"#,
        )
        .unwrap();
        migrate_legacy_polish_backends(&mut v);
        let backends = v["llm_backends"].as_array().unwrap();
        assert_eq!(backends.len(), 1, "完全相同的 backend 应去重为一份");
        let bid = backends[0]["id"].as_str().unwrap();
        let profiles = v["polish_profiles"].as_array().unwrap();
        for p in profiles {
            assert_eq!(p["backend_id"].as_str().unwrap(), bid);
            // 老字段已剥离
            assert!(p.get("mode").is_none());
            assert!(p.get("url").is_none());
            assert!(p.get("model").is_none());
            assert!(p.get("api_key").is_none());
            // prompt 还在
            assert!(p["prompt"].is_string());
        }
    }

    #[test]
    fn migrate_legacy_polish_backends_keeps_distinct_profiles_separate() {
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "polish_profiles": [
                {"id":"a","name":"A","mode":"ollama","url":"http://localhost:11434/api/generate","model":"qwen2.5","api_key":"","prompt":""},
                {"id":"b","name":"B","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-2","prompt":""}
              ]
            }"#,
        )
        .unwrap();
        migrate_legacy_polish_backends(&mut v);
        let backends = v["llm_backends"].as_array().unwrap();
        assert_eq!(backends.len(), 2, "不同 mode/url/model 应保留两份 backend");
        let profiles = v["polish_profiles"].as_array().unwrap();
        let bid_a = profiles[0]["backend_id"].as_str().unwrap();
        let bid_b = profiles[1]["backend_id"].as_str().unwrap();
        assert_ne!(bid_a, bid_b, "两个 profile 应指向不同 backend");
    }

    #[test]
    fn migrate_legacy_polish_backends_skips_when_already_migrated() {
        // 已经是新格式(llm_backends 非空)→ 不动
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "llm_backends":[{"id":"x","name":"已存在","mode":"off","url":"","model":"","api_key":""}],
              "polish_profiles":[{"id":"a","name":"A","backend_id":"x","prompt":""}]
            }"#,
        )
        .unwrap();
        let before = v.clone();
        migrate_legacy_polish_backends(&mut v);
        assert_eq!(v, before, "已迁移 cfg 不应被再次处理");
    }

    #[test]
    fn migrate_legacy_polish_backends_handles_empty_profiles() {
        // 空 profiles → llm_backends 不写入(由后续 ensure_default_backend 兜底)
        let mut v: serde_json::Value = serde_json::from_str(r#"{"polish_profiles": []}"#).unwrap();
        migrate_legacy_polish_backends(&mut v);
        assert!(
            v.get("llm_backends").is_none(),
            "无可迁移内容时不应写入 llm_backends"
        );
    }

    #[test]
    fn full_load_migrates_legacy_cfg_end_to_end() {
        let raw = r#"{
          "polish_profiles":[
            {"id":"a","name":"快速","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-A","prompt":"P1"},
            {"id":"b","name":"精修","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-A","prompt":"P2"}
          ],
          "active_profile_id":"a"
        }"#;
        let mut raw_value: serde_json::Value = serde_json::from_str(raw).unwrap();
        // 跑跟 Config::load 一样的两步
        migrate_legacy_polish_backends(&mut raw_value);
        let mut cfg: Config = serde_json::from_value(raw_value).unwrap();
        cfg.ensure_default_backend();
        assert_eq!(cfg.llm_backends.len(), 1, "同样的 backend 配置应只有一份");
        assert!(cfg.polish_profiles.iter().all(|p| !p.backend_id.is_empty()));
        let bid = &cfg.llm_backends[0].id;
        assert!(cfg.polish_profiles.iter().all(|p| p.backend_id == *bid));
        // active backend 应可解析
        assert!(cfg.active_backend().is_some());
    }

    #[test]
    fn migrate_polish_off_clears_backend_id_on_referencing_profile() {
        // 老 cfg: profile 引用的 backend mode = "off" → profile.backend_id 清空,
        // backend.mode 改写成 ollama + 默认 url/model
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "llm_backends":[{"id":"x","name":"老配置","mode":"off","url":"","model":"","api_key":""}],
              "polish_profiles":[{"id":"a","name":"A","backend_id":"x","prompt":""}],
              "active_profile_id":"a"
            }"#,
        )
        .unwrap();
        migrate_polish_off_to_empty_backend_id(&mut v);
        assert_eq!(
            v["polish_profiles"][0]["backend_id"], "",
            "引用 off backend 的 profile 应清空 backend_id"
        );
        let backends = v["llm_backends"].as_array().unwrap();
        assert_eq!(backends[0]["mode"], "ollama");
        assert!(!backends[0]["url"].as_str().unwrap().is_empty());
        assert!(!backends[0]["model"].as_str().unwrap().is_empty());
    }

    #[test]
    fn migrate_polish_off_keeps_profile_pointing_at_active_backend() {
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "llm_backends":[{"id":"x","name":"已配","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-1"}],
              "polish_profiles":[{"id":"a","name":"A","backend_id":"x","prompt":""}],
              "active_profile_id":"a"
            }"#,
        )
        .unwrap();
        migrate_polish_off_to_empty_backend_id(&mut v);
        assert_eq!(
            v["polish_profiles"][0]["backend_id"], "x",
            "引用非 off backend 的 profile 不应被清空"
        );
        // 非 off backend 不被改写
        assert_eq!(v["llm_backends"][0]["mode"], "openrouter");
    }

    #[test]
    fn migrate_polish_off_only_clears_profiles_pointing_at_off_backend() {
        // 多 profile:只有引用 off backend 的那个被清空,引用非 off 的不变
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{
              "llm_backends":[
                {"id":"x","name":"老","mode":"off","url":"","model":"","api_key":""},
                {"id":"y","name":"新","mode":"openrouter","url":"https://openrouter.ai/api/v1","model":"sonnet","api_key":"sk-2"}
              ],
              "polish_profiles":[
                {"id":"a","name":"A","backend_id":"x","prompt":""},
                {"id":"b","name":"B","backend_id":"y","prompt":""}
              ],
              "active_profile_id":"a"
            }"#,
        )
        .unwrap();
        migrate_polish_off_to_empty_backend_id(&mut v);
        assert_eq!(v["polish_profiles"][0]["backend_id"], "");
        assert_eq!(v["polish_profiles"][1]["backend_id"], "y");
        // 两个 backend 的 mode:off 被改写成 ollama,openrouter 保留
        assert_eq!(v["llm_backends"][0]["mode"], "ollama");
        assert_eq!(v["llm_backends"][1]["mode"], "openrouter");
    }

    #[test]
    fn migrate_polish_off_full_load_path_legacy_off_backend() {
        // 完整 Config::load 路径:profile 自带 mode=off → 拆 backend → backend_id 清空
        let raw = r#"{
          "polish_profiles":[
            {"id":"a","name":"A","mode":"off","url":"","model":"","api_key":"","prompt":"P"}
          ],
          "active_profile_id":"a"
        }"#;
        let mut raw_value: serde_json::Value = serde_json::from_str(raw).unwrap();
        migrate_legacy_polish_backends(&mut raw_value);
        migrate_polish_off_to_empty_backend_id(&mut raw_value);
        let mut cfg: Config = serde_json::from_value(raw_value).unwrap();
        cfg.ensure_default_backend();
        assert_eq!(cfg.polish_profiles[0].backend_id, "", "应表'关闭'");
        // backend mode 已被重写成非 off
        assert_eq!(cfg.llm_backends.len(), 1);
        assert_ne!(cfg.llm_backends[0].mode, POLISH_MODE_OFF);
        // active backend 解析得为空(profile 关闭)
        assert!(cfg.active_profile().backend_id.is_empty());
    }

    #[test]
    fn ensure_default_backend_preserves_empty_backend_id_sentinel() {
        // 用户主动选了"关闭"(backend_id="") → ensure_default_backend 不应回填
        let mut cfg = Config {
            polish_profiles: vec![PolishProfile {
                id: "a".into(),
                name: "A".into(),
                backend_id: String::new(), // 空 = "关闭"
                prompt: String::new(),
                template_id: None,
            }],
            ..Config::default()
        };
        cfg.ensure_default_backend();
        assert_eq!(cfg.polish_profiles[0].backend_id, "", "空 backend_id 应保留");
    }

    #[test]
    fn ensure_default_backend_clears_dangling_backend_id() {
        // profile 引用了不存在的 backend → 当作"关闭"清空(异常态兜底)
        let mut cfg = Config {
            polish_profiles: vec![PolishProfile {
                id: "a".into(),
                name: "A".into(),
                backend_id: "doesnt_exist".into(),
                prompt: String::new(),
                template_id: None,
            }],
            ..Config::default()
        };
        cfg.ensure_default_backend();
        assert_eq!(cfg.polish_profiles[0].backend_id, "");
    }
}
