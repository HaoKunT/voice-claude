# CLAUDE.md

## 项目概述

voice-claude 是一个跨平台语音输入法（macOS / Windows），按一下热键开始录音，再按一下结束，识别文字自动输入到当前焦点窗口。

**技术栈**：Rust + Tauri v2 + React + TypeScript，主线代码全部在 `rust/` 目录。

**归档**：`legacy/go/` 是旧版 Go + Fyne 代码，已经转正到 Rust，legacy 不再维护。

数据流：按热键 → 录音（cpal）→ PCM 增益 + 降采样 → ASR 转写 → AI 纠错（可选）→ 热词替换 → 模拟键盘输入（enigo）

## 架构

### 核心依赖

- **后端**：tokio / reqwest / tokio-tungstenite / cpal / enigo / rusqlite (bundled)
- **Tauri 插件**：tauri-plugin-global-shortcut / tauri-plugin-clipboard-manager / tauri-plugin-store / tauri-plugin-dialog
- **macOS 专用**：tauri-nspanel（NSPanel 真·不抢焦点）
- **离线 ASR**：sherpa-onnx 1.13（SenseVoice，默认启用 feature = "local-asr"）

### 前端

- React + TypeScript + Tailwind CSS + Vite
- Raycast 风格视觉（深色毛玻璃 + 三色渐变 + SF Pro 字体）
- 两个 entry：`index.html`（主窗口设置+历史）/ `indicator.html`（悬浮录音窗 Canvas 波形）

### 模块对照

| Rust 模块 | 职责 |
|---|---|
| `lib.rs` | 主入口 + Tauri Builder + 热键绑定 |
| `recorder.rs` | 录音切换状态机 + 主流程 |
| `audio.rs` | cpal 录音 + 设备兼容降采样到 16kHz mono |
| `input.rs` | enigo 键盘模拟输入 |
| `indicator.rs` | 悬浮波形窗（macOS NSPanel + 预创建） |
| `tray.rs` | 系统托盘 + 最近识别快捷复制 |
| `beep.rs` | 录音提示音（macOS afplay / Windows SystemSounds） |
| `asr/zhipu.rs` | 智谱 HTTP + 自动分段 |
| `asr/xfyun.rs` | 讯飞 WebSocket 流式 |
| `asr/volc.rs` | 豆包 WebSocket 流式（二进制帧协议） |
| `asr/openrouter.rs` | OpenRouter Whisper |
| `asr/local.rs` | 本地 SenseVoice（sherpa-onnx） |
| `correct.rs` | AI 纠错（ollama / openrouter / cloud） |
| `hotwords.rs` | 热词替换 |
| `history.rs` | SQLite 历史 |
| `config.rs` | JSON 配置 |
| `hotkey.rs` | 热键字符串解析 |
| `dirs.rs` | 跨平台路径 |
| `logger.rs` | tracing 日志 |
| `commands.rs` | Tauri IPC commands |

## 平台适配

### macOS
- 配置：`~/Library/Application Support/voice-claude/config.json`
- 日志：`~/Library/Logs/voice-claude/voice-claude.log`
- 历史：`~/Library/Application Support/voice-claude/history.db`
- 打包：`make install` → `pnpm tauri build --bundles app` → 自动 codesign 重签（固定 identifier + 嵌入 entitlements）
- Accessory 模式（menubar-only），tray 点"设置"临时切 Regular
- 悬浮窗：tauri-nspanel + `order_front_regardless` + `set_becomes_key_only_if_needed(true)` → 真·不抢焦点

### Windows
- 配置：`%APPDATA%\voice-claude\config.json`
- 日志：`%LOCALAPPDATA%\voice-claude\logs\voice-claude.log`
- 打包：`pnpm tauri build --bundles msi,nsis`
- 待补：NSPanel 等价（`WS_EX_NOACTIVATE`）+ 快捷键符号 + 毛玻璃，详见 `rust/WINDOWS_TODO.md`

## 构建

```bash
make dev           # 开发模式
make install       # macOS 打包 + 安装
make build-win     # Windows 打包（需 Windows 机器）
make test          # cargo test + typecheck
make lint          # clippy + fmt --check
```

legacy Go 版（只在 `make legacy-*` 下编译，不影响主线）。

## 编码约定

- 用户界面文本 / 日志 / commit message 均使用中文
- 所有跨平台路径走 `dirs::config_dir()` / `dirs::log_dir()`
- Tauri 主线程 API（window/menu/panel）必须用 `app.run_on_main_thread()` 从 tokio worker 调度
- 所有 ASR 后端都在 `async` 上下文跑，流式后端用 `transcribe_stream` 签名（onPartial 回调实时输入）
- 每次识别完成后调 `tray::refresh(&app)` 刷新托盘菜单的最近 5 条
- 音频格式：16kHz / 16bit / 单声道 PCM（audio.rs 自动从设备 native 采样率降采样）
