// 录音指示器：Raycast 风格波形 + 实时识别文字 + 录音计时
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

const BAR_COUNT = 40;
const BAR_WIDTH = 6;
const BAR_GAP = 10;
const history: number[] = Array(BAR_COUNT).fill(0);
let smoothed = 0;
const SMOOTH = 0.65;

const canvas = document.getElementById("wave") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

const dpr = window.devicePixelRatio || 2;
canvas.width = canvas.clientWidth * dpr;
canvas.height = canvas.clientHeight * dpr;
ctx.scale(dpr, dpr);

// ==== 波形渲染 ====

function render() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
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
const resultViewEl = document.getElementById("result-view") as HTMLElement;
const resultTextEl = document.getElementById("result-text") as HTMLElement;
const copyBtn = document.getElementById("result-copy") as HTMLButtonElement;
const closeBtn = document.getElementById("result-close") as HTMLButtonElement;

copyBtn.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(resultTextEl.textContent ?? "");
    copyBtn.textContent = "✓ 已复制";
    copyBtn.classList.add("copied");
  } catch (e) {
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

listen("recording-started", () => {
  // 切回录音态（清掉上次的 result-view）
  recordingViewEl.hidden = false;
  resultViewEl.hidden = true;
  startTimer();
  statusTextEl.textContent = "录音中";
  partialEl.textContent = "等待语音…";
  partialEl.classList.add("empty");
  timerEl.style.color = "";
});

listen("recording-stopped", () => {
  stopTimer();
  statusTextEl.textContent = "处理中…";
});

// panel output mode：识别 + 润色 + 热词完成后，切到"已识别态"
listen<string>("asr-final-text", (e) => {
  const text = e.payload ?? "";
  resultTextEl.textContent = text;
  recordingViewEl.hidden = true;
  resultViewEl.hidden = false;
  // 复位复制按钮
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
