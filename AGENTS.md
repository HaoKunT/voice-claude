# AGENTS.md

## 项目概述

voice-claude 是一个跨平台语音输入法（macOS / Windows）：按一下热键开始录音，说话，再按一下结束，识别文字自动输入到当前焦点窗口。

**技术栈**：Rust + Tauri v2 + React + TypeScript。

**代码位置**：
- 主线：`rust/`
- 归档：`legacy/go/`（Go + Fyne 旧版，不再维护）

数据流：**按热键 → 录音（cpal）→ PCM 增益 + 降采样 → ASR 转写 → AI 纠错（可选）→ 热词替换 → 模拟键盘输入（enigo）**

## 项目结构

```
rust/                           # Rust + Tauri 主线
├── src/                        # React + TypeScript 前端
│   ├── views/
│   │   ├── SettingsView.tsx    # 设置（自动保存）
│   │   └── HistoryView.tsx     # 历史记录
│   ├── indicator.tsx           # 悬浮波形窗 Canvas 动画
│   ├── api.ts                  # Tauri IPC 封装
│   ├── App.tsx
│   ├── main.tsx
│   └── index.css               # Raycast 风格 Tailwind
├── src-tauri/
│   ├── src/
│   │   ├── asr/                # 5 种 ASR 后端
│   │   │   ├── zhipu.rs        # 智谱 HTTP
│   │   │   ├── xfyun.rs        # 讯飞 WebSocket 流式
│   │   │   ├── volc.rs         # 豆包 WebSocket 流式
│   │   │   ├── openrouter.rs   # OpenRouter Whisper
│   │   │   ├── local.rs        # 本地 SenseVoice (sherpa-onnx)
│   │   │   └── wav.rs          # WAV 头 + 自动分段
│   │   ├── audio.rs            # cpal 录音 + 降采样
│   │   ├── input.rs            # enigo 键盘模拟
│   │   ├── indicator.rs        # NSPanel 悬浮窗
│   │   ├── tray.rs             # 系统托盘
│   │   ├── beep.rs             # 提示音
│   │   ├── recorder.rs         # 主流程
│   │   ├── correct.rs          # AI 纠错
│   │   ├── hotwords.rs         # 热词
│   │   ├── history.rs          # SQLite 历史
│   │   ├── config.rs           # JSON 配置
│   │   ├── hotkey.rs           # 热键解析
│   │   ├── dirs.rs             # 跨平台路径
│   │   ├── logger.rs           # tracing
│   │   ├── commands.rs         # Tauri IPC
│   │   └── lib.rs              # 入口
│   ├── icons/                  # 全套尺寸 icon
│   ├── entitlements.plist      # macOS 权限清单
│   ├── Info.plist              # Bundle Info（合并进 .app）
│   ├── tauri.conf.json
│   └── Cargo.toml
├── index.html                  # 设置/历史主窗口
├── indicator.html              # 悬浮窗
├── package.json
├── tailwind.config.js
├── tsconfig.json
├── vite.config.ts
├── WINDOWS_TODO.md             # Windows 待补齐清单
├── SMOKE_TEST.md
└── MIGRATION_PLAN.md

legacy/go/                      # Go + Fyne 旧版（归档，不再维护）
Makefile                        # 主线 make install / build / test，legacy 走 make legacy-*
.github/workflows/
├── ci.yml                      # Rust lint + test，package 手动触发
└── release.yml                 # 打 tag 自动发布
```

## ASR 后端签名

| 后端 | provider 值 | 协议 | 函数签名 |
|---|---|---|---|
| 智谱 | `zhipu` | HTTP 批处理 | `transcribe(cfg, wav) -> Result<String>` |
| 讯飞 | `xfyun` | WebSocket 流式 | `transcribe_stream(cfg, pcm_rx, on_partial, ready)` |
| 豆包 / 火山 | `volc` | WebSocket 流式 | 同上 |
| OpenRouter | `openrouter` | HTTP 批处理 | 同 zhipu |
| 本地 SenseVoice | `local` | 离线批处理 | 同 zhipu（feature=local-asr）|

流式后端 `on_partial` 回调在识别过程中实时调用 `input::type_text`，用户边说边看到文字出现。

## 平台适配要点

### macOS
- Accessory 模式（menubar-only），tray 点"设置" → 临时切 Regular → show 主窗口
- 悬浮窗：`tauri-nspanel` + `order_front_regardless` + `set_becomes_key_only_if_needed(true)` → 真·不抢焦点
- 签名：`make install` 自动 codesign 重签（固定 identifier=com.haokunt.voice-claude + 嵌入 entitlements）
- 权限：Info.plist 声明 NSMicrophoneUsageDescription + NSAppleEventsUsageDescription + LSUIElement

### Windows
- 待补，详见 `rust/WINDOWS_TODO.md`

## 构建

```bash
make dev           # 开发模式
make install       # macOS 安装
make build-win     # Windows 打包
make test          # cargo test + typecheck
make lint          # clippy + fmt --check
```

## 编码约定

- UI 文本 / 日志 / commit message 中文
- 跨平台路径走 `dirs::config_dir()`
- Tauri 主线程 API 用 `app.run_on_main_thread()` 从 worker 调度
- 流式 ASR 必须实现 `on_partial` 回调（边说边出字）
- 每次识别完成必须调 `tray::refresh()` 刷新最近 5 条
- 音频强制 16kHz / 16bit / 单声道 PCM（audio.rs 自动降采样）
