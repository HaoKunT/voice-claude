// Tauri IPC 调用封装。
import { invoke } from "@tauri-apps/api/core";

export interface Config {
  asr_provider: string;
  asr_api_key: string;
  xfyun_app_id: string;
  xfyun_access_key_id: string;
  xfyun_access_key_secret: string;
  openrouter_api_key: string;
  volc_app_key: string;
  volc_access_token: string;
  volc_resource_id: string;
  correct_mode: string;
  correct_url: string;
  correct_model: string;
  correct_api_key: string;
  hotkey: string;
  gain: number;
  device_name: string;
  correct_timeout: number;
  log_level: string;
  hotwords: Record<string, string>;
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
}

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (cfg: Config) => invoke<void>("save_config", { cfg }),
  listDevices: () => invoke<DeviceInfo[]>("list_devices"),
  loadHistory: (limit = 200) => invoke<HistoryEntry[]>("load_history", { limit }),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  checkOllama: (url: string) => invoke<void>("check_ollama", { url }),
  openLogs: () => invoke<void>("open_logs"),
  openConfigDir: () => invoke<void>("open_config_dir"),
  isSenseVoiceAvailable: () => invoke<boolean>("is_sense_voice_available"),
  downloadSenseVoice: () => invoke<void>("download_sense_voice"),
};

export const ASR_PROVIDERS = [
  { value: "volc", label: "豆包 / 火山引擎（实时）" },
  { value: "xfyun", label: "讯飞（实时）" },
  { value: "zhipu", label: "智谱（准确优先）" },
  { value: "openrouter", label: "OpenRouter Whisper（准确优先）" },
  { value: "local", label: "本地 SenseVoice（离线/隐私，开发中）" },
];

export const CORRECT_MODES = [
  { value: "off", label: "关闭" },
  { value: "ollama", label: "Ollama 本地" },
  { value: "openrouter", label: "OpenRouter 云端" },
  { value: "cloud", label: "兼容 OpenAI API 的云端" },
];
