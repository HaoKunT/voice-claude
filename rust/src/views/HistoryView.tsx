import { useEffect, useState } from "react";
import { api, HistoryEntry } from "../api";

export function HistoryView() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [selected, setSelected] = useState<HistoryEntry | null>(null);
  const [loading, setLoading] = useState(true);

  const reload = async () => {
    setLoading(true);
    try {
      const list = await api.loadHistory(200);
      setEntries(list);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    reload();
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

      {loading && <div className="text-gray-500 text-sm">加载中…</div>}

      {!loading && entries.length === 0 && (
        <div className="card text-center py-16 text-gray-500 text-sm">
          <div className="text-3xl mb-3 opacity-40">⏱</div>
          暂无识别记录<br />
          <span className="text-xs text-gray-600">按 Cmd+Shift+F5 开始录音</span>
        </div>
      )}

      <ul className="space-y-1.5">
        {entries.map((e) => (
          <li
            key={e.id}
            className="card hover:bg-bg-700 cursor-pointer transition-colors p-4"
            onClick={() => setSelected(e)}
          >
            <div className="flex justify-between items-start mb-1 text-[11px] text-gray-500 font-mono">
              <span>{formatTime(e.created_at)}</span>
              <span className="px-1.5 py-0.5 rounded bg-white/5">{e.asr_provider}</span>
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
              <span className="px-1.5 py-0.5 rounded bg-white/5">{selected.asr_provider}</span>
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
