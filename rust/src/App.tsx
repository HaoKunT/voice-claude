import { useEffect, useState } from "react";
import { SettingsView } from "./views/SettingsView";
import { HistoryView } from "./views/HistoryView";

type Route = "settings" | "history";

function useHashRoute(): [Route, (r: Route) => void] {
  const read = (): Route => {
    if (window.location.hash === "#/history") return "history";
    return "settings";
  };
  const [route, setRoute] = useState<Route>(read);
  useEffect(() => {
    const onHash = () => setRoute(read());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return [route, (r: Route) => (window.location.hash = r === "history" ? "#/history" : "#/")];
}

function App() {
  const [route, setRoute] = useHashRoute();

  return (
    <div className="min-h-screen flex flex-col">
      {/* 顶部彩色细光条 Raycast 招牌 */}
      <div className="h-[2px] top-accent flex-shrink-0" />
      <div className="flex-1 flex">
        <nav className="w-56 bg-bg-800/60 border-r border-white/[0.06] py-5 px-3 flex flex-col gap-0.5 backdrop-blur-heavy">
          <div className="px-3 mb-5 flex items-center gap-2">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-accent to-brand-purple flex items-center justify-center text-white text-sm font-bold shadow-glow">
              V
            </div>
            <div>
              <div className="text-[13px] font-semibold text-gray-100">voice-claude</div>
              <div className="text-[10px] text-gray-500">按 Cmd+Shift+F5</div>
            </div>
          </div>
          <NavItem icon="⚙" label="设置" active={route === "settings"} onClick={() => setRoute("settings")} />
          <NavItem icon="⏱" label="历史记录" active={route === "history"} onClick={() => setRoute("history")} />
        </nav>
        <main className="flex-1 overflow-y-auto">
          {route === "settings" && <SettingsView />}
          {route === "history" && <HistoryView />}
        </main>
      </div>
    </div>
  );
}

function NavItem(props: {
  icon: string;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      className={`nav-item ${props.active ? "nav-item-active" : "nav-item-inactive"}`}
    >
      <span className="w-5 text-center text-base opacity-80">{props.icon}</span>
      {props.label}
    </button>
  );
}

export default App;
