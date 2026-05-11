// 录音指示器：Raycast 风格波形 + 实时识别文字 + 录音计时 + 结果编辑
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { parseHotkeyKeys, formatHotkeyKey } from "./lib/hotkey";

// 临时 debug：在悬浮窗左上显示"模块加载 / 最近事件 / 当前 view"，
// 方便线上环境（release 无 devtools）确认 JS 跑到哪
const debugEl = document.createElement("div");
debugEl.style.cssText =
  "position:fixed;top:4px;left:8px;font-size:9px;color:rgba(255,255,255,0.4);font-family:ui-monospace,monospace;z-index:999;pointer-events:none;line-height:1.3;";
document.body.appendChild(debugEl);
const dbgState = { loaded: new Date().toLocaleTimeString(), lastEvent: "-", view: "-" };
function paintDebug() {
  debugEl.textContent = `loaded ${dbgState.loaded}\nevent  ${dbgState.lastEvent}\nview   ${dbgState.view}`;
}
paintDebug();

const BAR_COUNT = 40;
const BAR_WIDTH = 6;
const BAR_GAP = 10;
const history: number[] = Array(BAR_COUNT).fill(0);
let smoothed = 0;
const SMOOTH = 0.65;

const canvas = document.getElementById("wave") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

/// 同步 canvas backing store 尺寸到 CSS 尺寸。必须在 layout 完成后调。
/// 之前 module load 时直接读 clientWidth，新 DOM（多嵌套一层 recording-view）
/// 有时 layout 还没算完导致尺寸为 0，render 全画到空 canvas 上波形不见。
let canvasScaled = false;
function resizeCanvasIfNeeded() {
  const cw = canvas.clientWidth;
  const ch = canvas.clientHeight;
  if (cw === 0 || ch === 0) return;
  const dpr = window.devicePixelRatio || 2;
  const wantedW = cw * dpr;
  const wantedH = ch * dpr;
  // 即便尺寸恰好等于 wanted（retina 屏 HTML attribute 和 CSS*dpr 巧合相等
  // 的边界情况），首次 resize 仍必须跑一遍 scale，否则 ctx 的 transform 是
  // 单位矩阵，画出来的图会按 backing store 坐标显示成 1/dpr 尺寸
  if (canvasScaled && canvas.width === wantedW && canvas.height === wantedH) return;
  canvas.width = wantedW;
  canvas.height = wantedH;
  ctx.resetTransform();
  ctx.scale(dpr, dpr);
  canvasScaled = true;
}

// ==== 波形渲染 ====

function render() {
  resizeCanvasIfNeeded();
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (w === 0 || h === 0) return;
  ctx.clearRect(0, 0, w, h);

  const totalW = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP;
  const startX = (w - totalW) / 2;
  const centerY = h / 2;

  for (let i = 0; i < BAR_COUNT; i++) {
    const level = history[i];
    const barH = Math.max(4, Math.sqrt(level) * (h - 16));
    const x = startX + i * (BAR_WIDTH + BAR_GAP);
    const y = centerY - barH / 2;

    const positionRatio = i / (BAR_COUNT - 1);
    const grad = ctx.createLinearGradient(x, y, x, y + barH);
    const { r, g, b } = raycastColor(positionRatio, level);
    grad.addColorStop(0, `rgba(${r}, ${g}, ${b}, 0.95)`);
    grad.addColorStop(1, `rgba(${r}, ${g}, ${b}, 0.6)`);

    ctx.shadowColor = `rgba(${r}, ${g}, ${b}, ${level * 0.8})`;
    ctx.shadowBlur = 10;

    ctx.fillStyle = grad;
    ctx.beginPath();
    const radius = 3;
    ctx.moveTo(x + radius, y);
    ctx.arcTo(x + BAR_WIDTH, y, x + BAR_WIDTH, y + barH, radius);
    ctx.arcTo(x + BAR_WIDTH, y + barH, x, y + barH, radius);
    ctx.arcTo(x, y + barH, x, y, radius);
    ctx.arcTo(x, y, x + BAR_WIDTH, y, radius);
    ctx.closePath();
    ctx.fill();

    ctx.shadowBlur = 0;
  }
}

function raycastColor(pos: number, level: number): { r: number; g: number; b: number } {
  const centerDist = Math.abs(pos - 0.5) * 2;
  const baseR = 255 - 100 * centerDist;
  const baseG = 92 + 43 * centerDist;
  const baseB = 92 + 153 * centerDist;
  const warm = level;
  const r = Math.round(baseR * (0.6 + 0.4 * warm));
  const g = Math.round(baseG * 0.9);
  const b = Math.round(baseB * (1 - 0.4 * warm));
  return { r, g, b };
}

// ==== 录音计时器 ====

const timerEl = document.getElementById("timer") as HTMLElement;
const statusTextEl = document.getElementById("status-text") as HTMLElement;
const partialEl = document.getElementById("partial") as HTMLElement;
const recordingViewEl = document.getElementById("recording-view") as HTMLElement;
const processingViewEl = document.getElementById("processing-view") as HTMLElement;
const resultViewEl = document.getElementById("result-view") as HTMLElement;
const resultTextEl = document.getElementById("result-text") as HTMLTextAreaElement;
const hotkeyHintEl = document.getElementById("hotkey-hint") as HTMLElement;
const copyBtn = document.getElementById("result-copy") as HTMLButtonElement;
const closeBtn = document.getElementById("result-close") as HTMLButtonElement;

function showView(which: "recording" | "processing" | "result") {
  recordingViewEl.hidden = which !== "recording";
  processingViewEl.hidden = which !== "processing";
  resultViewEl.hidden = which !== "result";
  dbgState.view = which;
  paintDebug();
}

function renderHotkeyHint(hotkey: string) {
  hotkeyHintEl.innerHTML = "";
  for (const k of parseHotkeyKeys(hotkey)) {
    const kbd = document.createElement("kbd");
    kbd.textContent = formatHotkeyKey(k);
    hotkeyHintEl.appendChild(kbd);
  }
}

copyBtn.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(resultTextEl.value);
    copyBtn.textContent = "✓ 已复制";
    copyBtn.classList.add("copied");
  } catch {
    copyBtn.textContent = "复制失败";
  }
});

closeBtn.addEventListener("click", () => {
  invoke("close_indicator").catch(() => {});
});

let startedAt = 0;
let timerHandle: number | null = null;

function startTimer() {
  startedAt = Date.now();
  updateTimerDisplay();
  if (timerHandle != null) clearInterval(timerHandle);
  timerHandle = window.setInterval(updateTimerDisplay, 250);
}

function stopTimer() {
  if (timerHandle != null) {
    clearInterval(timerHandle);
    timerHandle = null;
  }
}

function updateTimerDisplay() {
  if (!startedAt) return;
  const sec = Math.floor((Date.now() - startedAt) / 1000);
  const mm = String(Math.floor(sec / 60)).padStart(2, "0");
  const ss = String(sec % 60).padStart(2, "0");
  timerEl.textContent = `${mm}:${ss}`;
  // 超过 60 秒时黄色提示（临时录音通常 < 30 秒）
  if (sec >= 60) {
    timerEl.style.color = "rgba(250, 200, 100, 0.95)";
  }
}

// ==== 事件订阅 ====

listen<number>("audio-level", (e) => {
  const raw = Math.max(0, Math.min(1, e.payload));
  smoothed = smoothed * SMOOTH + raw * (1 - SMOOTH);
  history.shift();
  history.push(smoothed);
});

listen<string>("asr-partial", (e) => {
  const text = e.payload?.trim() ?? "";
  if (text) {
    partialEl.textContent = text;
    partialEl.classList.remove("empty");
  }
});

interface RecordingStartedPayload {
  hotkey: string;
}

listen<RecordingStartedPayload>("recording-started", (e) => {
  dbgState.lastEvent = `started @ ${new Date().toLocaleTimeString()}`;
  paintDebug();
  showView("recording");
  startTimer();
  statusTextEl.textContent = "录音中";
  partialEl.textContent = "等待语音…";
  partialEl.classList.add("empty");
  timerEl.style.color = "";
  renderHotkeyHint(e.payload?.hotkey ?? "cmd+shift+f5");
});

listen("recording-stopped", () => {
  dbgState.lastEvent = `stopped @ ${new Date().toLocaleTimeString()}`;
  paintDebug();
  stopTimer();
  // 录音结束 → 进入润色 / 处理中态；input 模式下很快就会被 hide 一闪而过
  // panel 模式下会等 asr-final-text 到达再切 result
  showView("processing");
});

listen<string>("asr-final-text", (e) => {
  dbgState.lastEvent = `final @ ${new Date().toLocaleTimeString()}`;
  paintDebug();
  resultTextEl.value = e.payload ?? "";
  showView("result");
  copyBtn.textContent = "📋 复制";
  copyBtn.classList.remove("copied");
});

// 页面加载时假设已经进入录音（悬浮窗只在录音时显示）
startTimer();

function loop() {
  render();
  requestAnimationFrame(loop);
}
loop();
