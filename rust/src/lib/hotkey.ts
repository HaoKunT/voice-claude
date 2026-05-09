const SYMBOLS: Record<string, string> = {
  cmd: "⌘", command: "⌘", rcmd: "⌘", rcommand: "⌘",
  shift: "⇧", rshift: "⇧",
  alt: "⌥", option: "⌥", ralt: "⌥", roption: "⌥",
  ctrl: "⌃", control: "⌃", rctrl: "⌃",
};

export function parseHotkeyKeys(combo: string): string[] {
  return combo
    .split("+")
    .map((k) => k.trim().toLowerCase())
    .filter(Boolean);
}

export function formatHotkeyKey(key: string): string {
  return SYMBOLS[key] ?? key.toUpperCase();
}

/// 把 "cmd+shift+f5" 格式化成 "⌘⇧F5"，用在文本里（非 <kbd>）
export function formatHotkey(combo: string): string {
  return parseHotkeyKeys(combo).map(formatHotkeyKey).join("");
}
