// Tauri IPC 调用封装。
import { invoke } from "@tauri-apps/api/core";

export interface LlmBackend {
  id: string;
  name: string;
  mode: string;
  url: string;
  model: string;
  api_key: string;
}

export interface PolishProfile {
  id: string;
  name: string;
  /** 引用的 LlmBackend.id;profile 自身不再持 mode/url/model/api_key,
   *  这些跟 hotword 自动生成共用一份 backend 配置。
   *  空字符串 `""` 表示这个 profile 不要润色 —— ASR 原文直出,prompt / 历史 backend
   *  绑定全保留,切到有效 backend 立即恢复。每个 profile 独立选,跟其他 profile 解耦。 */
  backend_id: string;
  prompt: string;
  /** 内置模板 id;有值表示这个 profile 是「内置模板」类型,prompt 文本由后端从
   *  registry 动态读出(升级新版应用模板内容自动同步,用户改不了)。`undefined` =
   *  自定义 profile,prompt 字段是真源。前端"复制为自定义版本"会清掉这个字段。 */
  template_id?: string;
}

export interface Config {
  asr_provider: string;
  asr_api_key: string;
  xfyun_app_id: string;
  xfyun_access_key_id: string;
  xfyun_access_key_secret: string;
  openrouter_api_key: string;
  openrouter_model: string;
  openrouter_language: string;
  /** MiMo ASR(`mimo-v2.5-asr`)API key */
  mimo_api_key: string;
  /** `public`(默认,官方 api.xiaomimimo.com)/ `custom`(自部署,自填 base_url + model)。
   *  MiMo-V2.5-ASR 权重 HuggingFace 开源,可用 vLLM / sglang 等自托管 */
  mimo_endpoint: string;
  /** custom 模式下自部署 endpoint 的 chat completions URL */
  mimo_base_url: string;
  /** custom 模式下自部署的 model id */
  mimo_model: string;
  /** auto / zh / en */
  mimo_language: string;
  volc_app_key: string;
  volc_access_token: string;
  volc_resource_id: string;
  // 老的 correct_* 字段：后端迁移到 polish_profiles 后不再由前端直接编辑，
  // 但保留在接口里以便 save 时原样回写（向后兼容）
  correct_mode: string;
  correct_url: string;
  correct_model: string;
  correct_api_key: string;
  polish_profiles: PolishProfile[];
  /** 共享的 LLM 后端连接池。每个 profile 通过 backend_id 引用其中一项;
   *  hotword 自动生成也复用 active profile 的 backend。 */
  llm_backends: LlmBackend[];
  active_profile_id: string;
  hotkey: string;
  gain: number;
  device_name: string;
  correct_timeout: number;
  log_level: string;
  /** 识别词典:0.3.x 起改成关键词列表(老版本是 key→value 字符串替换映射,后端自动迁移)。
   *  同一份列表喂两条线:① ASR boosting ② LLM 校正 prompt 的 {glossary} 注入 */
  hotwords: string[];
  vad_enabled: boolean;
  vad_silence_ms: number;
  vad_threshold: number;
  output_mode: string;
  /** @deprecated UI 不再绑定,改用 trigger_mode。0.3+ 删 */
  push_to_talk: boolean;
  /** 触发方式: toggle / push_to_talk / double_tap_hold */
  trigger_mode: string;
  /** double_tap_hold 模式下要双击的 modifier(handy-keys 风格): right_option / left_ctrl / ... */
  double_tap_modifier: string;
  voice_enhance: boolean;
  /** UI 暂未暴露开关 —— sherpa-onnx crate 升级前手动改 config.json 才能开 */
  local_use_coreml: boolean;
  /** 本地 ASR 引擎:sense_voice / fire_red_aed / fire_red_ctc2 / qwen3_asr */
  local_engine: string;
}

export interface DeviceInfo {
  name: string;
}

export interface HistoryEntry {
  id: number;
  created_at: number;
  raw_text: string;
  corrected_text: string;
  asr_provider: string;
  duration_ms: number;
}

export interface HistoryStats {
  total_count: number;
  total_duration_ms: number;
  total_chars: number;
  avg_chars_per_minute: number;
  saved_minutes: number;
  first_created_at: number | null;
}

export interface LatencyRow {
  key: string;
  count: number;
  avg_ms: number;
  p99_ms: number;
  /** 仅润色行用:该 model 用过的 provider 集合(ollama / openrouter / api.groq.com 等)。ASR 行为空 */
  providers: string[];
  /** 仅润色行用:本组里触发超时的次数。ASR 行为 0 */
  timeout_count: number;
}

export interface LatencyWindow {
  asr: LatencyRow[];
  polish: LatencyRow[];
}

export interface LatencyStats {
  all_time: LatencyWindow;
  last_24h: LatencyWindow;
  last_7d: LatencyWindow;
}

export interface LocalEngineInfo {
  id: string;
  label: string;
  description: string;
  url: string;
  sha256: string;
  model_dir: string;
  available: boolean;
  size_mb: number;
}

export interface DownloadProgress {
  downloaded: number;
  total: number;
  /** 当前下载的引擎 id;前端按这个匹配自己 panel 的进度 */
  engine_id: string;
}

export interface PunctModelInfo {
  label: string;
  description: string;
  url: string;
  sha256: string;
  model_dir: string;
  available: boolean;
  size_mb: number;
}

export interface PunctDownloadProgress {
  downloaded: number;
  total: number;
}

export interface BenchResult {
  provider_id: string;
  text: string;
  error: string | null;
  ms: number;
}

export interface HotwordSourceInfo {
  id: string;
  label: string;
  available: boolean;
}

export interface HotwordCandidate {
  word: string;
  freq: number;
  /** LLM 二次筛选投了赞成票的候选词;UI 默认勾上 */
  suggested: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  git_hash: string;
  git_dirty: string;
  rustc_version: string;
  build_time: string;
  target: string;
  tauri_version: string;
  debug: boolean;
}

export interface ProfileTemplateInfo {
  id: string;
  name: string;
  description: string;
  mode: string;
  /** 当前 prompt 文本 —— 真源在 Rust profile_templates.rs,前端只读展示。 */
  prompt: string;
}

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (cfg: Config) => invoke<void>("save_config", { cfg }),
  listProfileTemplates: () =>
    invoke<ProfileTemplateInfo[]>("list_profile_templates"),
  listDevices: () => invoke<DeviceInfo[]>("list_devices"),
  loadHistory: (limit = 200) => invoke<HistoryEntry[]>("load_history", { limit }),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  getHistoryStats: () => invoke<HistoryStats>("get_history_stats"),
  getLatencyStats: () => invoke<LatencyStats>("get_latency_stats"),
  repolishHistory: (historyId: number, profileId: string) =>
    invoke<string>("repolish_history", { historyId, profileId }),
  checkOllama: (url: string) => invoke<void>("check_ollama", { url }),
  openLogs: () => invoke<void>("open_logs"),
  openLogDir: () => invoke<void>("open_log_dir"),
  suspendHotkey: () => invoke<void>("suspend_hotkey"),
  resumeHotkey: () => invoke<void>("resume_hotkey"),
  readRecentLogs: (limit: number) => invoke<string[]>("read_recent_logs", { limit }),
  openConfigDir: () => invoke<void>("open_config_dir"),
  listLocalEngines: () => invoke<LocalEngineInfo[]>("list_local_engines"),
  getLocalEngineInfo: (id: string) =>
    invoke<LocalEngineInfo>("get_local_engine_info", { id }),
  downloadLocalEngine: (id: string) =>
    invoke<void>("download_local_engine", { id }),
  importLocalEngineTarball: (id: string, path: string) =>
    invoke<void>("import_local_engine_tarball", { id, path }),
  getPunctModelInfo: () => invoke<PunctModelInfo>("get_punct_model_info"),
  downloadPunctModel: () => invoke<void>("download_punct_model"),
  importPunctModelTarball: (path: string) =>
    invoke<void>("import_punct_model_tarball", { path }),
  benchTranscribeFile: (path: string, providerIds: string[]) =>
    invoke<void>("bench_transcribe_file", { path, providerIds }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
  exportHotwordsCsv: () => invoke<string>("export_hotwords_csv"),
  importHotwordsCsv: (csv: string, merge: boolean) =>
    invoke<number>("import_hotwords_csv", { csv, merge }),
  exportConfig: () => invoke<string>("export_config"),
  importConfig: (json: string) => invoke<void>("import_config", { json }),
  checkAccessibility: () => invoke<boolean>("check_accessibility"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  listHotwordSources: () => invoke<HotwordSourceInfo[]>("list_hotword_sources"),
  scanHotwordCandidates: (sourceId: string, days: number, backendId: string) =>
    invoke<HotwordCandidate[]>("scan_hotword_candidates", {
      sourceId,
      days,
      backendId,
    }),
  addHotwords: (words: string[]) => invoke<number>("add_hotwords", { words }),
};

export const ASR_PROVIDERS = [
  { value: "volc", label: "豆包(流式)" },
  { value: "xfyun", label: "讯飞(流式)" },
  { value: "zhipu", label: "智谱 GLM-ASR" },
  { value: "openrouter", label: "OpenRouter Whisper" },
  { value: "mimo", label: "MiMo ASR (mimo-v2.5-asr)" },
  { value: "local", label: "本地引擎(离线)" },
];

// 和 Rust config.rs 里 POLISH_MODE_* / OUTPUT_MODE_* 常量保持一致
export const POLISH_MODE_OFF = "off";
export const POLISH_MODE_OLLAMA = "ollama";
export const POLISH_MODE_OPENROUTER = "openrouter";
export const POLISH_MODE_CLOUD = "cloud";

// LLM backend mode 选项。"off" 是兼容老配置的 sentinel,UI 不再暴露——
// 关闭润色走 profile 的 backend dropdown 选"(关闭)"(backend_id == ""),
// backend 永远代表"配了哪种连接"。
export const POLISH_MODES = [
  { value: POLISH_MODE_OLLAMA, label: "Ollama 本地" },
  { value: POLISH_MODE_OPENROUTER, label: "OpenRouter 云端" },
  { value: POLISH_MODE_CLOUD, label: "OpenAI 兼容 API" },
];

export const OUTPUT_MODE_INPUT = "input";
export const OUTPUT_MODE_CLIPBOARD = "clipboard";
export const OUTPUT_MODE_PANEL = "panel";

export const OUTPUT_MODES = [
  {
    value: OUTPUT_MODE_INPUT,
    label: "自动输入到焦点窗口(默认)",
    description: "模拟键盘输入到当前焦点窗口,省事。",
  },
  {
    value: OUTPUT_MODE_PANEL,
    label: "显示在悬浮窗,手动复制",
    description: "结果停在悬浮窗里可再编辑,点「复制」后自己粘贴。",
  },
];

// 跟 Rust config.rs 里 TRIGGER_MODE_* 常量保持一致
export const TRIGGER_MODE_TOGGLE = "toggle";
export const TRIGGER_MODE_PTT = "push_to_talk";
export const TRIGGER_MODE_DOUBLE_TAP_HOLD = "double_tap_hold";

export const TRIGGER_MODES = [
  {
    value: TRIGGER_MODE_TOGGLE,
    label: "按一下开始,再按一下结束",
    description: "默认。按主热键启动录音,再按一下停。",
  },
  {
    value: TRIGGER_MODE_PTT,
    label: "按住说话,松开停止",
    description: "按住主热键的整段时间内录音,松开立即停。",
  },
  {
    value: TRIGGER_MODE_DOUBLE_TAP_HOLD,
    label: "双击 modifier 并保持",
    description:
      "350ms 内连按两下选定的 modifier 键并保持按住,松开停止录音。跟 macOS 听写「双击 Fn」风格一致。",
  },
];

export const DOUBLE_TAP_MODIFIERS = [
  { value: "right_option", label: "右 ⌥ Option / Alt(推荐)" },
  { value: "left_option", label: "左 ⌥ Option / Alt" },
  { value: "right_ctrl", label: "右 ⌃ Control" },
  { value: "left_ctrl", label: "左 ⌃ Control" },
  { value: "right_shift", label: "右 ⇧ Shift" },
  { value: "left_shift", label: "左 ⇧ Shift" },
  { value: "right_cmd", label: "右 ⌘ Command / Win" },
  { value: "left_cmd", label: "左 ⌘ Command / Win" },
];
