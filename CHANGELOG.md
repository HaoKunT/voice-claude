# Changelog

## Unreleased

- 新增：VAD 静音自动停止录音——检测到说话起点后，连续静音超过阈值就自动结束，不用再按一次热键；录音参数页可开关 + 调静音时长（0.5–5.0 秒）和音量触发阈值
- 改：原「AI 纠错」改名为「AI 润色」，支持多个 profile（每个 profile 一套完整的后端 + 模型 + API Key + prompt 模板）；按场景切换，比如润色 / 改邮件风格 / 翻译等
- 自动迁移：首次启动时把老的 `correct_*` 字段搬进一个名为「默认」的 profile，用户无感升级
- Prompt 模板支持 `{text}` 占位符，替换为识别原文；没有占位符则把原文追加到末尾
- 新增：SenseVoice 下载地址和 SHA256 在设置页直接可见可复制；针对国内从 GitHub 下不动的场景，增加「📦 导入本地压缩包」按钮——自己用迅雷等工具下好 `.tar.bz2` 后一键导入，自动校验 SHA256 并解压
- 改：下载进度显示字节数 + 实时速率 + 剩余时间，而不是只有百分比
- 修复：快捷键改完后实际不生效——之前 setup 时注册一次就再也不更新。现在 `save_config` 检测到 hotkey 变了会自动 unregister 旧的、注册新的
- 修复：日志级别改完不生效——tracing EnvFilter 以前是静态的，现在用 `reload::Layer` 热替换，设置页改即生效
- 修复：日志文件名 `voice-claude.log.YYYY-MM-DD` → `voice-claude.YYYY-MM-DD.log`，`.log` 放在末尾让 macOS 能识别为文本文件直接打开；重命名历史文件保留；打开「最新日志」会扫目录找 mtime 最新的
- 新增：日志页底部「最近日志」实时预览——最后 200 行滚动列表，按级别过滤 + 彩色显示（ERROR 红 / WARN 黄 / INFO 蓝 / DEBUG 灰）+ 自动刷新（2 秒）。不用跳 Finder 开文件就能看到应用刚做了什么
- 换 logo：从简陋的 V 字母渐变换成线描风的对话气泡 + 语音波形（紫/粉/蓝三色描边），叠在深色圆角方形底上；替换了全套 macOS / Windows / iOS / Android icon，sidebar 和关于页的 V 字母方块也换成 app icon，系统菜单栏专用单色 template 版本单独做
- 改：`scripts/regen-icons.sh` 一键重生 —— 换 logo 源（webp/png/svg 都认）就跑一行命令，app icon + 菜单栏 tray + UI 内 logo 全部同步

## 0.1.1

- 新增：应用内自动更新（Tauri updater + GitHub Release `latest.json`）——启动 1 秒后自动检查，有新版在关于页显示下载按钮，点一下自动下载并重启；ad-hoc 签名下 reqwest 下载的新 app 不带 quarantine，重启无感
- 新增：辅助功能权限丢失检测——缺授权时主窗口顶部黄色横条提示 + 一键跳「系统设置 → 隐私与安全性 → 辅助功能」；用户授权完切回 app，横条自动消失
- 新增：关于页完整版本信息（git commit hash、rustc 版本、tauri 版本、target、构建时间）+ MIT 许可证链接
- 新增：热词批量 CSV 导入导出（支持引号包裹、特殊字符转义），合并或替换二选一
- 新增：悬浮窗实时显示 ASR 识别文字 + 录音时长计时器（超过 60 秒变黄提醒）
- 新增：历史记录每条展示录音时长
- 修复：悬浮窗白底覆盖透明背景（重新启用 `macOSPrivateApi` + 对底层 `NSWindow` 发 `setOpaque:NO` 和 `setBackgroundColor:[NSColor clearColor]`）
- 调整：设置页从顶部 tab 改为左侧平铺菜单（语音识别 / AI 纠错 / 录音参数 / 热词 / 日志）；历史记录和关于作为独立一级入口，不再贴底
- 调整：关于页 License 卡片只保留 MIT 链接，不再铺全文许可证
- 调整：日志改为 daily rotation + 7 天自动清理（替代旧版会单文件无限增长到几百 MB 的行为）
- 构建：GitHub Actions 发版流程自动化——matrix 打 macOS / Windows，`TAURI_SIGNING_PRIVATE_KEY` secret 注入生成 `.sig`，aggregation job 跑脚本聚合 `latest.json` 并上传 Release，draft 自动转 published 并强制标 latest
- 构建：release notes 自动生成——`scripts/gen-release-notes.mjs` 按 conventional commit 前缀分组（feat / fix / docs / ui / build / ci）渲染成 markdown，workflow 调用后覆盖 release body
- 文档：补 `LICENSE` 文件（MIT）
- 文档：README 新增「已知限制」节，说清 ad-hoc 签名与 TCC 权限的关系——首次安装需 `xattr -dr`（或用 `curl -LO` 下载规避）、每次更新后辅助功能权限通常需重勾

## 0.1.0

首版 Rust + Tauri 2 发布，从 Go + Fyne 版完整重写。

- 五种 ASR 后端：本地 SenseVoice（离线，基于 sherpa-onnx，约 1 GB 模型）、讯飞（WebSocket 实时流式）、豆包 / 火山（WebSocket 二进制帧实时流式）、智谱（HTTP 批处理 + 自动分段）、OpenRouter Whisper（HTTP 批处理）
- Raycast 风格深色毛玻璃 UI，三色渐变强调
- macOS 原生 NSPanel 悬浮波形窗（`tauri-nspanel`），真·不抢焦点，录音时目标窗口保持键盘焦点
- AI 纠错可选：Ollama 本地 / OpenRouter 云端 / 任意兼容 OpenAI API 的云端
- 热词替换、SQLite 历史记录（最近 200 条）、系统托盘最近 5 条一键复制
- 麦克风增益 1–10 倍，专为气声输入调校
- 跨平台：macOS（主要支持）+ Windows（NSPanel 等价适配待补，悬浮窗会抢焦点）
