import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  api,
  ASR_PROVIDERS,
  POLISH_MODES,
  OUTPUT_MODES,
  Config,
  DeviceInfo,
  DownloadProgress,
  PolishProfile,
  SenseVoiceInfo,
} from "../api";
import {
  parseHotkeyKeys,
  formatHotkeyKey,
  keyCodeToName,
  validateHotkey,
  IS_MAC,
} from "../lib/hotkey";
import { saveTextToFile, readTextFromFile } from "../lib/fileDialogHelpers";
import { PROMPT_TEMPLATES, PromptTemplate } from "../lib/promptTemplates";

export type SettingsSection = "asr" | "polish" | "record" | "hotwords" | "log";

const SECTION_META: Record<SettingsSection, { title: string; subtitle: string }> = {
  asr: { title: "语音识别", subtitle: "识别后端与对应的密钥" },
  polish: {
    title: "AI 润色",
    subtitle: "识别完成后交给 LLM 按你的 prompt 再润一遍；可建多个 profile 按场景切换",
  },
  record: { title: "录音参数", subtitle: "麦克风、快捷键与增益" },
  hotwords: { title: "热词替换", subtitle: "识别后自动替换（AI 润色之后执行）" },
  log: { title: "日志", subtitle: "日志级别与文件位置" },
};

export function SettingsView({ section }: { section: SettingsSection }) {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [errMsg, setErrMsg] = useState("");

  useEffect(() => {
    api.getConfig().then(setCfg);
    api.listDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

  // 自动保存：cfg 每次变化 → 500ms debounce → 调 saveConfig
  useEffect(() => {
    if (!cfg) return;
    setSaveState("saving");
    const timer = setTimeout(async () => {
      try {
        await api.saveConfig(cfg);
        setSaveState("saved");
        setErrMsg("");
        setTimeout(() => setSaveState("idle"), 1500);
      } catch (e) {
        setSaveState("error");
        setErrMsg(String(e));
      }
    }, 500);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(cfg)]);

  if (!cfg) return <div className="p-8 text-gray-500">加载中…</div>;

  const update = <K extends keyof Config>(k: K, v: Config[K]) => {
    setCfg({ ...cfg, [k]: v });
  };

  const updateHotword = (key: string, value: string) => {
    const hotwords = { ...cfg.hotwords };
    if (key) hotwords[key] = value;
    setCfg({ ...cfg, hotwords });
  };

  const deleteHotword = (key: string) => {
    const hotwords = { ...cfg.hotwords };
    delete hotwords[key];
    setCfg({ ...cfg, hotwords });
  };

  const meta = SECTION_META[section];

  return (
    <div className="p-10 max-w-3xl mx-auto space-y-5 pb-10">
      <div className="mb-2 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-gray-100">{meta.title}</h1>
          <p className="text-sm text-gray-500 mt-0.5">{meta.subtitle}</p>
        </div>
        <SaveIndicator state={saveState} errMsg={errMsg} />
      </div>
      {saveState === "error" && errMsg && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/30 text-[12px] text-red-300 leading-relaxed">
          <div className="font-medium text-red-400 mb-0.5">保存失败</div>
          <div className="break-all">{errMsg}</div>
        </div>
      )}

      {section === "asr" && (
        <section className="card space-y-3.5">
          <Field label="识别后端">
            <select
              className="input"
              value={cfg.asr_provider}
              onChange={(e) => update("asr_provider", e.target.value)}
            >
              {ASR_PROVIDERS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          </Field>

          {cfg.asr_provider === "zhipu" && (
            <Field label="智谱 API Key">
              <input
                type="password"
                className="input"
                value={cfg.asr_api_key}
                onChange={(e) => update("asr_api_key", e.target.value)}
              />
            </Field>
          )}

          {cfg.asr_provider === "xfyun" && (
            <>
              <TextField label="App ID" value={cfg.xfyun_app_id} onChange={(v) => update("xfyun_app_id", v)} />
              <TextField label="Access Key ID" value={cfg.xfyun_access_key_id} onChange={(v) => update("xfyun_access_key_id", v)} />
              <TextField label="Access Key Secret" value={cfg.xfyun_access_key_secret} onChange={(v) => update("xfyun_access_key_secret", v)} password />
            </>
          )}

          {cfg.asr_provider === "volc" && (
            <>
              <TextField label="App Key" value={cfg.volc_app_key} onChange={(v) => update("volc_app_key", v)} />
              <TextField label="Access Token" value={cfg.volc_access_token} onChange={(v) => update("volc_access_token", v)} password />
              <Field label="识别模型">
                <select
                  className="input"
                  value={cfg.volc_resource_id}
                  onChange={(e) => update("volc_resource_id", e.target.value)}
                >
                  <option value="volc.seedasr.sauc.duration">volc.seedasr.sauc.duration (2.0)</option>
                  <option value="volc.bigasr.sauc.duration">volc.bigasr.sauc.duration (1.0)</option>
                </select>
              </Field>
            </>
          )}

          {cfg.asr_provider === "openrouter" && (
            <>
              <TextField label="OpenRouter API Key" value={cfg.openrouter_api_key} onChange={(v) => update("openrouter_api_key", v)} password />
              <Field label="模型">
                <input
                  className="input"
                  value={cfg.openrouter_model}
                  onChange={(e) => update("openrouter_model", e.target.value)}
                  placeholder="openai/whisper-large-v3-turbo"
                />
                <div className="flex gap-1.5 mt-1.5 flex-wrap">
                  {[
                    "openai/whisper-large-v3-turbo",
                    "openai/gpt-4o-mini-transcribe",
                    "openai/gpt-4o-transcribe",
                  ].map((m) => (
                    <button
                      key={m}
                      type="button"
                      className="btn-ghost !py-0.5 !px-2 text-[11px] font-mono"
                      onClick={() => update("openrouter_model", m)}
                    >
                      {m}
                    </button>
                  ))}
                </div>
                <p className="text-[11px] text-gray-500 leading-relaxed mt-1">
                  填 OpenRouter 模型 slug。whisper-large-v3-turbo 便宜($0.04/小时);
                  gpt-4o-mini-transcribe / gpt-4o-transcribe 对气声 / 低 SNR 语音
                  更鲁棒。未支持的模型 OpenRouter 会返回 400。
                </p>
              </Field>
              <Field label="强制语言">
                <select
                  className="input"
                  value={cfg.openrouter_language}
                  onChange={(e) => update("openrouter_language", e.target.value)}
                >
                  <option value="zh">zh - 中文(推荐)</option>
                  <option value="en">en - English</option>
                  <option value="ja">ja - 日本語</option>
                  <option value="ko">ko - 한국어</option>
                  <option value="">auto - 服务端自动(不稳定,易误判)</option>
                </select>
                <p className="text-[11px] text-gray-500 leading-relaxed mt-1">
                  Whisper 对气声的自动语言判定不稳定,常把中文气声识别成韩语。
                  强制指定一门语言跳过 auto-detect 识别更准。
                </p>
              </Field>
            </>
          )}

          {cfg.asr_provider === "local" && (
            <>
              <Field
                label={
                  <>
                    <span>模型精度</span>
                    <label className="ml-auto flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={cfg.local_use_fp32_model}
                        onChange={(e) => update("local_use_fp32_model", e.target.checked)}
                        className="accent-accent"
                      />
                      {cfg.local_use_fp32_model ? "fp32 完整" : "int8 量化"}
                    </label>
                  </>
                }
              >
                <p className="text-[11px] text-gray-500 leading-relaxed mt-0.5">
                  fp32 完整(model.onnx, ~894MB)精度更高,适合气声 / 复杂场景。
                  int8 量化(~228MB)速度快、内存小,普通场景够用。模型目录里
                  两个文件都存在,切换不需重下载。
                </p>
              </Field>
              <LocalSenseVoicePanel />
            </>
          )}
        </section>
      )}

      {section === "polish" && (
        <PolishSection cfg={cfg} setCfg={setCfg} />
      )}

      {section === "record" && (
        <section className="card space-y-3.5">
          <Field label="麦克风">
            <select
              className="input"
              value={cfg.device_name}
              onChange={(e) => update("device_name", e.target.value)}
            >
              <option value="">(默认设备)</option>
              {devices.map((d) => (
                <option key={d.name} value={d.name}>{d.name}</option>
              ))}
            </select>
          </Field>
          <Field label={<><span>快捷键</span><KbdCombo combo={cfg.hotkey} /></>}>
            <HotkeyRecorder
              value={cfg.hotkey}
              onChange={(v) => update("hotkey", v)}
            />
            {(() => {
              const err = validateHotkey(cfg.hotkey);
              return err ? (
                <p className="text-[11px] text-amber-400 mt-1.5 leading-relaxed">
                  ⚠ {err}（不会保存成功）
                </p>
              ) : null;
            })()}
          </Field>
          <Field
            label={
              <>
                <span>触发方式</span>
                <label className="ml-auto flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.push_to_talk}
                    onChange={(e) => update("push_to_talk", e.target.checked)}
                    className="accent-accent"
                  />
                  按住说话
                </label>
              </>
            }
          >
            <p className="text-[11px] text-gray-500 leading-relaxed mt-0.5">
              {cfg.push_to_talk
                ? "按住快捷键录音，松开自动停止——适合短句和明确边界的场景（按组合键时松开任一键都算松开）"
                : "按一下开始、再按一下结束（默认）——适合长句、讲一段话的场景"}
            </p>
          </Field>
          <Field label="输出方式">
            <select
              className="input"
              value={cfg.output_mode}
              onChange={(e) => update("output_mode", e.target.value)}
            >
              {OUTPUT_MODES.map((m) => (
                <option key={m.value} value={m.value}>{m.label}</option>
              ))}
            </select>
            <p className="text-[11px] text-gray-500 mt-1 leading-relaxed">
              {OUTPUT_MODES.find((m) => m.value === cfg.output_mode)?.description}
            </p>
          </Field>
          <Field label={<><span>信号增益</span><span className="ml-auto font-mono text-accent">{cfg.gain}×</span></>}>
            <input
              type="range"
              min={1}
              max={10}
              value={cfg.gain}
              onChange={(e) => update("gain", parseInt(e.target.value))}
              className="w-full accent-accent"
            />
            <div className="flex justify-between text-[10px] text-gray-600 mt-1 font-mono">
              <span>1×</span><span>5×</span><span>10×</span>
            </div>
            <p className="text-[11px] text-gray-500 leading-relaxed mt-1">
              {cfg.voice_enhance
                ? "开启「气声增强」后,压缩器和 peak normalize 会自动均衡音量,增益通常保持 1× 即可。"
                : "固定倍数放大,值太大会 clip(爆音)、太小气声识别不出。建议开启下方「气声增强」自适应均衡。"}
            </p>
          </Field>
          <Field
            label={
              <>
                <span>气声增强</span>
                <label className="ml-auto flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.voice_enhance}
                    onChange={(e) => update("voice_enhance", e.target.checked)}
                    className="accent-accent"
                  />
                  {cfg.voice_enhance ? "开启" : "关闭"}
                </label>
              </>
            }
          >
            <p className="text-[11px] text-gray-500 leading-relaxed mt-0.5">
              预处理管线:pre-emphasis(增强高频摩擦音)+ 压缩器(提升低能量段)+
              批处理 ASR 结束时的 peak normalize。显著改善气声 / 耳语输入的识别率,
              对正常说话也无副作用。默认开启。
            </p>
          </Field>

          <div className="pt-3 border-t border-white/5 space-y-3.5">
            <Field
              label={
                <>
                  <span>静音自动停止（VAD）</span>
                  <label className="ml-auto flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={cfg.vad_enabled}
                      onChange={(e) => update("vad_enabled", e.target.checked)}
                      className="accent-accent"
                    />
                    {cfg.vad_enabled ? "开启" : "关闭"}
                  </label>
                </>
              }
            >
              <p className="text-[11px] text-gray-500 leading-relaxed mt-0.5">
                检测到你开口后，连续静音超过下方时长就自动结束录音，不用再按一次热键。关了就回到"两次热键"手动模式。
              </p>
            </Field>

            {cfg.vad_enabled && (
              <>
                <Field
                  label={
                    <>
                      <span>静音时长阈值</span>
                      <span className="ml-auto font-mono text-accent">
                        {(cfg.vad_silence_ms / 1000).toFixed(1)} 秒
                      </span>
                    </>
                  }
                >
                  <input
                    type="range"
                    min={500}
                    max={5000}
                    step={100}
                    value={cfg.vad_silence_ms}
                    onChange={(e) => update("vad_silence_ms", parseInt(e.target.value))}
                    className="w-full accent-accent"
                  />
                  <div className="flex justify-between text-[10px] text-gray-600 mt-1 font-mono">
                    <span>0.5s（反应快）</span>
                    <span>1.5s</span>
                    <span>5.0s（更包容）</span>
                  </div>
                </Field>

                <Field
                  label={
                    <>
                      <span>说话概率阈值</span>
                      <span className="ml-auto font-mono text-accent">
                        {cfg.vad_threshold.toFixed(2)}
                      </span>
                    </>
                  }
                >
                  <input
                    type="range"
                    min={20}
                    max={80}
                    step={5}
                    value={Math.round(cfg.vad_threshold * 100)}
                    onChange={(e) =>
                      update("vad_threshold", parseInt(e.target.value) / 100)
                    }
                    className="w-full accent-accent"
                  />
                  <div className="flex justify-between text-[10px] text-gray-600 mt-1 font-mono">
                    <span>0.20（敏感）</span>
                    <span>0.50</span>
                    <span>0.80（保守）</span>
                  </div>
                  <p className="text-[11px] text-gray-500 leading-relaxed mt-1.5">
                    silero 神经网络 VAD 输出的"是说话"概率门槛(0-1)。
                    嘈杂环境 / 误触发频繁 → 调高;气声 / 轻声被误切 → 调低。
                    模型 ~640KB,首次启用 VAD 时自动下载。
                  </p>
                </Field>
              </>
            )}
          </div>
        </section>
      )}

      {section === "hotwords" && (
        <section className="card space-y-3.5">
          <div className="space-y-2">
            {Object.entries(cfg.hotwords).length === 0 && (
              <div className="text-xs text-gray-500 py-2">暂无热词，点下方添加或导入 CSV</div>
            )}
            {Object.entries(cfg.hotwords).map(([from, to]) => (
              <HotwordRow
                key={from}
                from={from}
                to={to}
                onChange={(k, v) => {
                  const hotwords = { ...cfg.hotwords };
                  if (k !== from) delete hotwords[from];
                  if (k) hotwords[k] = v;
                  setCfg({ ...cfg, hotwords });
                }}
                onDelete={() => deleteHotword(from)}
              />
            ))}
            <div className="flex gap-2">
              <button
                className="btn-ghost flex-1 justify-center"
                onClick={() => updateHotword(`新热词_${Date.now()}`, "")}
              >
                ＋ 添加
              </button>
              <button className="btn-ghost" onClick={() => handleExportCsv()}>
                导出 CSV
              </button>
              <button className="btn-ghost" onClick={() => handleImportCsv(setCfg, cfg)}>
                导入 CSV
              </button>
            </div>
          </div>
        </section>
      )}

      {section === "log" && (
        <section className="card space-y-3.5">
          <Field label="日志级别">
            <select
              className="input"
              value={cfg.log_level}
              onChange={(e) => update("log_level", e.target.value)}
            >
              <option value="debug">debug</option>
              <option value="info">info</option>
              <option value="warn">warn</option>
              <option value="error">error</option>
            </select>
          </Field>
          <div className="flex gap-2 mt-3">
            <button className="btn-ghost" onClick={() => api.openLogs()}>打开最新日志</button>
            <button className="btn-ghost" onClick={() => api.openLogDir()}>打开日志目录</button>
          </div>
          <LogViewer />
        </section>
      )}
    </div>
  );
}

const CSV_FILTERS = [{ name: "CSV", extensions: ["csv"] }];

async function handleExportCsv() {
  try {
    const csv = await api.exportHotwordsCsv();
    const defaultName = `voice-claude-hotwords-${new Date().toISOString().slice(0, 10)}.csv`;
    await saveTextToFile(csv, defaultName, CSV_FILTERS);
  } catch (e) {
    alert(`导出失败：${e}`);
  }
}

async function handleImportCsv(setCfg: (c: Config) => void, _cfg: Config) {
  try {
    const csv = await readTextFromFile(CSV_FILTERS);
    if (csv === null) return;
    const merge = confirm(
      "选择「确定」合并到现有热词\n选择「取消」用 CSV 完全替换现有热词",
    );
    const added = await api.importHotwordsCsv(csv, merge);
    alert(`已导入 ${added} 条热词`);
    const latest = await api.getConfig();
    setCfg(latest);
  } catch (e) {
    alert(`导入失败：${e}`);
  }
}

type LogLevel = "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR";
const LEVEL_ORDER: LogLevel[] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

interface ParsedLogLine {
  raw: string;
  time: string;
  level: LogLevel | null;
  body: string;
}

function parseLogLine(raw: string): ParsedLogLine {
  // 形如 "2026-05-09T11:31:52.172994Z  INFO voice_claude_lib::recorder: VAD: 启动 ..."
  const m = raw.match(
    /^(\d{4}-\d{2}-\d{2}T(\d{2}:\d{2}:\d{2})(?:\.\d+)?Z)\s+(TRACE|DEBUG|INFO|WARN|ERROR)\s+(.*)$/,
  );
  if (!m) return { raw, time: "", level: null, body: raw };
  return { raw, time: m[2], level: m[3] as LogLevel, body: m[4] };
}

const LOG_LIMIT = 200;

function LogViewer() {
  const [lines, setLines] = useState<ParsedLogLine[]>([]);
  const [loading, setLoading] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [minLevel, setMinLevel] = useState<LogLevel>("INFO");
  // 记上一次拉到的 raw 内容，用于 change detection 避免无变化时重渲染
  const lastSignatureRef = useRef<string>("");

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const raws = await api.readRecentLogs(LOG_LIMIT);
      // 比较最后一行 + 长度就能判断有没有新日志，比 join 整个数组便宜
      const sig = `${raws.length}:${raws[raws.length - 1] ?? ""}`;
      if (sig !== lastSignatureRef.current) {
        lastSignatureRef.current = sig;
        setLines(raws.map(parseLogLine));
      }
    } catch (e) {
      console.warn("readRecentLogs failed:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    if (!autoRefresh) return;
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [autoRefresh, refresh]);

  const filtered = useMemo(() => {
    const minIdx = LEVEL_ORDER.indexOf(minLevel);
    return lines.filter(
      (l) => !l.level || LEVEL_ORDER.indexOf(l.level) <= minIdx,
    );
  }, [lines, minLevel]);

  return (
    <div className="mt-4 pt-4 border-t border-white/5">
      <div className="flex items-center gap-3 mb-2">
        <div className="text-xs font-semibold text-gray-300">最近日志</div>
        <select
          className="text-[11px] bg-bg-900/60 border border-white/10 rounded px-2 py-1 text-gray-300 outline-none"
          value={minLevel}
          onChange={(e) => setMinLevel(e.target.value as LogLevel)}
          title="最低显示级别"
        >
          <option value="TRACE">≥ trace</option>
          <option value="DEBUG">≥ debug</option>
          <option value="INFO">≥ info</option>
          <option value="WARN">≥ warn</option>
          <option value="ERROR">仅 error</option>
        </select>
        <label className="flex items-center gap-1.5 text-[11px] text-gray-400 cursor-pointer">
          <input
            type="checkbox"
            checked={autoRefresh}
            onChange={(e) => setAutoRefresh(e.target.checked)}
            className="accent-accent"
          />
          自动刷新
        </label>
        <span className="flex-1" />
        <span className="text-[10px] text-gray-600 font-mono">
          {filtered.length} / {lines.length}
        </span>
        <button
          className="text-[11px] px-2 py-1 rounded bg-white/5 hover:bg-white/10 text-gray-400 hover:text-gray-200 transition disabled:opacity-50"
          onClick={refresh}
          disabled={loading}
        >
          {loading ? "读取中…" : "刷新"}
        </button>
      </div>
      <div
        className="bg-bg-900/80 border border-white/5 rounded-lg p-3 font-mono text-[11px] leading-relaxed max-h-80 overflow-y-auto overflow-x-hidden"
        style={{ scrollbarGutter: "stable" }}
      >
        {filtered.length === 0 ? (
          <div className="text-gray-600">（暂无日志）</div>
        ) : (
          filtered.map((l, i) => (
            <div key={i} className="whitespace-pre-wrap break-all py-0.5">
              {l.time && <span className="text-gray-600">{l.time}</span>}
              {l.level && (
                <span className={`ml-2 font-semibold ${levelColor(l.level)}`}>
                  {l.level.padEnd(5)}
                </span>
              )}
              <span className="ml-2 text-gray-300">{l.body}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function levelColor(level: LogLevel): string {
  switch (level) {
    case "ERROR":
      return "text-red-400";
    case "WARN":
      return "text-amber-400";
    case "INFO":
      return "text-brand-blue";
    case "DEBUG":
      return "text-gray-500";
    case "TRACE":
      return "text-gray-600";
  }
}

function PolishSection({
  cfg,
  setCfg,
}: {
  cfg: Config;
  setCfg: (c: Config) => void;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set([cfg.active_profile_id]),
  );
  const [showTemplates, setShowTemplates] = useState(false);
  const profiles = cfg.polish_profiles;
  const multi = profiles.length > 1;

  const replaceProfiles = (next: PolishProfile[], nextActive?: string) => {
    setCfg({
      ...cfg,
      polish_profiles: next,
      active_profile_id: nextActive ?? cfg.active_profile_id,
    });
  };

  const updateProfile = (id: string, patch: Partial<PolishProfile>) => {
    replaceProfiles(profiles.map((p) => (p.id === id ? { ...p, ...patch } : p)));
  };

  const addProfile = () => {
    const id = cryptoId();
    const newProfile: PolishProfile = {
      id,
      name: "新 Profile",
      mode: "off",
      url: "http://localhost:11434/api/generate",
      model: "qwen2.5:3b",
      api_key: "",
      prompt: "润色以下文字，保留原意：\n\n{text}",
    };
    replaceProfiles([...profiles, newProfile]);
    setExpanded((s) => new Set(s).add(id));
    setShowTemplates(false);
  };

  const addFromTemplate = (t: PromptTemplate) => {
    const id = cryptoId();
    const newProfile: PolishProfile = {
      id,
      name: t.name,
      mode: t.mode,
      url: t.mode === "ollama" ? "http://localhost:11434/api/generate" : "",
      model: "",
      api_key: "",
      prompt: t.prompt,
    };
    replaceProfiles([...profiles, newProfile]);
    setExpanded((s) => new Set(s).add(id));
    setShowTemplates(false);
  };

  const duplicateProfile = (id: string) => {
    const src = profiles.find((p) => p.id === id);
    if (!src) return;
    const newId = cryptoId();
    const copy: PolishProfile = { ...src, id: newId, name: `${src.name} · 副本` };
    const idx = profiles.findIndex((p) => p.id === id);
    const next = [...profiles.slice(0, idx + 1), copy, ...profiles.slice(idx + 1)];
    replaceProfiles(next);
    setExpanded((s) => new Set(s).add(newId));
  };

  const removeProfile = (id: string) => {
    if (profiles.length <= 1) return; // 至少保留 1 个
    const next = profiles.filter((p) => p.id !== id);
    const nextActive =
      cfg.active_profile_id === id ? next[0].id : cfg.active_profile_id;
    replaceProfiles(next, nextActive);
  };

  const toggleExpanded = (id: string) => {
    setExpanded((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };

  return (
    <>
      {multi && (
        <section className="card">
          <div className="flex items-center gap-3">
            <label className="label !mb-0 min-w-24">当前 Profile</label>
            <select
              className="input flex-1"
              value={cfg.active_profile_id}
              onChange={(e) =>
                setCfg({ ...cfg, active_profile_id: e.target.value })
              }
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>
          <p className="text-[11px] text-gray-500 mt-2">
            或从菜单栏托盘图标里快速切换（无需打开设置）
          </p>
        </section>
      )}

      <section className="card space-y-3">
        <div className="section-title">📚 Profiles</div>
        {profiles.map((profile) => (
          <ProfileCard
            key={profile.id}
            profile={profile}
            active={profile.id === cfg.active_profile_id}
            expanded={expanded.has(profile.id)}
            canDelete={profiles.length > 1}
            onToggleExpanded={() => toggleExpanded(profile.id)}
            onSetActive={() =>
              setCfg({ ...cfg, active_profile_id: profile.id })
            }
            onChange={(patch) => updateProfile(profile.id, patch)}
            onDuplicate={() => duplicateProfile(profile.id)}
            onDelete={() => removeProfile(profile.id)}
          />
        ))}
        <div className="flex gap-2">
          <button
            className="flex-1 py-2.5 rounded-xl bg-white/[0.03] border border-dashed border-white/10 text-gray-400 hover:bg-white/[0.05] hover:text-gray-200 hover:border-white/20 transition text-sm"
            onClick={addProfile}
          >
            ＋ 空白 Profile
          </button>
          <button
            className={
              "flex-1 py-2.5 rounded-xl border transition text-sm " +
              (showTemplates
                ? "bg-accent/10 border-accent/30 text-accent"
                : "bg-white/[0.03] border-dashed border-white/10 text-gray-400 hover:bg-white/[0.05] hover:text-gray-200 hover:border-white/20")
            }
            onClick={() => setShowTemplates((s) => !s)}
          >
            📋 从模板{showTemplates ? " ▲" : " ▼"}
          </button>
        </div>
        {showTemplates && (
          <div className="grid gap-2 mt-2">
            {PROMPT_TEMPLATES.map((t) => (
              <button
                key={t.id}
                className="text-left p-3 rounded-lg bg-white/[0.02] hover:bg-white/[0.06] border border-white/[0.06] hover:border-white/[0.15] transition"
                onClick={() => addFromTemplate(t)}
              >
                <div className="text-sm font-medium text-gray-100">{t.name}</div>
                <div className="text-[11px] text-gray-500 mt-0.5 leading-relaxed">
                  {t.description}
                </div>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="card">
        <Field label="请求超时（秒）">
          <input
            type="number"
            className="input"
            value={cfg.correct_timeout}
            onChange={(e) =>
              setCfg({
                ...cfg,
                correct_timeout: parseInt(e.target.value) || 10,
              })
            }
          />
          <p className="text-[11px] text-gray-500 mt-1">
            所有 profile 共享；超过这个时间 LLM 没返回就使用原文
          </p>
        </Field>
      </section>
    </>
  );
}

function ProfileCard({
  profile,
  active,
  expanded,
  canDelete,
  onToggleExpanded,
  onSetActive,
  onChange,
  onDuplicate,
  onDelete,
}: {
  profile: PolishProfile;
  active: boolean;
  expanded: boolean;
  canDelete: boolean;
  onToggleExpanded: () => void;
  onSetActive: () => void;
  onChange: (patch: Partial<PolishProfile>) => void;
  onDuplicate: () => void;
  onDelete: () => void;
}) {
  const modelHint =
    profile.mode === "off"
      ? "—"
      : `${profile.mode}/${profile.model || "(未设)"}`;

  return (
    <div
      className={
        "rounded-xl border transition " +
        (active
          ? "border-green-400/40 bg-green-400/[0.03]"
          : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.12]")
      }
    >
      <div
        className="flex items-center gap-2 px-3.5 py-2.5 cursor-pointer"
        onClick={onToggleExpanded}
      >
        <span className="text-sm text-gray-100 font-medium">{profile.name}</span>
        {active && (
          <span className="px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-green-400/10 text-green-400 border border-green-400/25">
            活跃
          </span>
        )}
        <span className="flex-1" />
        <span className="text-[11px] text-gray-500 font-mono px-1.5 py-0.5 rounded bg-white/5">
          {modelHint}
        </span>
        {!active && (
          <button
            title="设为活跃"
            className="w-7 h-7 rounded-md text-gray-500 hover:bg-white/[0.06] hover:text-gray-200 flex items-center justify-center transition text-xs"
            onClick={(e) => {
              e.stopPropagation();
              onSetActive();
            }}
          >
            ○
          </button>
        )}
        <button
          title="展开 / 折叠"
          className="icon-btn"
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpanded();
          }}
        >
          {expanded ? "▼" : "▸"}
        </button>
        <button
          title="复制"
          className="icon-btn"
          onClick={(e) => {
            e.stopPropagation();
            onDuplicate();
          }}
        >
          ⎘
        </button>
        <button
          title={canDelete ? "删除" : "至少保留一个 profile"}
          className={
            "w-7 h-7 rounded-md flex items-center justify-center transition text-xs " +
            (canDelete
              ? "text-gray-500 hover:bg-red-500/15 hover:text-red-400"
              : "text-gray-700 cursor-not-allowed opacity-40")
          }
          disabled={!canDelete}
          onClick={(e) => {
            e.stopPropagation();
            if (!canDelete) return;
            if (confirm(`删除 profile「${profile.name}」？`)) onDelete();
          }}
        >
          ✕
        </button>
      </div>
      {expanded && (
        <div className="px-3.5 pb-3.5 border-t border-white/5 pt-3 space-y-3">
          <Field label="名称">
            <input
              className="input"
              value={profile.name}
              onChange={(e) => onChange({ name: e.target.value })}
            />
          </Field>
          <Field label="润色后端">
            <select
              className="input"
              value={profile.mode}
              onChange={(e) => onChange({ mode: e.target.value })}
            >
              {POLISH_MODES.map((m) => (
                <option key={m.value} value={m.value}>{m.label}</option>
              ))}
            </select>
          </Field>
          {profile.mode !== "off" && (
            <>
              <TextField
                label={profile.mode === "ollama" ? "Ollama API 地址" : "API 地址"}
                value={profile.url}
                onChange={(v) => onChange({ url: v })}
              />
              <TextField
                label="模型"
                value={profile.model}
                onChange={(v) => onChange({ model: v })}
              />
              <TextField
                label="API Key"
                value={profile.api_key}
                onChange={(v) => onChange({ api_key: v })}
                password
              />
              <Field
                label={
                  <>
                    <span>Prompt 模板</span>
                    <span className="ml-auto text-[11px] text-gray-500 font-mono">
                      {"{text}"} = 识别原文
                    </span>
                  </>
                }
              >
                <textarea
                  className="input font-mono text-[12px] leading-relaxed"
                  rows={5}
                  value={profile.prompt}
                  onChange={(e) => onChange({ prompt: e.target.value })}
                  placeholder="把下面这段语音识别结果润色为 ...：\n\n{text}"
                />
              </Field>
            </>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * 快捷键输入：保留手打 input，旁边加一个「⌨ 录入」按钮。
 * 点录入进入 capturing：suspend 系统 hotkey，监听 keydown，按下目标组合键
 * 后生成 "cmd+shift+f5" 格式字符串填入，save_config 会自动 re-register。
 * ESC 取消并 resume 原热键。
 */
function HotkeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const [capturing, setCapturing] = useState(false);
  // 用 ref 捕获最新的 onChange，让 effect 的 dep 只跟 capturing 变化；否则父组件
  // 每次 render 新建的 onChange 引用会触发 effect teardown + setup（频繁 suspend/resume）
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!capturing) return;
    api.suspendHotkey().catch(() => {});

    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      // 单按修饰键时等用户继续按主键
      if (["Shift", "Control", "Alt", "Meta", "CapsLock"].includes(e.key)) return;

      const mods: string[] = [];
      if (e.metaKey) mods.push(IS_MAC ? "cmd" : "win");
      if (e.ctrlKey) mods.push("ctrl");
      if (e.altKey) mods.push(IS_MAC ? "option" : "alt");
      if (e.shiftKey) mods.push("shift");
      const main = keyCodeToName(e.code, e.key);
      if (mods.length === 0 || !main) return;
      onChangeRef.current([...mods, main].join("+"));
      setCapturing(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => {
      window.removeEventListener("keydown", handler, true);
      api.resumeHotkey().catch(() => {});
    };
  }, [capturing]);

  return (
    <div className="flex gap-2">
      <input
        className={
          "input font-mono flex-1 " +
          (capturing ? "!bg-accent/10 !border-accent/40" : "")
        }
        value={capturing ? "按下目标组合键…（ESC 取消）" : value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="cmd+shift+f5"
        readOnly={capturing}
      />
      <button
        className={
          "px-3 rounded-lg text-xs font-medium transition " +
          (capturing
            ? "bg-red-500/20 border border-red-500/40 text-red-300 hover:bg-red-500/30"
            : "bg-white/5 border border-white/10 text-gray-300 hover:bg-white/10")
        }
        onClick={() => setCapturing((s) => !s)}
      >
        {capturing ? "取消" : "⌨ 录入"}
      </button>
    </div>
  );
}

function cryptoId(): string {
  // 浏览器原生 UUID → 取头 8 位够用
  const u =
    typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
      ? crypto.randomUUID()
      : Math.random().toString(36).slice(2);
  return u.replace(/-/g, "").slice(0, 8);
}

function SaveIndicator({ state, errMsg }: { state: "idle" | "saving" | "saved" | "error"; errMsg: string }) {
  if (state === "idle") {
    return (
      <div className="flex items-center gap-2 text-xs text-gray-500">
        <div className="w-1.5 h-1.5 rounded-full bg-gray-500" />
        已保存
      </div>
    );
  }
  if (state === "saving") {
    return (
      <div className="flex items-center gap-2 text-xs text-amber-400">
        <div className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
        保存中…
      </div>
    );
  }
  if (state === "saved") {
    return (
      <div className="flex items-center gap-2 text-xs text-green-400">
        <div className="w-1.5 h-1.5 rounded-full bg-green-400" />
        已保存 ✓
      </div>
    );
  }
  return (
    <div className="flex items-center gap-2 text-xs text-red-400" title={errMsg}>
      <div className="w-1.5 h-1.5 rounded-full bg-red-400" />
      保存失败
    </div>
  );
}

function Field(props: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div>
      <label className="label flex items-center">{props.label}</label>
      {props.children}
    </div>
  );
}

function TextField(props: { label: string; value: string; onChange: (v: string) => void; password?: boolean }) {
  return (
    <Field label={props.label}>
      <input
        type={props.password ? "password" : "text"}
        className="input"
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    </Field>
  );
}

function HotwordRow(props: {
  from: string;
  to: string;
  onChange: (k: string, v: string) => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex gap-2 items-center">
      <input
        className="input flex-1"
        placeholder="识别到的错词"
        value={props.from}
        onChange={(e) => props.onChange(e.target.value, props.to)}
      />
      <span className="text-gray-500 px-1">→</span>
      <input
        className="input flex-1"
        placeholder="正确的词"
        value={props.to}
        onChange={(e) => props.onChange(props.from, e.target.value)}
      />
      <button
        className="w-9 h-9 rounded-lg bg-white/[0.04] hover:bg-red-500/20 hover:text-red-400 transition flex items-center justify-center text-gray-500"
        onClick={props.onDelete}
      >
        ×
      </button>
    </div>
  );
}

function KbdCombo({ combo }: { combo: string }) {
  return (
    <div className="ml-auto flex gap-1">
      {parseHotkeyKeys(combo).map((k, i) => (
        <kbd key={i}>{formatHotkeyKey(k)}</kbd>
      ))}
    </div>
  );
}

function LocalSenseVoicePanel() {
  const [info, setInfo] = useState<SenseVoiceInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloaded, setDownloaded] = useState(0);
  const [total, setTotal] = useState(0);
  // 计算速率用：上一次 tick 的 { ts, bytes }
  const speedStateRef = useRef<{ ts: number; bytes: number } | null>(null);
  const [speedBps, setSpeedBps] = useState(0); // bytes per second
  const [msg, setMsg] = useState("");

  const refresh = useCallback(() => {
    api.getSenseVoiceInfo().then(setInfo);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen<DownloadProgress>(
      "sense-voice-download-progress",
      (e) => {
        const { downloaded: d, total: t } = e.payload;
        setDownloaded(d);
        setTotal(t);
        // 计算速率：相对上一次 tick
        const now = Date.now();
        const prev = speedStateRef.current;
        if (prev) {
          const dt = (now - prev.ts) / 1000;
          if (dt > 0.2) {
            const bps = (d - prev.bytes) / dt;
            setSpeedBps(bps);
            speedStateRef.current = { ts: now, bytes: d };
          }
        } else {
          speedStateRef.current = { ts: now, bytes: d };
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const onDownload = async () => {
    setDownloading(true);
    setMsg("");
    setDownloaded(0);
    setTotal(0);
    setSpeedBps(0);
    speedStateRef.current = null;
    try {
      await api.downloadSenseVoice();
      setMsg("下载完成 ✓");
      refresh();
    } catch (e) {
      setMsg(`下载失败：${e}`);
    } finally {
      setDownloading(false);
    }
  };

  const onImport = async () => {
    const selected = await openDialog({
      filters: [
        { name: "SenseVoice 压缩包", extensions: ["bz2", "tar.bz2"] },
        { name: "All", extensions: ["*"] },
      ],
      multiple: false,
    });
    if (!selected || typeof selected !== "string") return;
    setDownloading(true);
    setMsg("校验 + 解压中…");
    try {
      await api.importSenseVoiceTarball(selected);
      setMsg("导入完成 ✓");
      refresh();
    } catch (e) {
      setMsg(`导入失败：${e}`);
    } finally {
      setDownloading(false);
    }
  };

  if (!info) {
    return <div className="text-xs text-gray-500">加载中…</div>;
  }

  const percent = total > 0 ? Math.min(100, (downloaded / total) * 100) : 0;
  const etaSec = speedBps > 0 && total > downloaded ? (total - downloaded) / speedBps : 0;

  return (
    <div className="rounded-xl bg-white/[0.03] border border-white/[0.06] p-4 space-y-3">
      {/* 状态行 */}
      <div className="flex items-center gap-3">
        <div
          className={`w-2 h-2 rounded-full ${
            info.available
              ? "bg-green-400 shadow-[0_0_8px_rgba(74,222,128,0.5)]"
              : "bg-amber-400 shadow-[0_0_8px_rgba(251,191,36,0.5)]"
          } ${downloading ? "animate-pulse" : ""}`}
        />
        <span className="text-sm text-gray-300">
          {downloading
            ? "下载中…"
            : info.available
              ? "模型已就绪 ✓"
              : "模型未下载"}
        </span>
        <span className="text-xs text-gray-500 ml-auto font-mono">
          {total > 0
            ? `${formatBytes(downloaded)} / ${formatBytes(total)}`
            : "约 1 GB"}
        </span>
      </div>

      {/* URL + SHA256 + 安装路径（可复制） */}
      <div className="bg-bg-900/40 border border-white/[0.04] rounded-lg p-2.5 space-y-1.5">
        <CopyRow
          label="下载地址"
          value={info.url}
          displayValue={<a href={info.url} target="_blank" rel="noreferrer" className="text-brand-blue hover:underline">{info.url}</a>}
        />
        <CopyRow label="SHA256" value={info.sha256} />
        {info.available && <CopyRow label="安装路径" value={info.model_dir} />}
      </div>

      {/* 下载进度 */}
      {downloading && total > 0 && (
        <div>
          <div className="h-1 bg-bg-900 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-accent to-brand-purple transition-all"
              style={{ width: `${percent}%` }}
            />
          </div>
          <div className="flex justify-between text-[10px] text-gray-500 mt-1 font-mono">
            <span>{percent.toFixed(0)}%</span>
            <span>
              {speedBps > 0 ? `${formatBytes(speedBps)}/s` : "…"}
              {etaSec > 0 && etaSec < 3600 * 24
                ? ` · 剩余 ${formatDuration(etaSec)}`
                : ""}
            </span>
          </div>
        </div>
      )}

      {/* 操作按钮 */}
      <div className="flex gap-2 flex-wrap">
        <button
          className="btn-primary disabled:opacity-50 text-xs py-1.5"
          disabled={downloading}
          onClick={onDownload}
        >
          {info.available ? "重新下载" : "下载模型"}
        </button>
        <button
          className="btn-ghost text-xs py-1.5"
          disabled={downloading}
          onClick={onImport}
        >
          📦 导入本地压缩包
        </button>
        <button
          className="btn-ghost text-xs py-1.5"
          onClick={() => api.openConfigDir()}
        >
          📁 打开模型目录
        </button>
      </div>
      {msg && <div className="text-xs text-gray-400">{msg}</div>}

      {/* 降级引导 */}
      {!info.available && (
        <div className="rounded-lg bg-accent/[0.06] border border-accent/20 p-3 text-[11px] text-gray-300 leading-relaxed">
          <div className="font-medium text-gray-200">🇨🇳 国内下载不稳？</div>
          <div className="mt-1">
            用 <code className="px-1 rounded bg-black/30 font-mono">curl -C -</code>、迅雷等工具把上面的 <code className="px-1 rounded bg-black/30 font-mono">.tar.bz2</code> 下好，然后：
            <ul className="mt-1 ml-4 list-disc text-gray-400">
              <li>点「📦 导入本地压缩包」选文件 —— 自动校验 SHA256 并解压</li>
              <li>或自己解压到「📁 模型目录」里（文件夹名保持不变）</li>
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}

function CopyRow({
  label,
  value,
  displayValue,
}: {
  label: string;
  value: string;
  displayValue?: React.ReactNode;
}) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  };
  return (
    <div className="flex items-start gap-2 text-[11px] font-mono leading-relaxed">
      <span className="text-gray-500 w-16 flex-shrink-0">{label}</span>
      <span className="flex-1 text-gray-300 break-all">
        {displayValue ?? value}
      </span>
      <button
        className="flex-shrink-0 px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-gray-400 hover:bg-white/10 hover:text-gray-200 transition text-[10px]"
        onClick={copy}
        title="复制"
      >
        {copied ? "✓" : "⎘"}
      </button>
    </div>
  );
}

function formatBytes(n: number): string {
  if (n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function formatDuration(seconds: number): string {
  const s = Math.round(seconds);
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  const ss = s % 60;
  if (m < 60) return `${m} 分 ${ss.toString().padStart(2, "0")} 秒`;
  const h = Math.floor(m / 60);
  const mm = m % 60;
  return `${h} 小时 ${mm} 分`;
}
