# Windows 版待补齐

Rust + Tauri 现在 macOS 已经全功能可用。Windows 等价进度记录在这里。

## ✅ 已完成

- **快捷键符号显示** —— `src/lib/hotkey.ts` 用 `IS_MAC` 自动切 `SYMBOLS_MAC` / `SYMBOLS_WIN`,indicator + KbdCombo 都走它
- **MSI + NSIS 双格式打包** —— `.github/workflows/ci.yml` + `release.yml` 在 `windows-latest` 上已经能产出 `.msi` / `.exe`
- **悬浮窗不抢焦点** —— `indicator.rs` Windows 分支用 `windows` crate 调 `SetWindowLongPtrW` 设 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`
- **窗口透明背景 + 毛玻璃** —— `indicator.rs` / `result.rs` Windows 分支用 `window-vibrancy::apply_mica`,失败回退 `apply_acrylic`(Win11 用 Mica,Win10 用 Acrylic;CSS `backdrop-filter` 是最后兜底)
- **代码签名** —— 实测 Windows 上正常安装,SmartScreen 不阻断
- **跨平台 keyboard backend** —— 替换 tauri-plugin-global-shortcut,Windows 走 `WH_KEYBOARD_LL`(handy-keys 0.2 封装),支持 toggle / push-to-talk / double-tap-hold 三模式。**首次发布前要处理 AV 误报**:`SetWindowsHookExW` 是 keylogger 的标志 API,360 / 火绒 / 腾讯电脑管家几乎必标。缓解流程:① EV Code Signing(~2000 RMB/年)② 提报各家 AV 白名单(各 1-3 周审核)③ 安装包独立签名 ④ UI 引导用户手动加白名单(详见 `[[project-custom-keyboard-backend]]` memory)

## 🟡 锦上添花(待办,不阻塞首发)

### 自启动
- Windows:注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- 或用 `tauri-plugin-autostart`(同时给 macOS 用 `LaunchAgents`)
- 设置面板加一个开关

## 测试路径

- 本地无 Windows 开发机时,push tag 触发 `.github/workflows/release.yml`,在 `windows-latest` 跑 `tauri-action`,产出 `.msi` / `.exe`
- 实际行为(EX_NOACTIVATE 是否生效、Mica 是否显示、IME 候选词是否正常、keyboard backend 三模式)需要在 Windows 物理机上跑一遍验证
- **keyboard backend Win 端验证清单**:① toggle 主热键 + ESC 取消 ② PushToTalk 按住松开 ③ DoubleTapHold(注意 right_option = right Alt,德/法语 AltGr 键盘布局可能冲突,建议 Win 用户优先选其他 modifier)④ sleep / 锁屏唤醒后再触发热键正常 ⑤ 30 次 hot reload(改 trigger_mode/hotkey)无线程泄漏
