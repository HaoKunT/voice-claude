import { useEffect, useState } from "react";
import { api, AppInfo } from "../api";
import { saveTextToFile, readTextFromFile } from "../lib/fileDialogHelpers";
import { useUpdate } from "../contexts/UpdateContext";

export function AboutView() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const {
    hasUpdate,
    updateInfo,
    phase,
    progress,
    downloadedBytes,
    totalBytes,
    error,
    downloadAndInstall,
    relaunch,
  } = useUpdate();

  useEffect(() => {
    api.getAppInfo().then(setInfo);
  }, []);

  return (
    <div className="p-10 max-w-3xl mx-auto space-y-5">
      <div>
        <h1 className="text-xl font-semibold text-gray-100">关于</h1>
        <p className="text-sm text-gray-500 mt-0.5">版本信息与开源许可</p>
      </div>

      <section className="card space-y-4">
        <div className="flex items-center gap-3">
          <img
            src="/app-icon.png"
            alt="voice-claude"
            className="w-12 h-12 rounded-xl shadow-glow"
          />
          <div className="flex-1">
            <div className="text-lg font-semibold text-gray-100">voice-claude</div>
            <div className="text-sm text-gray-400 font-mono flex items-center gap-2">
              <span>{info ? `v${info.version}` : "…"}</span>
              {info?.debug && <span className="text-amber-400">DEBUG</span>}
              {hasUpdate && updateInfo && (
                <span className="px-2 py-0.5 rounded-full bg-green-400/10 text-green-400 text-[11px] border border-green-400/20">
                  有新版 v{updateInfo.availableVersion}
                </span>
              )}
            </div>
          </div>
        </div>

        {info && (
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm font-mono">
            <InfoRow label="Version" value={info.version} />
            <InfoRow
              label="Commit"
              value={`${info.git_hash}${info.git_dirty === "dirty" ? " (dirty)" : ""}`}
            />
            <InfoRow label="Target" value={info.target} />
            <InfoRow label="Rust" value={info.rustc_version} />
            <InfoRow label="Tauri" value={info.tauri_version} />
            <InfoRow label="Build time" value={formatBuildTime(info.build_time)} />
          </dl>
        )}

        <div className="pt-3 border-t border-white/5 flex flex-wrap items-center gap-2">
          <a
            href="https://github.com/HaoKunT/voice-claude"
            target="_blank"
            rel="noreferrer"
            className="btn-ghost text-xs py-1.5 gap-1.5"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
              <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"/>
            </svg>
            GitHub 仓库
          </a>
          <a
            href="https://github.com/HaoKunT/voice-claude/stargazers"
            target="_blank"
            rel="noreferrer"
            className="btn-ghost text-xs py-1.5 gap-1.5"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
              <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z"/>
            </svg>
            Star 支持
          </a>
          <a
            href="https://github.com/HaoKunT/voice-claude/releases"
            target="_blank"
            rel="noreferrer"
            className="btn-ghost text-xs py-1.5"
          >
            Releases
          </a>
          <a
            href="https://github.com/HaoKunT/voice-claude/issues/new"
            target="_blank"
            rel="noreferrer"
            className="btn-ghost text-xs py-1.5"
          >
            反馈问题
          </a>
        </div>
      </section>

      {hasUpdate && updateInfo && (
        <section className="card space-y-3 border-green-400/30 bg-green-400/[0.02]">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="section-title !mb-0">✨ 新版本可用</div>
              <div className="text-xs text-gray-500 mt-1 font-mono">
                v{updateInfo.currentVersion} → v{updateInfo.availableVersion}
                {updateInfo.pubDate && (
                  <span className="ml-2 text-gray-600">· {formatBuildTime(updateInfo.pubDate)}</span>
                )}
              </div>
            </div>
            {phase === "idle" && (
              <button className="btn-primary text-xs py-1.5" onClick={() => downloadAndInstall()}>
                下载并安装
              </button>
            )}
            {phase === "finished" && (
              <button className="btn-primary text-xs py-1.5" onClick={() => relaunch()}>
                立即重启
              </button>
            )}
          </div>
          {updateInfo.notes && (
            <pre className="text-xs text-gray-400 bg-bg-900/60 rounded-lg p-3 whitespace-pre-wrap font-sans border border-white/5 max-h-40 overflow-y-auto">
              {updateInfo.notes}
            </pre>
          )}
          {(phase === "downloading" || phase === "installing") && (
            <div>
              <div className="h-1 bg-bg-900 rounded-full overflow-hidden">
                <div
                  className="h-full bg-gradient-to-r from-accent to-brand-purple transition-all"
                  style={{ width: `${progress * 100}%` }}
                />
              </div>
              <div className="text-[11px] text-gray-500 mt-1 font-mono flex justify-between">
                <span>
                  {phase === "installing"
                    ? "安装中…"
                    : `${Math.round(progress * 100)}% · ${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}`}
                </span>
              </div>
            </div>
          )}
          {phase === "finished" && (
            <div className="text-xs text-green-400">✓ 下载完成，点右侧按钮重启</div>
          )}
          {phase === "error" && error && (
            <div className="text-xs text-red-400">下载失败：{error}</div>
          )}
        </section>
      )}

      <section className="card space-y-2">
        <div className="section-title">📄 License</div>
        <div className="text-sm text-gray-300">
          MIT License ·{" "}
          <a
            href="https://github.com/HaoKunT/voice-claude/blob/main/LICENSE"
            target="_blank"
            rel="noreferrer"
            className="text-brand-blue hover:underline"
          >
            查看完整许可证
          </a>
        </div>
        <p className="text-xs text-gray-500">
          Copyright © 2026 HaoKunT. 你可以自由使用、修改、分发本软件，包括商业用途，但需保留版权声明。
        </p>
      </section>

      <section className="card space-y-2">
        <div className="section-title">🔄 配置备份</div>
        <p className="text-xs text-gray-400 leading-relaxed">
          把全部设置（profile / 热词 / 快捷键 / ASR API Key 等）导出为 JSON，换机器 / 备份 / 分享配置时用；导入会覆盖当前全部配置。
        </p>
        <div className="flex gap-2 mt-2">
          <button className="btn-ghost text-xs py-1.5" onClick={() => handleExportConfig()}>
            📤 导出当前配置
          </button>
          <button className="btn-ghost text-xs py-1.5" onClick={() => handleImportConfig()}>
            📥 导入配置
          </button>
        </div>
      </section>

      <section className="card space-y-2">
        <div className="section-title">🔐 签名与权限</div>
        <p className="text-xs text-gray-400 leading-relaxed">
          本版本为 <span className="font-mono text-gray-300">ad-hoc</span> 签名（未购买 Apple Developer 证书）。
          macOS 的系统权限绑在 code signature 指纹上，所以
          <span className="text-amber-400">每次自动更新后，「辅助功能」与「麦克风」权限通常需要重新授权一次</span>。
          这是 macOS 系统级限制，不是 bug。
          {" "}
          <a
            href="https://github.com/HaoKunT/voice-claude#已知限制macos-ad-hoc-签名"
            target="_blank"
            rel="noreferrer"
            className="text-brand-blue hover:underline"
          >
            详细说明
          </a>
        </p>
      </section>

      <section className="card space-y-3">
        <div className="section-title">📦 主要依赖</div>
        <ul className="text-xs text-gray-400 space-y-1.5">
          <Dep name="Tauri" version={info?.tauri_version} url="https://tauri.app" />
          <Dep name="React" version="19" url="https://react.dev" />
          <Dep name="tokio" url="https://tokio.rs" />
          <Dep name="cpal" url="https://github.com/RustAudio/cpal" desc="跨平台录音" />
          <Dep name="enigo" url="https://github.com/enigo-rs/enigo" desc="键盘模拟" />
          <Dep name="sherpa-onnx" url="https://github.com/k2-fsa/sherpa-onnx" desc="离线 ASR（SenseVoice）" />
          <Dep name="tauri-nspanel" url="https://github.com/ahkohd/tauri-nspanel" desc="macOS NSPanel 不抢焦点" />
          <Dep name="rusqlite" url="https://github.com/rusqlite/rusqlite" desc="SQLite 历史记录" />
        </ul>
      </section>
    </div>
  );
}

const JSON_FILTERS = [{ name: "JSON", extensions: ["json"] }];

async function handleExportConfig() {
  try {
    const json = await api.exportConfig();
    const defaultName = `voice-claude-config-${new Date().toISOString().slice(0, 10)}.json`;
    if (await saveTextToFile(json, defaultName, JSON_FILTERS)) {
      alert("配置已导出 ✓");
    }
  } catch (e) {
    alert(`导出失败：${e}`);
  }
}

async function handleImportConfig() {
  try {
    const json = await readTextFromFile(JSON_FILTERS);
    if (json === null) return;
    if (
      !confirm(
        "导入会覆盖当前全部配置（profile / 热词 / 快捷键 / API Key 等）。\n\n继续？",
      )
    ) {
      return;
    }
    await api.importConfig(json);
    alert("配置已导入 ✓\n\n建议关闭应用重新打开，确保所有窗口状态同步。");
  } catch (e) {
    alert(`导入失败：${e}`);
  }
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-gray-500">{label}</dt>
      <dd className="text-gray-300">{value}</dd>
    </>
  );
}

function Dep({ name, version, url, desc }: { name: string; version?: string; url: string; desc?: string }) {
  return (
    <li className="flex items-baseline gap-2">
      <a href={url} target="_blank" rel="noreferrer" className="text-brand-blue hover:underline">
        {name}
      </a>
      {version && <span className="text-gray-600 font-mono">{version}</span>}
      {desc && <span className="text-gray-500">— {desc}</span>}
    </li>
  );
}

function formatBuildTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("zh-CN", { hour12: false });
  } catch {
    return iso;
  }
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
