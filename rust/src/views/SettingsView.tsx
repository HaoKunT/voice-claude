import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, ASR_PROVIDERS, CORRECT_MODES, Config, DeviceInfo } from "../api";

export function SettingsView() {
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

  return (
    <div className="p-10 max-w-3xl mx-auto space-y-5 pb-10">
      <div className="mb-2 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-gray-100">设置</h1>
          <p className="text-sm text-gray-500 mt-0.5">修改即自动保存</p>
        </div>
        <SaveIndicator state={saveState} errMsg={errMsg} />
      </div>

      {/* ASR */}
      <Section title="语音识别" icon="🎙">
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
          <TextField label="OpenRouter API Key" value={cfg.openrouter_api_key} onChange={(v) => update("openrouter_api_key", v)} password />
        )}

        {cfg.asr_provider === "local" && <LocalSenseVoicePanel />}
      </Section>

      {/* AI 纠错 */}
      <Section title="AI 纠错" icon="🧠">
        <Field label="纠错模式">
          <select
            className="input"
            value={cfg.correct_mode}
            onChange={(e) => update("correct_mode", e.target.value)}
          >
            {CORRECT_MODES.map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
        </Field>
        {cfg.correct_mode !== "off" && (
          <>
            <TextField label="API 地址" value={cfg.correct_url} onChange={(v) => update("correct_url", v)} />
            <TextField label="模型名称" value={cfg.correct_model} onChange={(v) => update("correct_model", v)} />
            <TextField label="API Key" value={cfg.correct_api_key} onChange={(v) => update("correct_api_key", v)} password />
            <Field label="超时（秒）">
              <input
                type="number"
                className="input"
                value={cfg.correct_timeout}
                onChange={(e) => update("correct_timeout", parseInt(e.target.value) || 10)}
              />
            </Field>
          </>
        )}
      </Section>

      {/* 录音参数 */}
      <Section title="录音参数" icon="🎤">
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
          <input
            className="input font-mono"
            value={cfg.hotkey}
            onChange={(e) => update("hotkey", e.target.value)}
            placeholder="cmd+shift+f5"
          />
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
        </Field>
      </Section>

      {/* 热词 */}
      <Section title="热词替换" icon="📝" subtitle="识别后自动替换（AI 纠错之后执行）">
        <div className="space-y-2">
          {Object.entries(cfg.hotwords).length === 0 && (
            <div className="text-xs text-gray-500 py-2">暂无热词，点下方添加</div>
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
          <button
            className="btn-ghost w-full justify-center"
            onClick={() => updateHotword(`新热词_${Date.now()}`, "")}
          >
            ＋ 添加热词
          </button>
        </div>
      </Section>

      {/* 日志 */}
      <Section title="日志" icon="📋">
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
          <button className="btn-ghost" onClick={() => api.openLogs()}>打开日志文件</button>
          <button className="btn-ghost" onClick={() => api.openConfigDir()}>打开配置目录</button>
        </div>
      </Section>

    </div>
  );
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

function Section(props: { title: string; icon: string; subtitle?: string; children: React.ReactNode }) {
  return (
    <section className="card">
      <div className="section-title">
        <span className="text-base">{props.icon}</span>
        <span>{props.title}</span>
      </div>
      {props.subtitle && <p className="text-xs text-gray-500 -mt-2 mb-3">{props.subtitle}</p>}
      <div className="space-y-3.5">{props.children}</div>
    </section>
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
  const keys = combo.split("+").map((k) => k.trim().toLowerCase());
  const sym: Record<string, string> = {
    cmd: "⌘", command: "⌘", rcmd: "⌘", rcommand: "⌘",
    shift: "⇧", rshift: "⇧",
    alt: "⌥", option: "⌥", ralt: "⌥", roption: "⌥",
    ctrl: "⌃", control: "⌃", rctrl: "⌃",
  };
  return (
    <div className="ml-auto flex gap-1">
      {keys.map((k, i) => (
        <kbd key={i}>{sym[k] ?? k.toUpperCase()}</kbd>
      ))}
    </div>
  );
}

function LocalSenseVoicePanel() {
  const [available, setAvailable] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [msg, setMsg] = useState("");

  useEffect(() => {
    api.isSenseVoiceAvailable().then(setAvailable);
    const unlisten = listen<number>("sense-voice-download-progress", (e) => {
      setProgress(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const onDownload = async () => {
    setDownloading(true);
    setMsg("");
    setProgress(0);
    try {
      await api.downloadSenseVoice();
      setAvailable(true);
      setMsg("下载完成 ✓");
    } catch (e) {
      setMsg(`下载失败：${e}`);
    } finally {
      setDownloading(false);
    }
  };

  return (
    <div className="rounded-xl bg-white/[0.03] border border-white/[0.06] p-4 space-y-3">
      <div className="flex items-center gap-3">
        <div className={`w-2 h-2 rounded-full ${available ? "bg-green-400" : "bg-amber-400"}`} />
        <span className="text-sm text-gray-300">
          {available ? "模型已就绪" : "模型未下载"}
        </span>
        <span className="text-xs text-gray-500 ml-auto">约 1 GB</span>
      </div>
      {downloading && (
        <div>
          <div className="h-1 bg-bg-900 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-accent to-brand-purple transition-all"
              style={{ width: `${progress * 100}%` }}
            />
          </div>
          <div className="text-[11px] text-gray-500 mt-1 font-mono">
            {Math.round(progress * 100)}%
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <button
          className="btn-primary disabled:opacity-50 text-xs py-1.5"
          disabled={downloading}
          onClick={onDownload}
        >
          {available ? "重新下载" : "下载模型"}
        </button>
        <button className="btn-ghost text-xs py-1.5" onClick={() => api.openConfigDir()}>
          打开模型目录
        </button>
      </div>
      {msg && <div className="text-xs text-gray-400">{msg}</div>}
    </div>
  );
}
