# Windows 版待补齐

Rust + Tauri 现在 macOS 已经全功能可用。Windows 等价进度记录在这里。

## ✅ 已完成

- **快捷键符号显示** —— `src/lib/hotkey.ts` 用 `IS_MAC` 自动切 `SYMBOLS_MAC` / `SYMBOLS_WIN`,indicator + KbdCombo 都走它
- **MSI + NSIS 双格式打包** —— `.github/workflows/ci.yml` + `release.yml` 在 `windows-latest` 上已经能产出 `.msi` / `.exe`
- **悬浮窗不抢焦点** —— `indicator.rs` Windows 分支用 `windows` crate 调 `SetWindowLongPtrW` 设 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`
- **窗口透明背景 + 毛玻璃** —— `indicator.rs` / `result.rs` Windows 分支用 `window-vibrancy::apply_mica`,失败回退 `apply_acrylic`(Win11 用 Mica,Win10 用 Acrylic;CSS `backdrop-filter` 是最后兜底)
- **代码签名** —— 实测 Windows 上正常安装,SmartScreen 不阻断

## 🟡 锦上添花(待办,不阻塞首发)

### 自启动
- Windows:注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- 或用 `tauri-plugin-autostart`(同时给 macOS 用 `LaunchAgents`)
- 设置面板加一个开关

## 测试路径

- 本地无 Windows 开发机时,push tag 触发 `.github/workflows/release.yml`,在 `windows-latest` 跑 `tauri-action`,产出 `.msi` / `.exe`
- 实际行为(EX_NOACTIVATE 是否生效、Mica 是否显示、IME 候选词是否正常)需要在 Windows 物理机上跑一遍验证
