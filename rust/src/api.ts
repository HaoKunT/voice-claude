// Tauri IPC 调用封装。
import { invoke } from "@tauri-apps/api/core";

export interface PolishProfile {
  id: string;
  name: string;
  mode: string;
  url: string;
  model: string;
  api_key: string;
  prompt: string;
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
  active_profile_id: string;
  hotkey: string;
  gain: number;
  device_name: string;
  correct_timeout: number;
  log_level: string;
  hotwords: Record<string, string>;
  vad_enabled: boolean;
  vad_silence_ms: number;
  vad_threshold: number;
  output_mode: string;
  push_to_talk: boolean;
  voice_enhance: boolean;
  local_use_fp32_model: boolean;
  local_use_coreml: boolean;
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

export interface SenseVoiceInfo {
  url: string;
  sha256: string;
  available: boolean;
  model_dir: string;
}

export interface DownloadProgress {
  downloaded: number;
  total: number;
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

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (cfg: Config) => invoke<void>("save_config", { cfg }),
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
  isSenseVoiceAvailable: () => invoke<boolean>("is_sense_voice_available"),
  getSenseVoiceInfo: () => invoke<SenseVoiceInfo>("get_sense_voice_info"),
  downloadSenseVoice: () => invoke<void>("download_sense_voice"),
  importSenseVoiceTarball: (path: string) =>
    invoke<void>("import_sense_voice_tarball", { path }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
  exportHotwordsCsv: () => invoke<string>("export_hotwords_csv"),
  importHotwordsCsv: (csv: string, merge: boolean) =>
    invoke<number>("import_hotwords_csv", { csv, merge }),
  exportConfig: () => invoke<string>("export_config"),
  importConfig: (json: string) => invoke<void>("import_config", { json }),
  checkAccessibility: () => invoke<boolean>("check_accessibility"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
};

export const ASR_PROVIDERS = [
  { value: "volc", label: "豆包 / 火山引擎（实时）" },
  { value: "xfyun", label: "讯飞（实时）" },
  { value: "zhipu", label: "智谱（准确优先）" },
  { value: "openrouter", label: "OpenRouter Whisper（准确优先）" },
  { value: "local", label: "本地 SenseVoice（离线 / 隐私）" },
];

// 和 Rust config.rs 里 POLISH_MODE_* / OUTPUT_MODE_* 常量保持一致
export const POLISH_MODE_OFF = "off";
export const POLISH_MODE_OLLAMA = "ollama";
export const POLISH_MODE_OPENROUTER = "openrouter";
export const POLISH_MODE_CLOUD = "cloud";

export const POLISH_MODES = [
  { value: POLISH_MODE_OFF, label: "关闭（原文直出）" },
  { value: POLISH_MODE_OLLAMA, label: "Ollama 本地" },
  { value: POLISH_MODE_OPENROUTER, label: "OpenRouter 云端" },
  { value: POLISH_MODE_CLOUD, label: "兼容 OpenAI API 的云端" },
];

export const OUTPUT_MODE_INPUT = "input";
export const OUTPUT_MODE_CLIPBOARD = "clipboard";
export const OUTPUT_MODE_PANEL = "panel";

export const OUTPUT_MODES = [
  {
    value: OUTPUT_MODE_INPUT,
    label: "自动输入到当前焦点窗口（默认）",
    description: "自动模拟键盘输入到当前焦点窗口，最省事（默认）。",
  },
  {
    value: OUTPUT_MODE_PANEL,
    label: "显示在悬浮窗，手动编辑 / 复制",
    description: "识别结果停留在悬浮窗里，文字可再编辑；点「复制」后自己粘贴到目标位置。",
  },
];
