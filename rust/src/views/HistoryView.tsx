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
    <div className="p-8 max-w-4xl mx-auto">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-2xl font-semibold">历史记录</h2>
        <button className="btn-ghost" onClick={onClear}>清空</button>
      </div>

      {loading && <p className="text-gray-500">加载中…</p>}

      {!loading && entries.length === 0 && (
        <p className="text-gray-500 text-center py-12">暂无识别记录</p>
      )}

      <ul className="space-y-2">
        {entries.map((e) => (
          <li
            key={e.id}
            className="card hover:bg-bg-700 cursor-pointer"
            onClick={() => setSelected(e)}
          >
            <div className="flex justify-between items-start mb-1 text-xs text-gray-500">
              <span>{new Date(e.created_at * 1000).toLocaleString()}</span>
              <span>{e.asr_provider}</span>
            </div>
            <div className="text-sm line-clamp-2">{e.corrected_text}</div>
          </li>
        ))}
      </ul>

      {selected && (
        <div
          className="fixed inset-0 bg-black/60 flex items-center justify-center p-4"
          onClick={() => setSelected(null)}
        >
          <div
            className="card max-w-2xl w-full space-y-3"
            onClick={(ev) => ev.stopPropagation()}
          >
            <div className="text-xs text-gray-500">
              {new Date(selected.created_at * 1000).toLocaleString()} · {selected.asr_provider}
            </div>
            <div>
              <div className="label">原文</div>
              <div className="bg-bg-900 rounded-lg p-3 text-sm whitespace-pre-wrap">
                {selected.raw_text}
              </div>
            </div>
            <div>
              <div className="label">最终文字</div>
              <div className="bg-bg-900 rounded-lg p-3 text-sm whitespace-pre-wrap">
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
              <button
                className="btn-ghost text-red-400"
                onClick={() => onDelete(selected.id)}
              >
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
