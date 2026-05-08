import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, ASR_PROVIDERS, CORRECT_MODES, Config, DeviceInfo } from "../api";

export function SettingsView() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [saveMsg, setSaveMsg] = useState<string>("");

  useEffect(() => {
    api.getConfig().then(setCfg);
    api.listDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

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

  const save = async () => {
    try {
      await api.saveConfig(cfg);
      setSaveMsg("已保存 ✓");
      setTimeout(() => setSaveMsg(""), 2000);
    } catch (e) {
      setSaveMsg(`保存失败：${e}`);
    }
  };

  return (
    <div className="p-8 max-w-3xl mx-auto space-y-6">
      <h2 className="text-2xl font-semibold">设置</h2>

      <section className="card space-y-4">
        <h3 className="text-accent font-medium">🎙 语音识别引擎</h3>
        <div>
          <label className="label">识别后端</label>
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
        </div>

        {cfg.asr_provider === "zhipu" && (
          <div>
            <label className="label">智谱 API Key</label>
            <input
              type="password"
              className="input"
              value={cfg.asr_api_key}
              onChange={(e) => update("asr_api_key", e.target.value)}
            />
          </div>
        )}

        {cfg.asr_provider === "xfyun" && (
          <>
            <Field label="App ID" value={cfg.xfyun_app_id} onChange={(v) => update("xfyun_app_id", v)} />
            <Field label="Access Key ID" value={cfg.xfyun_access_key_id} onChange={(v) => update("xfyun_access_key_id", v)} />
            <Field label="Access Key Secret" value={cfg.xfyun_access_key_secret} onChange={(v) => update("xfyun_access_key_secret", v)} password />
          </>
        )}

        {cfg.asr_provider === "volc" && (
          <>
            <Field label="App Key" value={cfg.volc_app_key} onChange={(v) => update("volc_app_key", v)} />
            <Field label="Access Token" value={cfg.volc_access_token} onChange={(v) => update("volc_access_token", v)} password />
            <div>
              <label className="label">识别模型</label>
              <select
                className="input"
                value={cfg.volc_resource_id}
                onChange={(e) => update("volc_resource_id", e.target.value)}
              >
                <option value="volc.seedasr.sauc.duration">volc.seedasr.sauc.duration (2.0)</option>
                <option value="volc.bigasr.sauc.duration">volc.bigasr.sauc.duration (1.0)</option>
              </select>
            </div>
          </>
        )}

        {cfg.asr_provider === "openrouter" && (
          <Field label="OpenRouter API Key" value={cfg.openrouter_api_key} onChange={(v) => update("openrouter_api_key", v)} password />
        )}

        {cfg.asr_provider === "local" && <LocalSenseVoicePanel />}
      </section>

      <section className="card space-y-4">
        <h3 className="text-accent font-medium">🧠 AI 纠错</h3>
        <div>
          <label className="label">纠错模式</label>
          <select
            className="input"
            value={cfg.correct_mode}
            onChange={(e) => update("correct_mode", e.target.value)}
          >
            {CORRECT_MODES.map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
        </div>
        {cfg.correct_mode !== "off" && (
          <>
            <Field label="API 地址" value={cfg.correct_url} onChange={(v) => update("correct_url", v)} />
            <Field label="模型名称" value={cfg.correct_model} onChange={(v) => update("correct_model", v)} />
            <Field label="API Key" value={cfg.correct_api_key} onChange={(v) => update("correct_api_key", v)} password />
            <div>
              <label className="label">超时（秒）</label>
              <input
                type="number"
                className="input"
                value={cfg.correct_timeout}
                onChange={(e) => update("correct_timeout", parseInt(e.target.value) || 10)}
              />
            </div>
          </>
        )}
      </section>

      <section className="card space-y-4">
        <h3 className="text-accent font-medium">🎤 录音参数</h3>
        <div>
          <label className="label">麦克风</label>
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
        </div>
        <Field label="录音快捷键" value={cfg.hotkey} onChange={(v) => update("hotkey", v)} />
        <div>
          <label className="label">信号增益: {cfg.gain}x</label>
          <input
            type="range"
            min={1}
            max={10}
            value={cfg.gain}
            onChange={(e) => update("gain", parseInt(e.target.value))}
            className="w-full"
          />
        </div>
      </section>

      <section className="card space-y-3">
        <h3 className="text-accent font-medium">📝 热词替换</h3>
        <p className="text-xs text-gray-400">识别后自动替换（AI 纠错之后执行）</p>
        <div className="space-y-2">
          {Object.entries(cfg.hotwords).map(([from, to]) => (
            <HotwordRow
              key={from}
              from={from}
              to={to}
              onChange={(k, v) => {
                // 更新 key 或 value
                const hotwords = { ...cfg.hotwords };
                if (k !== from) {
                  delete hotwords[from];
                }
                if (k) hotwords[k] = v;
                setCfg({ ...cfg, hotwords });
              }}
              onDelete={() => deleteHotword(from)}
            />
          ))}
          <button
            className="btn-ghost"
            onClick={() => updateHotword(`新热词_${Date.now()}`, "")}
          >
            ＋ 添加热词
          </button>
        </div>
      </section>

      <section className="card space-y-3">
        <h3 className="text-accent font-medium">📋 日志</h3>
        <div>
          <label className="label">日志级别</label>
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
        </div>
        <div className="flex gap-2">
          <button className="btn-ghost" onClick={() => api.openLogs()}>打开日志文件</button>
          <button className="btn-ghost" onClick={() => api.openConfigDir()}>打开配置目录</button>
        </div>
      </section>

      <div className="flex items-center gap-4 sticky bottom-0 bg-bg-900/80 backdrop-blur py-3">
        <button className="btn-primary" onClick={save}>保存配置</button>
        {saveMsg && <span className="text-sm text-gray-400">{saveMsg}</span>}
      </div>
    </div>
  );
}

function Field(props: { label: string; value: string; onChange: (v: string) => void; password?: boolean }) {
  return (
    <div>
      <label className="label">{props.label}</label>
      <input
        type={props.password ? "password" : "text"}
        className="input"
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      />
    </div>
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
      <span className="text-gray-500">→</span>
      <input
        className="input flex-1"
        placeholder="正确的词"
        value={props.to}
        onChange={(e) => props.onChange(props.from, e.target.value)}
      />
      <button className="btn-ghost px-3" onClick={props.onDelete}>×</button>
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
    <div className="space-y-3">
      <p className="text-sm text-gray-400">
        本地 SenseVoice：离线识别，无需 API Key。点"下载模型（约 1GB）"下载到配置目录后即可直接使用。
      </p>
      <div className="text-sm">
        状态：
        {available ? (
          <span className="text-green-400">模型已就绪 ✓</span>
        ) : (
          <span className="text-amber-400">模型未下载</span>
        )}
      </div>
      {downloading && (
        <div>
          <div className="h-2 bg-bg-700 rounded overflow-hidden">
            <div
              className="h-full bg-accent transition-all"
              style={{ width: `${progress * 100}%` }}
            />
          </div>
          <div className="text-xs text-gray-500 mt-1">
            {Math.round(progress * 100)}%（模型约 1GB，需稳定网络）
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <button
          className="btn-primary disabled:opacity-50"
          disabled={downloading}
          onClick={onDownload}
        >
          {available ? "重新下载" : "下载模型（约 1GB）"}
        </button>
        <button className="btn-ghost" onClick={() => api.openConfigDir()}>
          打开模型目录
        </button>
      </div>
      {msg && <div className="text-xs text-gray-400">{msg}</div>}
    </div>
  );
}
