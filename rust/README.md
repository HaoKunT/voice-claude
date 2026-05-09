# voice-claude (Rust + Tauri)

跨平台语音输入法的 Rust 重写版。Go + Fyne 旧版归档在 repo 根目录，保留到 Rust 版稳定。

## 结构

```
rust/
├── src/                # React + TypeScript 前端
│   ├── views/          # SettingsView / HistoryView
│   ├── indicator.tsx   # 悬浮录音波形
│   └── api.ts          # Tauri IPC 封装
├── src-tauri/          # Rust 后端
│   ├── src/
│   │   ├── asr/        # 5 个 ASR 后端
│   │   ├── audio.rs    # cpal 录音
│   │   ├── config.rs   # JSON 配置
│   │   ├── correct.rs  # AI 纠错
│   │   ├── history.rs  # SQLite 历史
│   │   ├── hotkey.rs   # 热键解析
│   │   ├── hotwords.rs # 热词替换
│   │   ├── indicator.rs# 悬浮窗
│   │   ├── input.rs    # enigo 键盘模拟
│   │   ├── recorder.rs # 主流程
│   │   ├── tray.rs     # 系统托盘
│   │   └── commands.rs # Tauri IPC commands
│   └── Cargo.toml
├── index.html          # 设置 / 历史主窗口
├── indicator.html      # 悬浮指示器窗口
└── tailwind.config.js
```

## 开发

```bash
# 前端依赖
pnpm install

# 开发模式（热重载 + Tauri）
pnpm tauri dev

# 打包 .app（macOS）+ .msi/.exe（Windows）
pnpm tauri build
```

## 测试 / lint

```bash
# Rust 测试
cd src-tauri && cargo test

# Rust clippy
cargo clippy --all-targets -- -D warnings

# 前端类型检查
pnpm typecheck
```

## 迁移进度

详见 `MIGRATION_PLAN.md`。

## macOS ad-hoc 签名相关限制

首次安装需 `xattr -dr`、每次更新后需重新勾辅助功能权限——根因与完整解释见根目录 [`README.md` 的「已知限制」节](../README.md#已知限制macos-ad-hoc-签名)。
