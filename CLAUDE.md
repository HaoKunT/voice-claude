# CLAUDE.md

## 项目概述

voice-claude 是一个跨平台语音输入法（macOS / Windows），按一下热键开始录音，再按一下结束，识别文字自动输入到当前焦点窗口。

**技术栈正在从 Go + Fyne 迁移到 Rust + Tauri。** 主线代码在 `rust/` 目录，根目录的 Go 代码保留作为归档。所有新功能应加在 Rust 版。

数据流：按热键 → 录音（cpal）→ PCM 增益 → ASR 转写 → AI 纠错（可选）→ 热词替换 → 模拟键盘输入（enigo）

## 架构

### Rust + Tauri 主线（`rust/`）

- **后端**：Rust + Tauri v2 + tokio
- **前端**：React + TypeScript + Tailwind CSS + Vite
- **全局热键**：tauri-plugin-global-shortcut
- **数据库**：rusqlite (bundled)
- **音频**：cpal
- **键盘模拟**：enigo
- **WebSocket**：tokio-tungstenite
- **HTTP**：reqwest

### Go + Fyne 旧版（根目录）

保留到 Rust 版稳定。只做必要维护，不加新功能。

### ASR 后端

在设置界面切换，配置保存到 `config.json`：

| 后端 | provider 值 | 协议 | 说明 |
|---|---|---|---|
| 智谱（默认） | `zhipu` | HTTP 批处理 | 需要 API Key |
| 讯飞 | `xfyun` | WebSocket 流式 | 需要 AppID + AccessKey |
| 豆包/火山 | `volc` | WebSocket 流式 | 需要 App Key + Access Token，效果最好 |
| OpenRouter | `openrouter` | HTTP 批处理 | Whisper 模型 |
| 本地 SenseVoice | `local` | 离线 | 需手动下载模型，macOS arm64/amd64 |

流式后端（讯飞/豆包）：WebSocket 连接就绪后开始录音，松键发结束帧，实时出字。
批处理后端（智谱/OpenRouter/本地）：录完整段后识别。

### AI 纠错

可选，在识别完成后对文字做后处理：

| 模式 | 说明 |
|---|---|
| `off` | 不纠错（默认） |
| `ollama` | 本地 Ollama 模型 |
| `openrouter` | OpenRouter 任意模型 |
| `cloud` | 兼容 OpenAI API 的任意云端模型 |

## 项目结构

```
main.go              # 入口，热键注册，录音→识别→纠错→输入主流程
audio.go             # 录音设备枚举、PCM 采集、增益、静音裁剪、WAV 打包、流式推送
config.go            # 配置读写，路径由 dirs.go 提供
dirs.go              # 平台相关路径（配置/日志目录）
logger.go            # 日志初始化，同时写 stderr 和文件
hotkey.go            # 热键字符串解析
input.go             # 模拟键盘输入（robotgo）
gui.go               # 设置窗口（Fyne，暗色主题）
tray.go              # 系统托盘菜单
history.go           # 历史记录 SQLite 存储
history_window.go    # 历史记录窗口
asr.go               # 智谱 ASR（HTTP，支持超 30s 自动分段）
xfyun_asr.go         # 讯飞 ASR（WebSocket 流式）
volc_asr.go          # 豆包/火山 ASR（WebSocket 流式，VolcProtocol 二进制帧）
openrouter_asr.go    # OpenRouter ASR（Whisper，HTTP）
local_asr.go         # 本地 SenseVoice ASR（sherpa-onnx，build tag: !nosherpa）
local_asr_stub.go    # 本地 ASR stub（build tag: nosherpa，用于不支持平台）
correct.go           # AI 纠错（ollama / openrouter / cloud）
```

## 平台适配

### 跨平台原则
- 所有路径通过 `dirs.go` 的 `configDir()` / `appLogDir()` 获取，按平台返回惯例目录
- 不使用硬编码的 `/tmp`、`~` 路径
- 编译标签控制平台特定代码

### macOS
- 配置：`~/Library/Application Support/voice-claude/config.json`
- 日志：`~/Library/Logs/voice-claude/voice-claude.log`
- 历史：`~/Library/Application Support/voice-claude/history.db`
- 打包：`make build` 生成 `voice-claude.app`，`Info.plist` 设 `LSUIElement=true`（仅菜单栏）
- 安装：`make install` 编译 + 打包 + 复制到 `/Applications` 并清理本地构建产物

### Windows
- 配置：`%APPDATA%\voice-claude\config.json`
- 日志：`%LOCALAPPDATA%\voice-claude\logs\voice-claude.log`
- 历史：`%APPDATA%\voice-claude\history.db`
- 编译：`make build-win`，`-H windowsgui` 隐藏控制台窗口
- 本地 ASR 使用 `nosherpa` 构建标签跳过 sherpa-onnx（dylib 仅 macOS）

### 本地 SenseVoice（macOS only 暂时）
- 模型文件放在 `configDir()/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/`
- 依赖 `github.com/k2-fsa/sherpa-onnx-go`，需要预编译的 `.dylib`
- Windows 编译时加 `-tags nosherpa` 跳过

## 构建

```bash
# macOS：编译 + 打包 .app + 安装到 /Applications（一行搞定）
make install

# macOS：仅打包 .app，不安装
make build

# Windows（无控制台窗口）
make build-win

# 卸载
make uninstall
```

## 编码约定

- 用户界面文本和日志使用中文
- commit message 使用中文
- 流式后端函数签名：`func(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error)`
- `ready` channel 在 WebSocket 连接就绪后关闭，调用方等待后再开始录音
- 批处理后端函数签名：`func(wavBytes []byte, cfg *Config) (string, error)`
- 音频格式：16kHz、16bit、单声道 PCM
