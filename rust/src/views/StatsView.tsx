import { useEffect, useState } from "react";
import { api, LatencyRow, LatencyStats, LatencyWindow } from "../api";

type Range = "all_time" | "last_24h" | "last_7d";

const RANGE_OPTIONS: { value: Range; label: string }[] = [
  { value: "last_24h", label: "近 24 小时" },
  { value: "last_7d", label: "近 7 天" },
  { value: "all_time", label: "全部" },
];

export function StatsView() {
  const [stats, setStats] = useState<LatencyStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [range, setRange] = useState<Range>("last_24h");
  const [error, setError] = useState<string>("");

  const reload = async () => {
    setLoading(true);
    setError("");
    try {
      setStats(await api.getLatencyStats());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    reload();
  }, []);

  const window_: LatencyWindow | null = stats ? stats[range] : null;

  return (
    <div className="p-10 max-w-3xl mx-auto">
      <div className="flex justify-between items-start mb-5">
        <div>
          <h1 className="text-xl font-semibold text-gray-100">状态</h1>
          <p className="text-sm text-gray-500 mt-0.5">
            ASR 和 AI 润色的延时分布,按 provider / model 分组
          </p>
        </div>
        <button className="btn-ghost !py-1 !px-3 text-xs" onClick={reload}>
          刷新
        </button>
      </div>

      <div className="flex gap-1 mb-5">
        {RANGE_OPTIONS.map((o) => (
          <button
            key={o.value}
            className={`btn-ghost !py-1 !px-3 text-xs ${
              range === o.value ? "!bg-white/15 !text-gray-100" : ""
            }`}
            onClick={() => setRange(o.value)}
          >
            {o.label}
          </button>
        ))}
      </div>

      {loading && <div className="text-gray-500 text-sm">加载中…</div>}
      {error && <div className="text-red-400 text-sm">{error}</div>}

      {!loading && window_ && (
        <div className="space-y-6">
          <LatencySection
            title="ASR 识别延时"
            hint="从用户停止说话到拿到最终识别文本的等待时长。流式后端是尾包延时,批处理是整个 HTTP 调用。"
            rows={window_.asr}
            keyHeader="ASR 后端"
          />
          <LatencySection
            title="AI 润色延时"
            hint="调用 LLM 润色的耗时。off profile 不记录。"
            rows={window_.polish}
            keyHeader="Model"
          />
        </div>
      )}
    </div>
  );
}

function LatencySection({
  title,
  hint,
  rows,
  keyHeader,
}: {
  title: string;
  hint: string;
  rows: LatencyRow[];
  keyHeader: string;
}) {
  return (
    <div>
      <h2 className="text-sm font-semibold text-gray-200 mb-1">{title}</h2>
      <p className="text-xs text-gray-500 mb-3">{hint}</p>
      {rows.length === 0 ? (
        <div className="card text-center py-6 text-gray-500 text-xs">
          这个时间段没有数据
        </div>
      ) : (
        <div className="card p-0 overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-white/[0.03] text-[11px] text-gray-500 uppercase tracking-wider">
              <tr>
                <th className="text-left px-4 py-2 font-medium">{keyHeader}</th>
                <th className="text-right px-4 py-2 font-medium">次数</th>
                <th className="text-right px-4 py-2 font-medium">平均</th>
                <th className="text-right px-4 py-2 font-medium">P99</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr
                  key={r.key}
                  className={i % 2 === 0 ? "" : "bg-white/[0.02]"}
                >
                  <td className="px-4 py-2 text-gray-200 font-mono text-xs">
                    {r.key}
                  </td>
                  <td className="px-4 py-2 text-right text-gray-300 font-mono">
                    {r.count.toLocaleString()}
                  </td>
                  <td className="px-4 py-2 text-right text-gray-300 font-mono">
                    {formatMs(r.avg_ms)}
                  </td>
                  <td className="px-4 py-2 text-right text-gray-300 font-mono">
                    {formatMs(r.p99_ms)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}
