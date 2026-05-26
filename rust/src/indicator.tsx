// 录音指示器：Raycast 风格波形 + 实时识别文字 + 录音计时。
// panel 输出模式的识别结果展示/编辑由独立的 result 窗口负责（见 result.tsx）。
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { parseHotkeyKeys, formatHotkeyKey, formatDoubleTapModifier } from "./lib/hotkey";

const BAR_COUNT = 40;
const BAR_WIDTH = 6;
const BAR_GAP = 10;
const history: number[] = Array(BAR_COUNT).fill(0);
let smoothed = 0;
const SMOOTH = 0.65;

const canvas = document.getElementById("wave") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

/// 同步 canvas backing store 尺寸到 CSS 尺寸。在 render loop 里每帧调；
/// 尺寸没变就跳过，避免重复 resize 清空 canvas。layout 未完成（clientWidth=0）
/// 也直接返回，等下一帧。
function resizeCanvasIfNeeded() {
  const cw = canvas.clientWidth;
  const ch = canvas.clientHeight;
  if (cw === 0 || ch === 0) return;
  const dpr = window.devicePixelRatio || 2;
  const wantedW = cw * dpr;
  const wantedH = ch * dpr;
  if (canvas.width === wantedW && canvas.height === wantedH) return;
  canvas.width = wantedW;
  canvas.height = wantedH;
  ctx.resetTransform();
  ctx.scale(dpr, dpr);
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
const hotkeyHintEl = document.getElementById("hotkey-hint") as HTMLElement;

function showView(which: "recording" | "processing") {
  recordingViewEl.hidden = which !== "recording";
  processingViewEl.hidden = which !== "processing";
}

function renderTriggerHint(payload: RecordingStartedPayload) {
  hotkeyHintEl.innerHTML = "";
  if (payload.trigger_mode === "double_tap_hold") {
    // 双击 modifier:渲染成两个相同的 <kbd>(对齐"双击"语义),供用户视觉提示
    const sym = formatDoubleTapModifier(payload.double_tap_modifier ?? "right_option");
    for (let i = 0; i < 2; i++) {
      const kbd = document.createElement("kbd");
      kbd.textContent = sym;
      hotkeyHintEl.appendChild(kbd);
    }
    return;
  }
  // toggle / push_to_talk:正常拆主热键
  for (const k of parseHotkeyKeys(payload.hotkey ?? "")) {
    const kbd = document.createElement("kbd");
    kbd.textContent = formatHotkeyKey(k);
    hotkeyHintEl.appendChild(kbd);
  }
}

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

// 实时识别文本越长越占行,需要把 indicator 窗口跟着撑高,否则下面的 bottom 栏会被
// 挤出可视区。后端窗口固定 W=520 H=180,这里通过 webview API 直接 setSize,
// 不重新调位置(从底部往下扩,顶部 y 不变,避免视觉抖动)。
const W = 520;
const BASE_H = 180;
const MAX_H = 360;
const PARTIAL_BASELINE = 18;
const win = getCurrentWindow();
let lastH = BASE_H;
let fitPending = false;

function scheduleFit() {
  if (fitPending) return;
  fitPending = true;
  requestAnimationFrame(async () => {
    fitPending = false;
    const partialH = partialEl.offsetHeight;
    const extra = Math.max(0, partialH - PARTIAL_BASELINE);
    const target = Math.min(MAX_H, BASE_H + extra);
    if (Math.abs(target - lastH) < 4) return;
    lastH = target;
    try {
      await win.setSize(new LogicalSize(W, target));
    } catch {
      // setSize 在 panel 上偶发失败,忽略 —— 下次更新会再试
    }
  });
}

async function resetSize() {
  if (lastH === BASE_H) return;
  lastH = BASE_H;
  try {
    await win.setSize(new LogicalSize(W, BASE_H));
  } catch {}
}

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
    scheduleFit();
  }
});

interface RecordingStartedPayload {
  hotkey: string;
  trigger_mode: string;
  double_tap_modifier: string;
}

listen<RecordingStartedPayload>("recording-started", (e) => {
  showView("recording");
  startTimer();
  statusTextEl.textContent = "录音中";
  partialEl.textContent = "等待语音…";
  partialEl.classList.add("empty");
  timerEl.style.color = "";
  // 后端 recording-started 永远会带 hotkey + trigger_mode + double_tap_modifier。
  // payload 缺时 fallback 到 toggle 风格,只渲染空 hint —— 比硬编码具体值更安全
  renderTriggerHint(
    e.payload ?? { hotkey: "", trigger_mode: "toggle", double_tap_modifier: "" },
  );
  resetSize();
});

listen("recording-stopped", () => {
  stopTimer();
  // 录音结束 → 进入润色 / 处理中态；input 模式下很快就会被 hide 一闪而过。
  // panel 模式下 indicator 也会 hide，结果文字会被单独的 result 窗口接手展示。
  showView("processing");
});

listen("recording-cancelled", () => {
  // ESC 取消:timer 关掉、partial 复位,窗口由后端 guard drop 直接 hide;
  // 下一次 recording-started 会把视图切回 recording 并重置
  stopTimer();
  partialEl.textContent = "已取消";
  partialEl.classList.add("empty");
});

// 页面加载时假设已经进入录音（悬浮窗只在录音时显示）
startTimer();

function loop() {
  render();
  requestAnimationFrame(loop);
}
loop();
