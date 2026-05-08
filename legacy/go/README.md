# voice-claude Go + Fyne 归档

这是 voice-claude 最初的 Go + Fyne 实现，已经**被 Rust + Tauri 版本取代**。

**不再维护**。保留在仓库里作为：
- 参考（Go 时代的实现思路，对照 Rust 迁移）
- 紧急 fallback（如果 Rust 版严重回归）

## 如何构建这个老版本

根仓库的 Makefile 提供 `legacy-*` 目标：

```bash
# 从仓库根目录
make legacy-build        # macOS 打包 .app
make legacy-install      # macOS 安装到 /Applications
make legacy-build-win    # Windows 编译 .exe
make legacy-test         # 跑 Go 测试
make legacy-lint         # golangci-lint
make legacy-vuln         # govulncheck
```

或者直接在此目录下手动跑 `go build ./...`。

## 为什么换到 Rust？

核心原因：**Fyne/GLFW 底层无法做 macOS NSPanel nonactivating panel**，悬浮录音窗总会抢焦点，识别结果可能投递到错误窗口。

Rust + Tauri + `tauri-nspanel` 能调到 NSPanel native API，焦点完美。

详细迁移故事见 `git log` 和主仓库 README。

## 原目录结构

```
main.go              # 入口：热键注册、录音→识别→纠错→输入主流程
audio.go             # 录音（malgo）+ 增益 + 静音裁剪 + WAV 打包 + 流式推送
config.go            # 配置读写
dirs.go              # 平台相关路径
logger.go            # 日志
hotkey.go            # 热键字符串解析
hotkey_darwin.go     # macOS 键映射
hotkey_windows.go    # Windows 键映射
input.go             # 键盘模拟（robotgo）
gui.go               # 设置窗口（Fyne 暗色主题）
tray.go              # 系统托盘
history.go           # SQLite 历史
history_window.go    # 历史记录窗口
asr.go               # 智谱 ASR + 自动分段
xfyun_asr.go         # 讯飞 WebSocket 流式
volc_asr.go          # 豆包 WebSocket 流式
openrouter_asr.go    # OpenRouter Whisper
local_asr.go         # 本地 SenseVoice（!nosherpa）
local_asr_stub.go    # 本地 ASR stub（nosherpa 时编译）
correct.go           # AI 纠错（ollama / openrouter / cloud）
hotwords.go          # 热词替换
recording_indicator.go # 悬浮录音窗（Fyne，抢焦点）
Info.plist           # macOS bundle 描述
```
