# Rust 版 Smoke Test 指南

用于验证 Rust 版本功能已达到 Go 版 feature parity。每次 iteration 后跑一遍。

## 前置

```bash
cd rust
pnpm install
cd src-tauri && cargo build --release
```

## 1. 静态检查

```bash
cd rust/src-tauri
cargo fmt --check            # 代码格式
cargo clippy --all-targets -- -D warnings  # lint
cargo test --locked          # unit test（29 个）
```

```bash
cd rust
pnpm typecheck               # TS 类型
```

预期：全部通过。

## 2. 打包

### macOS

```bash
cd rust
pnpm tauri build --bundles app,dmg
```

预期产物：
- `src-tauri/target/release/bundle/macos/voice-claude.app`（~18MB）
- `src-tauri/target/release/bundle/dmg/voice-claude_0.1.0_aarch64.dmg`

```bash
# 安装到 /Applications
make rust-install
open /Applications/voice-claude.app
```

### Windows（需要 Windows 机器或 CI）

```powershell
cd rust
pnpm tauri build --bundles msi,nsis
```

## 3. 交互 smoke（macOS）

运行 `voice-claude.app` 后：

| 步骤 | 预期 |
|---|---|
| 启动后应用不在 Dock 出现 | ✓ activationPolicy::Accessory 生效 |
| 菜单栏出现 voice-claude 图标 | ✓ tray 正常 |
| 点击托盘 → 设置 | ✓ 主窗口弹出 |
| 选择豆包，填入 App Key / Access Token | ✓ 能保存 |
| 关闭主窗口（红叉） | ✓ 只隐藏，托盘仍在 |
| 按热键 Cmd+Shift+F5 | ✓ 悬浮波形窗弹出，菜单栏无焦点切换 |
| 对麦克风说话 | ✓ 悬浮窗波形随音量跳动 |
| 再按 Cmd+Shift+F5 | ✓ 悬浮窗消失，文字输入到光标位置 |
| 打开设置 → 历史记录 | ✓ 记录出现，点击可看详情 |
| 填 "克劳德→Claude" 热词 | ✓ 下次识别 "克劳德" 被替换 |

## 4. 日志

```bash
tail -f ~/Library/Logs/voice-claude/voice-claude.log
```

看是否有错误。

## 5. 已知差异（Go vs Rust）

- **本地 SenseVoice**：Go 版通过 `sherpa-onnx-go` 支持（macOS），Rust 版是 stub，后续用 sherpa-onnx 官方 Rust API 接入
- **真·不抢焦点浮窗**：Go 版和 Rust 版都用 always-on-top 模拟。后续 Rust 接 `tauri-nspanel` 做真 NSPanel
- **悬浮窗毛玻璃**：均未实现（需要 `NSVisualEffectView`）
