// 识别结果窗口：独立普通 WebviewWindow（非 NSPanel），让 macOS Accessory 模式下
// 输入法候选词窗能正常挂上——NSPanel + NonactivatingPanel 不激活 app 进程，
// TSM 拿不到 input context，IME 候选窗显示异常。
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";

const textEl = document.getElementById("result-text") as HTMLTextAreaElement;
const copyBtn = document.getElementById("result-copy") as HTMLButtonElement;
const closeBtn = document.getElementById("result-close") as HTMLButtonElement;

// 窗口尺寸约束(logical px),与 result.rs 的 inner_size 常量对齐:宽度始终 520,
// 高度根据 textarea 内容在 [200, 600] 区间自适应
const WIN_W = 520;
const WIN_H_MIN = 200;
const WIN_H_MAX = 600;
// panel 上下 padding + 按钮行 + gap + 一点余量,用来从 textarea 高度算窗口总高
const CHROME_OVERHEAD = 88;
const TEXTAREA_MIN = 56;

const win = getCurrentWebviewWindow();

async function fitHeight() {
  // 先置 auto 让 scrollHeight 反映真实内容,否则它会卡在旧的 style.height 上
  textEl.style.height = "auto";
  const needed = textEl.scrollHeight;
  const textareaMax = WIN_H_MAX - CHROME_OVERHEAD;
  const textareaH = Math.max(TEXTAREA_MIN, Math.min(needed, textareaMax));
  textEl.style.height = `${textareaH}px`;
  const winH = Math.max(WIN_H_MIN, Math.min(WIN_H_MAX, textareaH + CHROME_OVERHEAD));
  try {
    await win.setSize(new LogicalSize(WIN_W, winH));
  } catch {
    // setSize 失败不影响主流程
  }
}

listen<string>("result-show", (e) => {
  textEl.value = e.payload ?? "";
  copyBtn.textContent = "📋 复制";
  copyBtn.classList.remove("copied");
  // 窗口 show 后 focus textarea:macOS 下 first responder 收到 key events 才会
  // 激活 TSM input context,中文 IME 才会挂上候选词窗口
  requestAnimationFrame(() => {
    fitHeight();
    // 长文本撑开后重新居中,否则以左上角为锚向下扩展会偏离屏幕中央
    win.center().catch(() => {});
    textEl.focus();
  });
});

// 用户在 textarea 里编辑时(增/删行)窗口跟着撑开或收缩,但不 center 避免跳动。
// 中文 IME 组字期间(compositionstart..compositionend)跳过 resize —— setSize 会触发
// webview relayout,把 IME 候选词浮窗打断,用户打不出字。组字完成后再 fit 一次兜底。
let isComposing = false;
textEl.addEventListener("compositionstart", () => {
  isComposing = true;
});
textEl.addEventListener("compositionend", () => {
  isComposing = false;
  requestAnimationFrame(fitHeight);
});
textEl.addEventListener("input", () => {
  if (isComposing) return;
  requestAnimationFrame(fitHeight);
});

copyBtn.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(textEl.value);
    copyBtn.textContent = "✓ 已复制";
    copyBtn.classList.add("copied");
  } catch {
    copyBtn.textContent = "复制失败";
  }
});

closeBtn.addEventListener("click", () => {
  invoke("close_result_window").catch(() => {});
});

// Esc 也能关窗
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    invoke("close_result_window").catch(() => {});
  }
});
