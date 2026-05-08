# voice-claude Rust + Tauri 迁移计划

## 目标

把 Go + Fyne 版本完整迁移到 Rust + Tauri v2 + React + TypeScript。

**前端**：React + TypeScript + Tailwind CSS + Vite
**后端**：Rust + Tauri v2 + tokio
**跨平台**：macOS（arm64 + amd64）+ Windows（amd64）

## 技术栈

| 模块 | Crate / 库 |
|---|---|
| Async runtime | `tokio` |
| HTTP | `reqwest` |
| WebSocket | `tokio-tungstenite` |
| 音频录制 | `cpal` |
| 键盘模拟 | `enigo` |
| 全局热键 | `tauri-plugin-global-shortcut` |
| SQLite | `rusqlite` with bundled |
| 配置存储 | `tauri-plugin-store` + 手写 JSON |
| 序列化 | `serde` + `serde_json` |
| 错误处理 | `thiserror` + `anyhow` |
| 日志 | `tracing` + `tracing-subscriber` |
| HMAC/签名 | `hmac` + `sha1` + `base64` |
| macOS NSPanel | `tauri-nspanel` (ahkohd) |

## 迁移 checklist

### Phase 1 - 基础架构
- [x] 创建 Tauri + React + TS 脚手架
- [x] 重组 rust/ 目录结构
- [ ] 更新 Cargo.toml 加齐依赖
- [ ] 更新 package.json 加 Tailwind + UI 库
- [ ] 初始化 Tailwind CSS
- [ ] Tauri 配置（窗口、capabilities、bundler）

### Phase 2 - 业务逻辑（无 UI 部分）
- [ ] `dirs.rs` - 跨平台配置/日志路径（对应 dirs.go）
- [ ] `config.rs` - 配置 struct + JSON 读写（对应 config.go）
- [ ] `logger.rs` - tracing 日志初始化（对应 logger.go）
- [ ] `hotwords.rs` - 热词替换（对应 hotwords.go）
- [ ] `history.rs` - SQLite 历史（对应 history.go）
- [ ] `hotkey.rs` - 热键字符串解析（对应 hotkey.go）
- [ ] `audio.rs` - cpal 录音 + PCM 增益 + WAV 打包（对应 audio.go）
- [ ] `input.rs` - enigo 键盘输入 + 退格 + 焦点保存（对应 input.go）

### Phase 3 - ASR 后端
- [ ] `asr/zhipu.rs` - 智谱 HTTP + 自动分段（对应 asr.go）
- [ ] `asr/xfyun.rs` - 讯飞 WebSocket 流式（对应 xfyun_asr.go）
- [ ] `asr/volc.rs` - 豆包 WebSocket 流式 + 二进制帧（对应 volc_asr.go）
- [ ] `asr/openrouter.rs` - OpenRouter Whisper（对应 openrouter_asr.go）
- [ ] `asr/local.rs` - 本地 SenseVoice（可用 sherpa-onnx 的 Rust FFI，先做 stub）

### Phase 4 - AI 纠错
- [ ] `correct.rs` - ollama / openrouter / cloud 纠错（对应 correct.go）

### Phase 5 - UI
- [ ] 系统托盘菜单（Tauri tray API）
- [ ] 设置窗口（React + Tailwind）
- [ ] 历史记录窗口（React + Tailwind）
- [ ] 悬浮录音窗口（`tauri-nspanel` macOS / 模拟 Windows）
- [ ] 波形动画组件（Canvas）

### Phase 6 - 热键 + 主流程
- [ ] 全局热键注册（`tauri-plugin-global-shortcut`）
- [ ] 录音切换状态机（对应 toggleRecording）
- [ ] handleRecord 等价流程（录音 → ASR → 纠错 → 热词 → 输入）

### Phase 7 - 打包
- [ ] tauri.conf.json bundler 配置
- [ ] macOS .app + dmg
- [ ] Windows .msi + .exe
- [ ] 图标资源（从 icon.png 转换）

### Phase 8 - CI/CD
- [ ] `.github/workflows/ci-rust.yml` 新增
- [ ] Rust test / clippy / fmt
- [ ] 前端 lint / build
- [ ] macOS 打包 job
- [ ] Windows 打包 job
- [ ] 旧 Go CI 保留直到 Rust 版稳定

### Phase 9 - 文档
- [ ] 更新 README.md
- [ ] 更新 CLAUDE.md
- [ ] 更新 AGENTS.md
- [ ] 迁移说明

### Phase 10 - 清理
- [ ] 验证所有功能
- [ ] 归档 Go 代码到 `legacy/`
- [ ] 替换 Makefile
- [ ] CI 切换到 Rust 为主

## 进度追踪

ralph-loop 每轮迭代的进度记在这里。完成后在对应 checkbox 打勾。

Last updated: iteration 1
