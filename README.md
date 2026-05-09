# voice-claude

按一下热键说话，再按一下结束，识别的文字自动输入到当前焦点窗口。macOS / Windows 跨平台，专为办公室气声输入设计。

数据流：**按热键 → 录音 → PCM 增益 → ASR 转写 → AI 纠错（可选）→ 热词替换 → 模拟键盘输入**

## 功能

- **菜单栏常驻**，不占 Dock，全程后台
- **悬浮波形窗**：Raycast 风格渐变色波形，macOS 原生 NSPanel 真·不抢焦点
- **5 种 ASR 后端**：本地 SenseVoice（离线）、讯飞 / 豆包（实时流式）、智谱 / OpenRouter Whisper（准确优先）
- **AI 纠错**：可选接 Ollama 本地模型 / OpenRouter / 任意兼容 OpenAI API 的云端
- **热词替换**：专有名词自动修正（克劳德 → Claude 等）
- **历史记录**：SQLite 本地存储，托盘菜单最近 5 条点击复制
- **录音提示音**：开始叮、结束啵
- **设置自动保存**：改完即生效，无需手动保存

## 安装

### macOS：从 Releases 下载（推荐）

从 [Releases](https://github.com/HaoKunT/voice-claude/releases/latest) 下载 `voice-claude_x.x.x_aarch64.dmg`（Apple Silicon）。

因为没有 Apple Developer 证书，macOS 会把 app 标记为"不受信任"。需要一条命令解除隔离：

```bash
# 1. 双击 .dmg，把 voice-claude 拖进 Applications
# 2. 在终端执行（绕过 Gatekeeper）：
xattr -dr com.apple.quarantine /Applications/voice-claude.app

# 3. 启动 app
open /Applications/voice-claude.app
```

启动后会依次请求：
1. **麦克风权限** — 点"允许"
2. **辅助功能权限** — 系统设置 → 隐私与安全性 → 辅助功能 → 点 `+` → 选 `/Applications/voice-claude.app` → 勾选
3. **菜单栏图标**出现后点击 → 设置 → 填 ASR API Key 或下载本地 SenseVoice 模型
4. **默认热键** `Cmd+Shift+F5`：按一下说话，再按一下结束

### macOS：从源码构建

```bash
git clone https://github.com/HaoKunT/voice-claude.git
cd voice-claude
make install   # 编译 + 打包 + 自动签名 + 安装到 /Applications
```

源码构建会自动做稳定签名，不需要 xattr。**前置依赖**：Rust stable + Node.js 20+ + pnpm。

### Windows

从 [Releases](https://github.com/HaoKunT/voice-claude/releases/latest) 下载 `voice-claude_x.x.x_x64-setup.exe`（NSIS 安装包）或 `.msi`。

> Windows 版 NSPanel 等价尚未实现（悬浮窗会抢焦点），详见 `rust/WINDOWS_TODO.md`。

## 快速上手

1. 启动 `voice-claude.app`，菜单栏出现图标
2. 点图标 → **设置**，选 ASR 后端填 API Key（改完即自动保存）
3. 默认热键 `Cmd+Shift+F5`（**按一下开始说话，再按一下结束**）

## 已知限制（macOS ad-hoc 签名）

项目没有 Apple Developer 证书（99 美元/年），app 走 **ad-hoc 自签**。几个由此而来的限制：

1. **首次安装需手动解除隔离**：浏览器下载的 `.app` 会被 Gatekeeper 标 quarantine，跑一次：
   ```bash
   xattr -dr com.apple.quarantine /Applications/voice-claude.app
   ```
   > 小技巧：改用 `curl -LO` 下载产物（而不是浏览器），quarantine 属性不会被打上，这一步可以省。

2. **更新后需要重新勾选「辅助功能」**：macOS 的 TCC 权限绑在 code signature 的 cdhash 上。每次新版本 cdhash 都变，系统认为是"另一个" app，之前授予的**辅助功能**和可能的**麦克风**权限会失效。需要到 **系统设置 → 隐私与安全性 → 辅助功能** 重新勾选一次 voice-claude（启动 app 后，如果辅助功能未授权，主窗口顶部会有横条提示 + 一键跳转按钮）。

3. **应用内自动更新**本身是能用的：启动后自动检查 GitHub Release 的 `latest.json`，有新版在「关于」页显示下载按钮，点一下就替换、重启；文件替换过程无感，**但 TCC 权限不会继承**（同上）。

这些都是 macOS 系统级行为，不是 bug。根治方案是 Apple Developer + notarize，未来考虑。Windows 下的 SmartScreen 有类似 "未知发行商" 拦截，首次运行点"仍要运行"。

## ASR 后端

| 后端 | 模式 | 说明 |
|---|---|---|
| 本地 SenseVoice | 离线 | 约 1GB 模型，隐私最佳，需手动下载 |
| 讯飞 | 流式 | 需要 AppID + AccessKey，边说边出字 |
| 豆包 / 火山 | 流式 | 效果最好，注册送 40 小时额度 |
| 智谱 | 批处理 | 免费额度充足 |
| OpenRouter | 批处理 | Whisper large-v3-turbo |

## AI 纠错

可选，在设置里开启：

- **Ollama** - 本地模型（推荐 qwen2.5:3b）
- **OpenRouter** - 云端任意模型（共用 ASR 的 API Key）
- **Cloud** - 兼容 OpenAI API 的任意服务

## 构建

```bash
make dev           # 开发模式（热重载）
make build         # macOS 打包 .app
make install       # 编译 + 安装到 /Applications
make build-win     # Windows 打包 .msi + .exe
make test          # cargo test + 前端 typecheck
make lint          # clippy + fmt --check
make fmt           # cargo fmt
make clean         # 清理构建产物
make uninstall     # 从 /Applications 卸载
```

## 项目结构

```
rust/                       # Rust + Tauri 主线代码
├── src/                    # React + TypeScript 前端
│   ├── views/              # SettingsView / HistoryView
│   ├── indicator.tsx       # 悬浮波形窗
│   └── api.ts              # Tauri IPC 封装
├── src-tauri/
│   └── src/
│       ├── asr/            # 5 种 ASR 后端
│       ├── audio.rs        # cpal 录音 + 降采样
│       ├── config.rs       # 配置
│       ├── correct.rs      # AI 纠错
│       ├── history.rs      # SQLite 历史
│       ├── hotwords.rs     # 热词
│       ├── indicator.rs    # NSPanel 悬浮窗
│       ├── input.rs        # enigo 键盘模拟
│       ├── recorder.rs     # 主流程
│       ├── tray.rs         # 系统托盘
│       └── beep.rs         # 提示音
└── indicator.html          # 悬浮窗 HTML

legacy/go/                  # Go + Fyne 老版归档（不再维护）
```

## 技术栈

- **后端**：Rust + Tauri v2 + tokio + cpal + enigo + rusqlite
- **前端**：React + TypeScript + Tailwind CSS + Vite
- **悬浮窗**：tauri-nspanel（macOS 真·不抢焦点）
- **离线 ASR**：sherpa-onnx（SenseVoice）
- **打包**：macOS .app / .dmg，Windows .msi / .exe（Tauri bundler）

## 气声输入技巧

- 麦杆贴近嘴角 2-3 cm，不振动声带只送气
- 说话速度适中，增益调到 3-5x 可覆盖大多数气声场景
- 推荐带麦杆的有线 USB 耳麦（EPOS IMPACT 400 / Jabra Evolve2 40）

## 变更历史

完整版本变更记录见 [CHANGELOG.md](./CHANGELOG.md)。

## License

MIT
