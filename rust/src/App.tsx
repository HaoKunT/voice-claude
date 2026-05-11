import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Config } from "./api";
import { formatHotkey } from "./lib/hotkey";
import { SettingsView, SettingsSection } from "./views/SettingsView";
import { HistoryView } from "./views/HistoryView";
import { AboutView } from "./views/AboutView";
import { UpdateProvider, useUpdate } from "./contexts/UpdateContext";

type Route =
  | "asr"
  | "polish"
  | "record"
  | "hotwords"
  | "log"
  | "history"
  | "about";

const ROUTE_HASHES: Record<Route, string> = {
  asr: "#/",
  polish: "#/polish",
  record: "#/record",
  hotwords: "#/hotwords",
  log: "#/log",
  history: "#/history",
  about: "#/about",
};

const NAV_ITEMS: { route: Route; icon: string; label: string }[] = [
  { route: "asr", icon: "🎙", label: "语音识别" },
  { route: "polish", icon: "🧠", label: "AI 润色" },
  { route: "record", icon: "🎤", label: "录音参数" },
  { route: "hotwords", icon: "📝", label: "热词替换" },
  { route: "log", icon: "📋", label: "日志" },
  { route: "history", icon: "⏱", label: "历史记录" },
  { route: "about", icon: "ℹ", label: "关于" },
];

function useHashRoute(): [Route, (r: Route) => void] {
  const read = (): Route => {
    const h = window.location.hash;
    for (const [r, path] of Object.entries(ROUTE_HASHES)) {
      if (h === path) return r as Route;
    }
    return "asr";
  };
  const [route, setRoute] = useState<Route>(read);
  useEffect(() => {
    const onHash = () => setRoute(read());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return [route, (r: Route) => (window.location.hash = ROUTE_HASHES[r])];
}

function Shell() {
  const [route, setRoute] = useHashRoute();
  const { hasUpdate } = useUpdate();
  const [cfg, setCfg] = useState<Config | null>(null);

  useEffect(() => {
    api.getConfig().then(setCfg);
    // 监听 save_config 后的广播，保持 sidebar 显示和当前 hotkey 同步
    const unlisten = listen("config-updated", () => {
      api.getConfig().then(setCfg);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="min-h-screen flex flex-col">
      {/* 顶部彩色细光条 Raycast 招牌 */}
      <div className="h-[2px] top-accent flex-shrink-0" />
      <AccessibilityBanner />
      <div className="flex-1 flex">
        <nav className="w-56 bg-bg-800/60 border-r border-white/[0.06] py-5 px-3 flex flex-col gap-0.5 backdrop-blur-heavy">
          <div className="px-3 mb-5 flex items-center gap-2">
            <img
              src="/app-icon.png"
              alt="voice-claude"
              className="w-7 h-7 rounded-lg shadow-glow"
            />
            <div>
              <div className="text-[13px] font-semibold text-gray-100">voice-claude</div>
              <div className="text-[10px] text-gray-500">
                按 {cfg ? formatHotkey(cfg.hotkey) : "…"}
              </div>
            </div>
          </div>
          {NAV_ITEMS.map((item) => (
            <NavItem
              key={item.route}
              icon={item.icon}
              label={item.label}
              active={route === item.route}
              badge={item.route === "about" && hasUpdate}
              onClick={() => setRoute(item.route)}
            />
          ))}
        </nav>
        <main className="flex-1 overflow-y-auto">
          {route === "history" && <HistoryView />}
          {route === "about" && <AboutView />}
          {route !== "history" && route !== "about" && (
            <SettingsView section={route as SettingsSection} />
          )}
        </main>
      </div>
    </div>
  );
}

function App() {
  return (
    <UpdateProvider>
      <Shell />
    </UpdateProvider>
  );
}

function AccessibilityBanner() {
  const [granted, setGranted] = useState<boolean | null>(null);

  const check = useCallback(async () => {
    try {
      setGranted(await api.checkAccessibility());
    } catch {
      setGranted(true); // 非 macOS / 命令异常，按已授权处理，不显示横条
    }
  }, []);

  useEffect(() => {
    check();
    // 窗口重新 focus 时再查一次——覆盖"用户跳系统设置勾完回来" 的场景
    const onFocus = () => check();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [check]);

  if (granted !== false) return null;

  const openSettings = () => api.openAccessibilitySettings();

  return (
    <div className="flex-shrink-0 bg-amber-500/15 border-b border-amber-500/30 px-4 py-2.5 flex items-center gap-3">
      <span className="text-amber-400 text-base">⚠</span>
      <div className="flex-1 text-xs text-amber-100 leading-snug">
        <span className="font-medium">辅助功能权限未生效</span>
        <span className="text-amber-200/70 ml-2">
          —— 热键不会工作。如果系统设置里 voice-claude 看起来已经勾选，那是升级后 macOS 缓存了旧签名：
          <b className="text-amber-100">先取消勾选、再重新勾选</b>一次即可。
        </span>
      </div>
      <button
        className="btn-ghost text-xs py-1 px-3 bg-amber-500/20 text-amber-100 hover:bg-amber-500/30"
        onClick={openSettings}
      >
        去授权
      </button>
      <button className="btn-ghost text-xs py-1 px-3" onClick={check}>
        已授权，重新检查
      </button>
    </div>
  );
}

function NavItem(props: {
  icon: string;
  label: string;
  active: boolean;
  badge?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      className={`nav-item ${props.active ? "nav-item-active" : "nav-item-inactive"}`}
    >
      <span className="w-5 text-center text-base opacity-80">{props.icon}</span>
      <span className="flex-1 text-left">{props.label}</span>
      {props.badge && (
        <span
          className="w-1.5 h-1.5 rounded-full bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.6)]"
          aria-label="有新版本"
        />
      )}
    </button>
  );
}

export default App;
