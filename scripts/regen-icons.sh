#!/usr/bin/env bash
# voice-claude icon 一键重生
#
# 从一张透明底线描 logo 源图，产出：
#   1. Dock/Finder/关于页用的 app icon（深色 squircle 底 + logo 居中）
#   2. 菜单栏 tray icon（无底、加粗 stroke、trim 到主体铺满）
#
# 前置：macOS 装了 ImageMagick v7（`brew install imagemagick`）
#
# 用法：
#   scripts/regen-icons.sh [source]                 # 默认源 logo.webp
#   scripts/regen-icons.sh logo-v2.png              # 用别的源
#   APP_LOGO_SIZE=720 scripts/regen-icons.sh        # 主体更小留更多呼吸
#   BG_COLOR='#0a0a0f' scripts/regen-icons.sh       # 换 squircle 底色
#   TRAY_DILATE=5 scripts/regen-icons.sh            # 菜单栏线条更粗
#
# 可调参数（都有默认值）：
#   APP_LOGO_SIZE  squircle 内主体像素，1024 canvas 里，默认 780（约占 76%）
#   BG_COLOR       squircle 底色，默认 #1c1c26
#   CORNER_R       squircle 圆角半径，默认 180
#   TRAY_SIZE      tray 主体像素，44 canvas 里，默认 40（留 2px 安全边）
#   TRAY_DILATE    tray stroke 加粗半径，默认 3（太细菜单栏看不清）
#
# 源图要求：
#   - 透明背景 + 纯 logo 主体（推荐 webp / png / svg）
#   - 最好是 1024×1024 或更高分辨率
#   - 若源图本身带深色底（已是 app icon 成品），跳过这个脚本，直接：
#       cd rust && pnpm tauri icon ../your-icon.png

set -euo pipefail

SOURCE="${1:-logo.webp}"
APP_LOGO_SIZE="${APP_LOGO_SIZE:-720}"
BG_COLOR="${BG_COLOR:-#1c1c26}"
CORNER_R="${CORNER_R:-180}"
TRAY_SIZE="${TRAY_SIZE:-40}"
TRAY_DILATE="${TRAY_DILATE:-3}"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

if [ ! -f "$SOURCE" ]; then
  echo "❌ 找不到源图：$SOURCE" >&2
  echo "   用法：$0 [source]   （默认 logo.webp）" >&2
  exit 1
fi

if ! command -v magick >/dev/null 2>&1; then
  echo "❌ 没找到 ImageMagick，请先 brew install imagemagick" >&2
  exit 1
fi

# 检查源图是否带 alpha 通道；没有 alpha 的话合成出来会是大方块
HAS_ALPHA=$(magick "$SOURCE" -format '%A' info: 2>/dev/null || echo "False")
if [ "$HAS_ALPHA" != "True" ] && [ "$HAS_ALPHA" != "Blend" ]; then
  echo "⚠ 源图 $SOURCE 没有 alpha 通道，合成出来的 app icon 会是白底方块。"
  echo "  请先手动抠掉背景（推荐 rembg 或 Photoshop），再重跑此脚本。" >&2
  exit 2
fi

TMP_ICON=$(mktemp -t vc-icon.XXXXXX).png
trap 'rm -f "$TMP_ICON"' EXIT

echo "=== [1/2] 合成 app icon：$SOURCE (${APP_LOGO_SIZE}px) + squircle 底（${BG_COLOR} R=${CORNER_R}）==="
magick -size 1024x1024 xc:none \
  -fill "$BG_COLOR" \
  -draw "roundrectangle 20,20 1003,1003 ${CORNER_R},${CORNER_R}" \
  \( "$SOURCE" -resize "${APP_LOGO_SIZE}x${APP_LOGO_SIZE}" \) \
  -gravity center -composite \
  "$TMP_ICON"

echo "=== [2/2a] pnpm tauri icon 派生全套 macOS/Windows/iOS/Android icon ==="
(cd rust && pnpm tauri icon "$TMP_ICON" 2>&1 | tail -3)

echo "=== [2/2b] tray icon：dilate Disk:${TRAY_DILATE} + trim + ${TRAY_SIZE}/44 ==="
magick "$SOURCE" \
  -channel A -morphology Dilate "Disk:${TRAY_DILATE}" +channel \
  -trim +repage -resize "${TRAY_SIZE}x${TRAY_SIZE}" \
  -background none -gravity center -extent 44x44 \
  rust/src-tauri/icons/tray.png

echo "=== [2/2c] 同步 UI 里的 app-icon.png（sidebar + 关于页展示用）==="
cp rust/src-tauri/icons/128x128.png rust/public/app-icon.png

echo ""
echo "✓ 完成。下一步：make install 重装后看 Dock / 菜单栏 / 关于页的效果。"
