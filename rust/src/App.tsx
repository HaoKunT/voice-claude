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
    <div className="min-h-screen flex">
      <nav className="w-48 bg-bg-800 border-r border-white/5 py-6 px-3 flex flex-col gap-1">
        <h1 className="text-lg font-semibold px-3 mb-4 text-accent">voice-claude</h1>
        <NavItem label="设置" active={route === "settings"} onClick={() => setRoute("settings")} />
        <NavItem label="历史记录" active={route === "history"} onClick={() => setRoute("history")} />
      </nav>
      <main className="flex-1 overflow-y-auto">
        {route === "settings" && <SettingsView />}
        {route === "history" && <HistoryView />}
      </main>
    </div>
  );
}

function NavItem(props: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      className={
        "text-left px-3 py-2 rounded-lg text-sm transition-colors " +
        (props.active ? "bg-accent text-white" : "hover:bg-bg-700 text-gray-300")
      }
    >
      {props.label}
    </button>
  );
}

export default App;
