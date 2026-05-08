import { useEffect, useState } from "react";
import { api, AppInfo } from "../api";

const LICENSE_MIT = `MIT License

Copyright (c) 2026 HaoKunT

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

export function AboutView() {
  const [info, setInfo] = useState<AppInfo | null>(null);

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
          <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-accent to-brand-purple flex items-center justify-center text-white text-xl font-bold shadow-glow">
            V
          </div>
          <div>
            <div className="text-lg font-semibold text-gray-100">voice-claude</div>
            <div className="text-sm text-gray-400 font-mono">
              {info ? `v${info.version}` : "…"}
              {info?.debug && <span className="ml-2 text-amber-400">DEBUG</span>}
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

      <section className="card space-y-3">
        <div className="section-title">📄 License</div>
        <pre className="text-[11px] text-gray-400 whitespace-pre-wrap font-mono leading-relaxed bg-bg-900/60 rounded-xl p-4 border border-white/5 max-h-96 overflow-y-auto">
          {LICENSE_MIT}
        </pre>
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
