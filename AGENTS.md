# AGENTS.md

## 项目概述

voice-claude 是一个跨平台语音输入法（macOS / Windows），按住热键录音、松键后自动将识别文字输入到当前焦点窗口。适合办公室气声输入场景。

数据流：**按住热键 → 录音（malgo）→ PCM 增益 → ASR 转写 → AI 纠错（可选）→ 模拟键盘输入**

## 技术栈

- 语言：Go，CGO 用于音频和 GUI
- GUI 框架：Fyne v2（暗色主题，系统托盘）
- 音频：malgo（PortAudio 封装）
- 输入模拟：robotgo
- ASR：多后端，HTTP 批处理或 WebSocket 流式

## 项目结构

```
main.go              # 入口：热键注册、录音→识别→纠错→输入主流程
audio.go             # 录音设备枚举、PCM 采集、增益、WAV 打包、流式推送
config.go            # 配置读写（JSON，路径由 dirs.go 提供）
dirs.go              # 平台相关路径（macOS/Windows）
logger.go            # 日志初始化，同时写 stderr 和文件
hotkey.go            # 热键字符串解析
input.go             # 模拟键盘输入（robotgo）
gui.go               # 设置窗口（Fyne 暗色主题）
tray.go              # 系统托盘菜单
history.go           # 历史记录（SQLite）
history_window.go    # 历史记录窗口
asr.go               # 智谱 ASR（HTTP，支持超 30s 自动分段）
xfyun_asr.go         # 讯飞 ASR（WebSocket 流式）
volc_asr.go          # 豆包/火山 ASR（WebSocket 流式，自定义二进制帧）
openrouter_asr.go    # OpenRouter ASR（Whisper，HTTP）
local_asr.go         # 本地 SenseVoice（sherpa-onnx，build tag: !nosherpa）
local_asr_stub.go    # 本地 ASR stub（build tag: nosherpa，Windows/不支持平台）
correct.go           # AI 纠错（Ollama / OpenRouter / 云端）
```

## ASR 后端

| 后端 | provider 值 | 协议 | 函数签名 |
|---|---|---|---|
| 智谱（默认）| `zhipu` | HTTP 批处理 | `TranscribeZhipu(ctx, wavBytes, apiKey)` |
| 讯飞 | `xfyun` | WebSocket 流式 | `TranscribeXfyunStream(cfg, pcmCh, onPartial, ready)` |
| 豆包/火山 | `volc` | WebSocket 流式 | `TranscribeVolc(cfg, pcmCh, onPartial, ready)` |
| OpenRouter | `openrouter` | HTTP 批处理 | `TranscribeOpenRouter(ctx, wavBytes, cfg)` |
| 本地 SenseVoice | `local` | 离线批处理 | `TranscribeLocal(wavBytes)` |

- **流式后端**函数签名：`func(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error)`
  - `ready` channel 在 WebSocket 连接就绪后关闭，调用方等待后再开始录音
- **批处理后端**函数签名：`func(ctx context.Context, wavBytes []byte, ...) (string, error)`
- 音频格式：16kHz、16bit、单声道 PCM

## 配置文件

配置保存到平台惯例目录的 `config.json`：

- macOS：`~/Library/Application Support/voice-claude/config.json`
- Windows：`%APPDATA%\voice-claude\config.json`

## 构建

```bash
make install     # macOS：编译 + 打包 .app + 安装到 /Applications
make build       # macOS：仅打包 .app，不安装
make build-win   # Windows：编译 .exe
make test        # 运行测试（含 race detector）
make lint        # 运行 golangci-lint
```

## 编码约定

- 用户界面文本、日志、commit message 均使用中文
- 所有路径通过 `dirs.go` 的 `configDir()` / `appLogDir()` 获取，不硬编码
- 新增流式 ASR 后端：实现上述流式函数签名，在 `main.go` 的 `handleRecord` switch 中注册
- 新增批处理 ASR 后端：实现批处理函数签名，在 `asrTranscribe` switch 中注册
- HTTP 请求必须使用 `http.NewRequestWithContext` 并传入 `ctx`
- 包级别复用 HTTP client：短超时用 `ollamaCheckClient`，ASR 用 `asrHTTPClient`
