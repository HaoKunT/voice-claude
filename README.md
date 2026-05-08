# voice-claude

按一下热键开始说话，再按一下热键结束，识别文字自动输入到当前焦点窗口。专为办公室气声输入设计。

> **迁移中：** 项目正在从 Go + Fyne 迁移到 Rust + Tauri。主线代码在 [`rust/`](./rust/) 目录，根目录的 Go 版本作为归档保留，CI 仍然同时校验两版。

数据流：**按热键 → 录音 → PCM 增益 → ASR 转写 → AI 纠错（可选）→ 热词替换 → 模拟键盘输入**

## 功能特性

- **系统托盘运行**，不占 Dock，全程后台
- **5 种 ASR 后端**：智谱、讯飞（流式）、豆包/火山（流式，效果最好）、OpenRouter Whisper、本地 SenseVoice（离线）
- **流式实时出字**：讯飞/豆包后端边说边出，延迟低
- **AI 纠错**：可选接 Ollama 本地模型或云端 API，自动修正同音字
- **信号增益**：自动放大气声，1-10x 可调
- **历史记录**：SQLite 本地存储，随时查看

## 安装

### macOS（Rust 版 - 主线）

需要 Rust stable + Node.js 20+ + pnpm：

```bash
git clone https://github.com/HaoKunT/voice-claude.git
cd voice-claude
make rust-install   # 编译 + 打包 .app + 安装到 /Applications
# 或开发模式
make rust-dev
```

首次启动需在「系统设置 → 隐私与安全性 → 辅助功能」中授权。

### macOS / Windows（Go 旧版 - 归档）

```bash
make go-install      # macOS
make go-build-win    # Windows
```

### Windows（Rust 版）

```bash
make rust-build-win   # 生成 Windows .msi / .exe 安装包
```

## 快速上手

1. 启动 `voice-claude.app`，菜单栏出现图标
2. 点击图标 → **设置**，选择 ASR 后端并填入 API Key
3. 默认热键 `Cmd+Shift+F5`：**按一下开始说话，再按一下结束**

## ASR 后端

| 后端 | 模式 | 说明 |
|------|------|------|
| 智谱（默认）| 批处理 | 需要 API Key，免费额度充足 |
| 讯飞 | 流式 | 需要 AppID + AccessKey |
| 豆包/火山 | 流式 | 效果最好，注册送 40 小时额度 |
| OpenRouter | 批处理 | Whisper large-v3-turbo |
| 本地 SenseVoice | 离线批处理 | 约 1GB 模型，macOS arm64/amd64 |

## AI 纠错（可选）

设置窗口中开启，可接：

- **Ollama**：本地模型，推荐 `qwen2.5:3b`
- **OpenRouter**：共用 ASR 的 API Key，可选任意模型
- **云端**：兼容 OpenAI API 的任意服务

## 气声输入技巧

- 麦杆贴近嘴角 2-3 cm，不振动声带，只送气
- 说话速度适中，增益调到 3-5x 可覆盖大多数气声场景
- 推荐带麦杆的有线 USB 耳麦（EPOS IMPACT 400 / Jabra Evolve2 40）

## 构建

```bash
# Rust + Tauri 主线
make rust-install    # macOS：编译 + 打包 .app + 安装到 /Applications
make rust-build      # macOS：仅打包 .app + .dmg
make rust-build-win  # Windows：.msi + .exe 安装包
make rust-test       # cargo test + 前端 typecheck
make rust-clippy     # Rust clippy lint
make rust-fmt        # 格式化 Rust 代码

# Go 旧版（归档）
make go-install      # macOS
make go-build-win    # Windows
make go-test / lint / vuln

# 清理
make clean
```

## 项目结构

```
rust/                # Rust + Tauri 主线（见 rust/README.md）
├── src/             # React + TypeScript 前端
│   ├── views/       # 设置 + 历史
│   ├── indicator.tsx# 悬浮波形
│   └── api.ts       # IPC 封装
└── src-tauri/src/   # Rust 后端
    ├── asr/         # 5 个 ASR 后端
    ├── audio.rs     # cpal 录音
    ├── config.rs    # JSON 配置
    ├── correct.rs   # AI 纠错
    ├── history.rs   # SQLite 历史
    ├── hotkey.rs    # 热键解析
    ├── hotwords.rs  # 热词替换
    ├── indicator.rs # 悬浮窗
    ├── input.rs     # enigo 键盘模拟
    ├── recorder.rs  # 主流程
    ├── tray.rs      # 系统托盘
    └── commands.rs  # Tauri IPC

# Go 旧版（根目录）
main.go              # 入口：热键注册、录音→识别→纠错→输入主流程
audio.go             # 录音设备枚举、PCM 采集、增益、WAV 打包、流式推送
config.go            # 配置读写
dirs.go              # 平台相关路径（macOS/Windows）
logger.go            # 日志初始化
hotkey.go            # 热键字符串解析
input.go             # 模拟键盘输入（robotgo）
gui.go               # 设置窗口（Fyne 暗色主题）
tray.go              # 系统托盘菜单
history.go           # 历史记录（SQLite）
history_window.go    # 历史记录窗口
asr.go               # 智谱 ASR（HTTP，支持自动分段）
xfyun_asr.go         # 讯飞 ASR（WebSocket 流式）
volc_asr.go          # 豆包/火山 ASR（WebSocket 流式）
openrouter_asr.go    # OpenRouter ASR（Whisper，HTTP）
local_asr.go         # 本地 SenseVoice（sherpa-onnx，!nosherpa）
local_asr_stub.go    # 本地 ASR stub（nosherpa，Windows/不支持平台）
correct.go           # AI 纠错（Ollama / OpenRouter / 云端）
```

## License

MIT
