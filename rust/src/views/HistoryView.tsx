import { useEffect, useMemo, useState } from "react";
import { api, Config, HistoryEntry, HistoryStats, PolishProfile } from "../api";
import { formatTrigger } from "../lib/hotkey";

export function HistoryView() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [selected, setSelected] = useState<HistoryEntry | null>(null);
  const [loading, setLoading] = useState(true);
  const [profiles, setProfiles] = useState<PolishProfile[]>([]);
  const [activeProfileId, setActiveProfileId] = useState<string>("");
  const [trigger, setTrigger] = useState<{ trigger_mode: string; hotkey: string; double_tap_modifier: string } | null>(null);
  const [stats, setStats] = useState<HistoryStats | null>(null);
  // 重润色相关状态,每次打开 detail 都 reset
  const [repolishProfileId, setRepolishProfileId] = useState<string>("");
  const [repolishResult, setRepolishResult] = useState<string>("");
  const [repolishing, setRepolishing] = useState(false);
  const [repolishError, setRepolishError] = useState<string>("");

  // 过滤出真正能跑的 profile:mode != off 且必要凭证不空
  const runnableProfiles = useMemo(
    () => profiles.filter((p) => p.mode && p.mode !== "off"),
    [profiles],
  );

  const reload = async () => {
    setLoading(true);
    try {
      const [list, s] = await Promise.all([
        api.loadHistory(200),
        api.getHistoryStats().catch(() => null),
      ]);
      setEntries(list);
      setStats(s);
    } finally {
      setLoading(false);
    }
  };

  const loadProfiles = async () => {
    try {
      const cfg: Config = await api.getConfig();
      setProfiles(cfg.polish_profiles ?? []);
      setActiveProfileId(cfg.active_profile_id ?? "");
      setTrigger({
        trigger_mode: cfg.trigger_mode ?? "toggle",
        hotkey: cfg.hotkey ?? "",
        double_tap_modifier: cfg.double_tap_modifier ?? "right_option",
      });
    } catch {
      setProfiles([]);
      setActiveProfileId("");
      setTrigger(null);
    }
  };

  useEffect(() => {
    reload();
    loadProfiles();
  }, []);

  const onDelete = async (id: number) => {
    await api.deleteHistory(id);
    setSelected(null);
    reload();
  };

  const onClear = async () => {
    if (!confirm("确定清空所有历史记录吗？")) return;
    await api.clearHistory();
    reload();
  };

  const onSelect = (e: HistoryEntry) => {
    setSelected(e);
    // 默认选中 active profile(与录音主流程一致);active 不是 runnable 时才回退到第一个
    const active = runnableProfiles.find((p) => p.id === activeProfileId);
    setRepolishProfileId(active?.id ?? runnableProfiles[0]?.id ?? "");
    setRepolishResult("");
    setRepolishError("");
  };

  const onRepolish = async () => {
    if (!selected || !repolishProfileId) return;
    setRepolishing(true);
    setRepolishError("");
    setRepolishResult("");
    try {
      const out = await api.repolishHistory(selected.id, repolishProfileId);
      setRepolishResult(out);
    } catch (err) {
      setRepolishError(String(err));
    } finally {
      setRepolishing(false);
    }
  };

  return (
    <div className="p-10 max-w-3xl mx-auto">
      <div className="flex justify-between items-center mb-5">
        <div>
          <h1 className="text-xl font-semibold text-gray-100">历史记录</h1>
          <p className="text-sm text-gray-500 mt-0.5">最近 200 条识别结果</p>
        </div>
        {entries.length > 0 && (
          <button className="btn-danger" onClick={onClear}>清空</button>
        )}
      </div>

      {stats && stats.total_count > 0 && <StatsPanel stats={stats} />}

      {loading && <div className="text-gray-500 text-sm">加载中…</div>}

      {!loading && entries.length === 0 && (
        <div className="card text-center py-16 text-gray-500 text-sm">
          <div className="text-3xl mb-3 opacity-40">⏱</div>
          暂无识别记录<br />
          <span className="text-xs text-gray-600">
            {trigger ? `按 ${formatTrigger(trigger)} 开始录音` : "按设置的快捷键开始录音"}
          </span>
        </div>
      )}

      <ul className="space-y-1.5">
        {entries.map((e) => (
          <li
            key={e.id}
            className="card hover:bg-bg-700 cursor-pointer transition-colors p-4"
            onClick={() => onSelect(e)}
          >
            <div className="flex justify-between items-start mb-1 text-[11px] text-gray-500 font-mono">
              <span>{formatTime(e.created_at)}</span>
              <div className="flex items-center gap-2">
                {e.duration_ms > 0 && (
                  <span className="text-gray-500">{formatDuration(e.duration_ms)}</span>
                )}
                <span className="px-1.5 py-0.5 rounded bg-white/5">{e.asr_provider}</span>
              </div>
            </div>
            <div className="text-sm line-clamp-2 text-gray-200">{e.corrected_text}</div>
          </li>
        ))}
      </ul>

      {selected && (
        <div
          className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center p-6 z-50"
          onClick={() => setSelected(null)}
        >
          <div
            className="card max-w-xl w-full space-y-4 !p-6"
            onClick={(ev) => ev.stopPropagation()}
          >
            <div className="flex justify-between items-center text-xs text-gray-500 font-mono">
              <span>{formatTime(selected.created_at)}</span>
              <div className="flex items-center gap-2">
                {selected.duration_ms > 0 && (
                  <span>{formatDuration(selected.duration_ms)}</span>
                )}
                <span className="px-1.5 py-0.5 rounded bg-white/5">{selected.asr_provider}</span>
              </div>
            </div>
            <div>
              <div className="label">原文</div>
              <div className="bg-bg-900/60 rounded-xl p-3 text-sm whitespace-pre-wrap border border-white/5">
                {selected.raw_text}
              </div>
            </div>
            <div>
              <div className="label">最终文字</div>
              <div className="bg-bg-900/60 rounded-xl p-3 text-sm whitespace-pre-wrap border border-white/5">
                {selected.corrected_text}
              </div>
            </div>

            {/* 用其他 profile 重新润色:试不同 profile 不用重新说话 */}
            {runnableProfiles.length > 0 && (
              <div>
                <div className="label">用其他 Profile 重新润色</div>
                <div className="flex gap-2 items-center">
                  <select
                    className="input flex-1 !py-1.5"
                    value={repolishProfileId}
                    onChange={(ev) => setRepolishProfileId(ev.target.value)}
                    disabled={repolishing}
                  >
                    {runnableProfiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.id === activeProfileId ? `${p.name}（当前）` : p.name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="btn-ghost"
                    onClick={onRepolish}
                    disabled={repolishing || !repolishProfileId}
                  >
                    {repolishing ? "润色中…" : "重新润色"}
                  </button>
                </div>
                {repolishError && (
                  <div className="mt-2 text-xs text-red-400">{repolishError}</div>
                )}
                {repolishResult && (
                  <div className="mt-2">
                    <div className="bg-bg-900/60 rounded-xl p-3 text-sm whitespace-pre-wrap border border-white/5">
                      {repolishResult}
                    </div>
                    <div className="flex justify-end mt-2">
                      <button
                        className="btn-ghost !py-1 !px-3 text-xs"
                        onClick={() => navigator.clipboard.writeText(repolishResult)}
                      >
                        复制结果
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}

            <div className="flex gap-2 justify-end">
              <button
                className="btn-ghost"
                onClick={() => navigator.clipboard.writeText(selected.corrected_text)}
              >
                复制
              </button>
              <button className="btn-danger" onClick={() => onDelete(selected.id)}>
                删除
              </button>
              <button className="btn-primary" onClick={() => setSelected(null)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function StatsPanel({ stats }: { stats: HistoryStats }) {
  const cards: Array<{ label: string; value: string; hint?: string }> = [
    { label: "总次数", value: stats.total_count.toLocaleString() },
    { label: "总字数", value: stats.total_chars.toLocaleString() },
    {
      label: "口述总时长",
      value: formatLongDuration(stats.total_duration_ms),
    },
    {
      label: "平均字速",
      value:
        stats.avg_chars_per_minute > 0
          ? `${Math.round(stats.avg_chars_per_minute)} 字/分`
          : "—",
      hint: "口述时长 ≥ 0 的记录",
    },
    {
      label: "估计节省",
      value:
        stats.saved_minutes > 0
          ? formatSavedTime(stats.saved_minutes)
          : "—",
      hint: "按打字 40 字/分估算",
    },
  ];
  return (
    <div className="card p-4 mb-4 grid grid-cols-5 gap-3 text-center">
      {cards.map((c) => (
        <div key={c.label}>
          <div className="text-[11px] text-gray-500 mb-1">{c.label}</div>
          <div className="text-base font-semibold text-gray-100 font-mono">
            {c.value}
          </div>
          {c.hint && (
            <div className="text-[10px] text-gray-600 mt-0.5">{c.hint}</div>
          )}
        </div>
      ))}
    </div>
  );
}

function formatLongDuration(ms: number): string {
  const totalSec = Math.round(ms / 1000);
  if (totalSec < 60) return `${totalSec} 秒`;
  const totalMin = Math.round(totalSec / 60);
  if (totalMin < 60) return `${totalMin} 分`;
  const hh = Math.floor(totalMin / 60);
  const mm = totalMin % 60;
  return mm === 0 ? `${hh} 小时` : `${hh} 时 ${mm} 分`;
}

function formatSavedTime(minutes: number): string {
  if (minutes < 60) return `${Math.round(minutes)} 分`;
  const hh = Math.floor(minutes / 60);
  const mm = Math.round(minutes - hh * 60);
  return mm === 0 ? `${hh} 小时` : `${hh} 时 ${mm} 分`;
}

function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000);
  if (total < 60) return `${total}s`;
  const mm = Math.floor(total / 60);
  const ss = total % 60;
  return `${mm}:${String(ss).padStart(2, "0")}`;
}

function formatTime(ts: number): string {
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  const pad = (n: number) => String(n).padStart(2, "0");
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  if (sameDay) return `今天 ${time}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${time}`;
}
