// 录音指示器：监听后端 audio-level 事件，Canvas 实时画波形。
import { listen } from "@tauri-apps/api/event";

const BAR_COUNT = 40;
const history: number[] = Array(BAR_COUNT).fill(0);
let smoothed = 0;
const SMOOTH = 0.6;

const canvas = document.getElementById("wave") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

function render() {
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const totalW = BAR_COUNT * 8 + (BAR_COUNT - 1) * 8; // barWidth 8 + gap 8
  const startX = (w - totalW) / 2;
  const centerY = h / 2;

  for (let i = 0; i < BAR_COUNT; i++) {
    const level = history[i];
    const barH = Math.max(8, Math.sqrt(level) * (h - 16));
    const x = startX + i * 16;
    const y = centerY - barH / 2;

    // 冷色 → 暖色
    const r = Math.round(100 + (255 - 100) * level);
    const g = Math.round(180 + (80 - 180) * level);
    const b = Math.round(255 + (180 - 255) * level);
    ctx.fillStyle = `rgba(${r},${g},${b},0.9)`;

    ctx.beginPath();
    const radius = 4;
    ctx.moveTo(x + radius, y);
    ctx.arcTo(x + 8, y, x + 8, y + barH, radius);
    ctx.arcTo(x + 8, y + barH, x, y + barH, radius);
    ctx.arcTo(x, y + barH, x, y, radius);
    ctx.arcTo(x, y, x + 8, y, radius);
    ctx.closePath();
    ctx.fill();
  }
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
