# Windows 版待补齐

Rust + Tauri 现在 macOS 已经全功能可用。Windows 等价需要：

## 必做（影响体验）

### 1. 快捷键符号（前端）
- 检测 `navigator.platform`
- macOS → `⌘⇧F5`
- Windows → `Ctrl+Shift+F5`

相关文件：
- `src/views/SettingsView.tsx`（KbdCombo 组件）
- `indicator.html`（录音指示器底部 kbd）

### 2. 悬浮窗不抢焦点
- 当前 macOS 用 `NSPanel` + `set_becomes_key_only_if_needed`
- Windows 等价：`SetWindowLongPtrW(hwnd, GWL_EXSTYLE, existing | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)`
- 用 `windows-rs` crate 调原生 API
- 改动点：`src-tauri/src/indicator.rs` 的 `prebuild` 里 `#[cfg(target_os = "windows")]` 分支

## 锦上添花

### 3. 毛玻璃背景（Windows 11 Mica）
- 用 `window-vibrancy` crate
- Windows 11 → Mica
- Windows 10 → Acrylic
- 现有 CSS `backdrop-filter` 作为 fallback

### 4. 安装体验
- 默认打包 `msi` + `nsis exe`，用户可选
- 代码签名证书（避免 SmartScreen 警告，用户需要 EV 证书）

### 5. 自启动
- Windows：注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- 或者 `tauri-plugin-autostart`
- macOS 同步用 `LaunchAgents`

## 测试路径

没有 Windows 开发机 → 走 CI 的 release workflow（`.github/workflows/release.yml`）按 tag 触发
打 tag 时会在 windows-latest 跑 tauri-action，产出 `.msi` / `.exe`
