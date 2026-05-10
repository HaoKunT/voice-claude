const IS_MAC =
  typeof navigator !== "undefined" && /mac|iphone|ipad|ipod/i.test(navigator.platform);

const SYMBOLS_MAC: Record<string, string> = {
  cmd: "⌘", command: "⌘", rcmd: "⌘", rcommand: "⌘",
  shift: "⇧", rshift: "⇧",
  alt: "⌥", option: "⌥", ralt: "⌥", roption: "⌥",
  ctrl: "⌃", control: "⌃", rctrl: "⌃",
  space: "Space",
  enter: "Enter", return: "Enter",
  tab: "Tab",
  esc: "Esc", escape: "Esc",
  delete: "Delete",
  backspace: "Backspace",
};

const SYMBOLS_WIN: Record<string, string> = {
  cmd: "Ctrl", command: "Ctrl", rcmd: "Ctrl", rcommand: "Ctrl",
  ctrl: "Ctrl", control: "Ctrl", rctrl: "Ctrl",
  alt: "Alt", option: "Alt", ralt: "Alt", roption: "Alt",
  shift: "Shift", rshift: "Shift",
  win: "Win", super: "Win", meta: "Win",
  space: "Space",
  enter: "Enter", return: "Enter",
  tab: "Tab",
  esc: "Esc", escape: "Esc",
  delete: "Delete",
  backspace: "Backspace",
};

const SYMBOLS: Record<string, string> = IS_MAC ? SYMBOLS_MAC : SYMBOLS_WIN;

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
