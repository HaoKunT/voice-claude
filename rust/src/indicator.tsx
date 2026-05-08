// 录音指示器：Raycast 风格波形（渐变色 + 发光 + 平滑过渡）
import { listen } from "@tauri-apps/api/event";

const BAR_COUNT = 40;
const BAR_WIDTH = 6;
const BAR_GAP = 10;
const history: number[] = Array(BAR_COUNT).fill(0);
let smoothed = 0;
const SMOOTH = 0.65;

const canvas = document.getElementById("wave") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

// 高 DPI 支持
const dpr = window.devicePixelRatio || 2;
canvas.width = canvas.clientWidth * dpr;
canvas.height = canvas.clientHeight * dpr;
ctx.scale(dpr, dpr);

function render() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  ctx.clearRect(0, 0, w, h);

  const totalW = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP;
  const startX = (w - totalW) / 2;
  const centerY = h / 2;

  for (let i = 0; i < BAR_COUNT; i++) {
    const level = history[i];
    // 非线性：小音量也有感知高度（sqrt 曲线）
    const barH = Math.max(4, Math.sqrt(level) * (h - 16));
    const x = startX + i * (BAR_WIDTH + BAR_GAP);
    const y = centerY - barH / 2;

    // 渐变：紫 → 红 → 青（Raycast 的招牌三色）
    const positionRatio = i / (BAR_COUNT - 1); // 0..1
    const grad = ctx.createLinearGradient(x, y, x, y + barH);
    const { r, g, b } = raycastColor(positionRatio, level);
    grad.addColorStop(0, `rgba(${r}, ${g}, ${b}, 0.95)`);
    grad.addColorStop(1, `rgba(${r}, ${g}, ${b}, 0.6)`);

    // 发光
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

    // 重置阴影避免影响下个 bar
    ctx.shadowBlur = 0;
  }
}

/**
 * 位置 + 音量共同决定颜色：
 * - 中间更偏品红 (ff5c5c)
 * - 两侧偏紫蓝 (9b87f5) / 青 (7dd3fc)
 * - 音量越大越偏暖
 */
function raycastColor(pos: number, level: number): { r: number; g: number; b: number } {
  // 中心距离（0 中间 → 1 两端）
  const centerDist = Math.abs(pos - 0.5) * 2;
  // 基色：中间偏红，两侧偏紫
  const baseR = 255 - 100 * centerDist;
  const baseG = 92 + 43 * centerDist;
  const baseB = 92 + 153 * centerDist;
  // 高音量推向暖色（更红）
  const warm = level;
  const r = Math.round(baseR * (0.6 + 0.4 * warm));
  const g = Math.round(baseG * 0.9);
  const b = Math.round(baseB * (1 - 0.4 * warm));
  return { r, g, b };
}

listen<number>("audio-level", (e) => {
  const raw = Math.max(0, Math.min(1, e.payload));
  smoothed = smoothed * SMOOTH + raw * (1 - SMOOTH);
  history.shift();
  history.push(smoothed);
});

function loop() {
  render();
  requestAnimationFrame(loop);
}
loop();
