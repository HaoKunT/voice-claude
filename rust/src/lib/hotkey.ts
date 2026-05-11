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

// 所有 modifier 的小写名字；和 Rust hotkey.rs 里认的保持一致
const MODIFIERS = new Set([
  "cmd", "command", "rcmd", "rcommand",
  "ctrl", "control", "rctrl",
  "alt", "option", "ralt", "roption",
  "shift", "rshift",
  "win", "super", "meta",
]);

/// 前端实时校验，和 Rust hotkey::to_tauri_shortcut 的核心约束对齐：
/// - 至少有 2 段（mod+key）
/// - 至少有一个非 modifier 的主键
/// 返回 null 表示通过，否则返回人话错误提示。
export function validateHotkey(combo: string): string | null {
  const keys = parseHotkeyKeys(combo);
  if (keys.length === 0) return "快捷键不能为空";
  if (keys.length < 2) return "至少要有一个修饰键（Cmd/Ctrl/Shift/Alt）+ 一个主键";
  const nonMods = keys.filter((k) => !MODIFIERS.has(k));
  if (nonMods.length === 0) {
    return "缺少主键：需要 F1–F24、A–Z、0–9 或 Space/Enter/Tab/Esc 等";
  }
  return null;
}
