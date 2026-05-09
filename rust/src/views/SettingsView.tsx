import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, ASR_PROVIDERS, POLISH_MODES, Config, DeviceInfo, PolishProfile } from "../api";

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
            <TextField label="OpenRouter API Key" value={cfg.openrouter_api_key} onChange={(v) => update("openrouter_api_key", v)} password />
          )}

          {cfg.asr_provider === "local" && <LocalSenseVoicePanel />}
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
                      <span>音量触发阈值</span>
                      <span className="ml-auto font-mono text-accent">
                        {cfg.vad_threshold.toFixed(3)}
                      </span>
                    </>
                  }
                >
                  <input
                    type="range"
                    min={5}
                    max={50}
                    step={1}
                    value={Math.round(cfg.vad_threshold * 1000)}
                    onChange={(e) =>
                      update("vad_threshold", parseInt(e.target.value) / 1000)
                    }
                    className="w-full accent-accent"
                  />
                  <div className="flex justify-between text-[10px] text-gray-600 mt-1 font-mono">
                    <span>0.005（安静环境）</span>
                    <span>0.015</span>
                    <span>0.050（嘈杂环境）</span>
                  </div>
                  <p className="text-[11px] text-gray-500 leading-relaxed mt-1.5">
                    如果在吵的环境里 VAD 不停，调高；如果说话不够响被误判为静音提前停了，调低。
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
            <button className="btn-ghost" onClick={() => api.openLogs()}>打开日志文件</button>
            <button className="btn-ghost" onClick={() => api.openConfigDir()}>打开配置目录</button>
          </div>
        </section>
      )}
    </div>
  );
}

async function handleExportCsv() {
  try {
    const csv = await api.exportHotwordsCsv();
    const path = await saveDialog({
      defaultPath: `voice-claude-hotwords-${new Date().toISOString().slice(0, 10)}.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    // 用 Rust 写文件（前端直接写有权限问题）
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    await writeTextFile(path, csv);
  } catch (e) {
    alert(`导出失败：${e}`);
  }
}

async function handleImportCsv(setCfg: (c: Config) => void, cfg: Config) {
  try {
    const selected = await openDialog({
      filters: [{ name: "CSV", extensions: ["csv"] }],
      multiple: false,
    });
    if (!selected || typeof selected !== "string") return;

    const merge = confirm(
      "选择「确定」合并到现有热词\n选择「取消」用 CSV 完全替换现有热词",
    );

    const { readTextFile } = await import("@tauri-apps/plugin-fs");
    const csv = await readTextFile(selected);
    const added = await api.importHotwordsCsv(csv, merge);
    alert(`已导入 ${added} 条热词`);
    // 刷新
    const latest = await api.getConfig();
    setCfg(latest);
    // cfg 引用保持一致（防止 useEffect 自动保存覆盖）
    void cfg;
  } catch (e) {
    alert(`导入失败：${e}`);
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
        <button
          className="w-full py-2.5 rounded-xl bg-white/[0.03] border border-dashed border-white/10 text-gray-400 hover:bg-white/[0.05] hover:text-gray-200 hover:border-white/20 transition text-sm"
          onClick={addProfile}
        >
          ＋ 添加 Profile
        </button>
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
