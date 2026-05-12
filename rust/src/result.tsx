// 识别结果窗口：独立普通 WebviewWindow（非 NSPanel），让 macOS Accessory 模式下
// 输入法候选词窗能正常挂上——NSPanel + NonactivatingPanel 不激活 app 进程，
// TSM 拿不到 input context，IME 候选窗显示异常。
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

const textEl = document.getElementById("result-text") as HTMLTextAreaElement;
const copyBtn = document.getElementById("result-copy") as HTMLButtonElement;
const closeBtn = document.getElementById("result-close") as HTMLButtonElement;

listen<string>("result-show", (e) => {
  textEl.value = e.payload ?? "";
  copyBtn.textContent = "📋 复制";
  copyBtn.classList.remove("copied");
  // 窗口 show 后 focus textarea：macOS 下 first responder 收到 key events 才会
  // 激活 TSM input context，中文 IME 才会挂上候选词窗口
  requestAnimationFrame(() => textEl.focus());
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
