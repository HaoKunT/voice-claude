# voice-claude Rust + Tauri 迁移计划

## 目标

把 Go + Fyne 版本完整迁移到 Rust + Tauri v2 + React + TypeScript。

**前端**：React + TypeScript + Tailwind CSS + Vite
**后端**：Rust + Tauri v2 + tokio
**跨平台**：macOS（arm64 + amd64）+ Windows（amd64）

## 迁移 checklist

### Phase 1 - 基础架构 ✓
- [x] Tauri + React + TS 脚手架
- [x] 扁平化 rust/ 目录结构
- [x] Cargo.toml 完整依赖
- [x] package.json 补齐 Tailwind + ESLint
- [x] Tailwind 初始化
- [x] Tauri 配置（窗口 / tray / bundler / private-api / entitlements）

### Phase 2 - 业务逻辑 ✓
- [x] dirs.rs - 跨平台配置/日志/历史路径（含 unit test）
- [x] config.rs - Config + JSON 读写（含 unit test）
- [x] logger.rs - tracing 双输出
- [x] hotwords.rs - 热词替换（含 unit test）
- [x] history.rs - SQLite 历史
- [x] hotkey.rs - 热键字符串解析（含 unit test）
- [x] audio.rs - cpal 录音 + 增益 + 实时音量
- [x] input.rs - enigo 键盘模拟 + 退格

### Phase 3 - ASR 后端 ✓（本地模块 stub）
- [x] asr/wav.rs - WAV 头 + 分段（含 unit test）
- [x] asr/zhipu.rs - 智谱 HTTP + 自动分段
- [x] asr/xfyun.rs - 讯飞 WebSocket 流式
- [x] asr/volc.rs - 豆包 WebSocket 流式（含 unit test）
- [x] asr/openrouter.rs - OpenRouter Whisper
- [~] asr/local.rs - 本地 SenseVoice（stub，后续接入 sherpa-rs）

### Phase 4 - AI 纠错 ✓
- [x] correct.rs - ollama / openrouter / cloud + check_ollama（含 unit test）

### Phase 5 - UI ✓
- [x] 系统托盘菜单（Tauri tray API）
- [x] 主窗口（设置 + 历史，React 路由）
- [x] SettingsView.tsx - 完整配置界面（ASR / 纠错 / 录音 / 热词 / 日志 / 本地 SenseVoice 下载 + 进度 + 打开模型目录）
- [x] HistoryView.tsx - 历史记录 + 详情弹窗 + 删除/清空
- [x] indicator.html + indicator.tsx - Canvas 波形动画
- [x] indicator.rs - macOS 真·NSPanel 不抢焦点（tauri-nspanel），Windows/Linux 兜底 always-on-top

### Phase 6 - 热键 + 主流程 ✓
- [x] 全局热键注册（tauri-plugin-global-shortcut）
- [x] 录音切换状态机（recorder::toggle）
- [x] 主流程 run（录音 → ASR → 纠错 → 热词 → 输入）
- [x] 流式 partial 实时输入 + 结束时退格替换
- [x] audio-level 30fps emit 给前端

### Phase 7 - 打包 ✓
- [x] tauri.conf.json bundler 配置（macOS / Windows）
- [x] entitlements.plist（麦克风 / 自动化）
- [x] icons 从 icon.png 生成全套尺寸
- [~] 实际 `pnpm tauri build` 跑通（iteration 4 做）

### Phase 8 - CI/CD ✓
- [x] .github/workflows/ci.yml
  - Rust 主线：lint（fmt + clippy + typecheck）/ test / package (macOS arm64 + amd64 + Windows)
  - Go 归档：go-lint / go-security / go-test
- [x] Makefile 重写（rust-* 主目标 + go-* legacy）

### Phase 9 - 文档 ✓
- [x] README.md 更新双版本说明
- [x] CLAUDE.md 更新 Rust 架构
- [x] rust/README.md 新建

### Phase 10 - 收尾完成 ✓
- [x] `pnpm tauri build` 本地验证通过（18MB .app）
- [x] macOS arm64 + Windows amd64 CI 打包验证（14MB / 11MB 产物）
- [x] 本地 SenseVoice 下载 + SHA256 + bz2+tar 解压完整实现
- [x] tauri-nspanel 真·不抢焦点（macOS 核心诉求）
- [x] tauri-action 自动发布 release workflow

### 仍可作为未来优化（非功能缺失）
- [ ] `--features local-asr` 首次编译时间长（sherpa-onnx C 库 ~20 分钟），需机器稳定网络环境跑一遍
- [ ] Windows 端代码签名证书
- [ ] macOS Intel (amd64) CI 矩阵（runner 队列问题，目前走 release workflow 按需）

## 进度追踪

- **iteration 1**: Phase 1 + 2 (80%) + 3 + 4 + 5 骨架 + 6 骨架
- **iteration 2**: 前端 UI + Tailwind + CI + cargo test/clippy 全绿
- **iteration 3**: 悬浮指示器集成 + audio-level emit + icons 生成 + README
- **iteration 4**: macOS .app 打包通过 + menubar-only activation policy
- **iteration 5**: smoke test 文档 + 静音裁剪 + zhipu 分段参数对齐
- **iteration 6**: release workflow + CI 精简 + local.rs API 镜像
- **iteration 7**: local SenseVoice 完整下载 + 推理（feature-gated）
- **iteration 8**: **真·NSPanel 不抢焦点（tauri-nspanel）+ 本地 SenseVoice 下载 UI**
- CI 精简：日常 push 只跑 lint/test，打包走 release workflow

Last updated: iteration 8（完整迁移达成）
