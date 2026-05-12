# Changelog

## 0.1.3

- 新增：Prompt 模板库——新建 AI 润色 Profile 时可从 4 个内置模板挑一个一键预填，不用自己从头写 prompt。**Claude Code 指令**（重点）：纠错 + 去口头禅 + 保留代码标识符/文件路径/英文术语原样 + 不扩写臆测，专为给 Claude Code 下达编程任务优化；另外还有「只纠错不改写」「口语 → 规范书面中文」「中译英」三个模板
- 新增：输出方式可选——默认仍是 `input`（enigo 自动输入，与老版一致），新增 `panel` 选项让识别结果停留在悬浮窗 textarea，用户可以就地编辑、复制、关闭；给 enigo 在某些 Electron app 失灵或需要二次确认的场景兜底
- 新增：快捷键按一下录入——设置页快捷键框旁「⌨ 录入」按钮，点一下后按目标组合键自动填入 `cmd+shift+f5` 格式（用 `e.code` 生成主键字符串，不受 shift 大小写/输入法影响），ESC 取消；录入期间临时 suspend 全局热键避免冲突，不用手打字符串
- 新增：push-to-talk 模式——录音参数页「按住说话」开关（默认关），按下组合键开始录、松开任一键自动停；适合短句和边界明确的场景。与原 toggle 模式「按一下开始、再按一下结束」并存可切
- 新增：配置一键导入/导出——关于页「🔄 配置备份」卡片，导出写 pretty JSON 到文件；导入先校验新热键可注册（避免导入后失效）→ save → 重注册 → replace AppState → 广播 config-updated。换机器 / 备份 / 分享配置用
- 改：Claude Code 模板加极短输入保护——ASR 偶尔返回 "。"、"嗯"、"好" 等 <10 字无意义文本时直接原样返回，不再让 LLM 基于无意义输入幻觉出一整段 Python 代码
- 改：悬浮窗 panel 模式结果从 `<div>` 换成 `<textarea>`——就地编辑识别文字再复制粘贴；NSPanel 在点 textarea 时走 `becomes_key_only_if_needed=true` 自动接收键盘输入
- 改：悬浮窗底部热键提示改为从 `recording-started` event payload 动态渲染，跟随配置实时更新；之前硬编码「再按 ⌘⇧F5 结束」，改快捷键后不同步
- 改：录音 / 处理 / 结果三阶段悬浮窗独立 view——新增 processing view（spinner + "正在处理…"），录音停止切 processing，ASR 最终文本到达切 result；input 模式下很快 hide，panel 模式等润色完再切 result
- 改：macOS 上录入热键显示 `option` / `cmd`（之前显示 `alt` / `win`，看着像 Windows 命名）；底层 Rust hotkey 解析两种命名都认，兼容老配置
- 修复：Windows 黑框 + 热键注册改进（Pressed/Released 区分、组合键松开任一键都触发 Released）
- 修复：panel 模式悬浮窗卡在"正在润色"——`recording-stopped` 和 `asr-final-text` event 顺序修正，`run()` 内部先 emit `recording-stopped` 再 emit `asr-final-text`，indicator 才不会最后停在 processing view
- 修复：悬浮窗波形不显示——canvas 嵌套进 `#recording-view` 多了一层，module load 时 layout 还没算完，`clientWidth` 读到 0，backing store 写成 0×0，柱子画到空 canvas 上用户完全看不到；改成首帧 `render()` 时才 resize backing store + `ctx.scale(dpr)`，尺寸不变时 skip 避免每帧 scale 导致柱子越来越大
- 修复：Retina 屏悬浮窗波形不显示 + 尺寸错乱——HTML `<canvas width="920" height="120">` 属性恰好等于 `dpr=2` 时 CSS 尺寸 × 2，`resizeCanvasIfNeeded` 尺寸相等 early-return 跳过 `ctx.scale(dpr)`；去掉 HTML 尺寸属性强制首次一定 resize + scale
- 修复：panel 模式悬浮窗底部按钮被裁——`indicator.rs` 窗口高度 140 → 180；`#result-view` 加 flex column + align-items:center + gap 让按钮居中整齐
- 修复：悬浮窗 view 切换失效——`[hidden]` 被 `#foo { display: flex }` ID 选择器覆盖（UA stylesheet `[hidden] { display: none }` 特异性 0,0,1,0 输给 ID 的 0,1,0,0），三个 view 永远都显示；给三个 view 加 `#foo[hidden] { display: none }` (0,1,1,0) 反超
- 修复：`suspend_hotkey` 没同步清 `AppState.registered_hotkey`，resume 时 `register_hotkey` 以为还有旧 accel 需要 unregister，多发 warn 日志
- 修复：`import_config` 顺序修正——预校验 hotkey 语法 → save 到磁盘 → register 真正注册 → replace AppState。之前顺序下 save 失败时磁盘是旧、内存是旧、系统热键已改，三处不一致
- 修复：panic-safe——`RecordingGuard::drop` 补 `active.store(false)`，`run().await` panic 时 active 不再卡 true 让下次 toggle 误判「还在录音」
- 构建：clippy `-D warnings` 在 `objc 0.2` 的 `cargo-clippy` cfg 上过不了——给 cfg 检查加白名单
- 重构：`/simplify` 清理最近 5 个 feature 的重复与瑕疵——抽 `fileDialogHelpers.ts`（save/read text 给 AboutView 配置导入导出、SettingsView 热词 CSV 导入导出共用）、`keyCodeToName()`（HotkeyRecorder 的 `KeyA→a` / `Digit1→1` / `F5→f5` 映射从内联移进来）、`POLISH_MODE_*` / `OUTPUT_MODE_*` 常量；HotkeyRecorder 的 useEffect 依赖去掉 onChange，改用 onChangeRef 避免父组件每次 render 新 onChange 触发 effect teardown/setup 频繁调 suspend/resume
- 文档：README 勘误——辅助功能恢复权限要「**减号删除 → 加号重加**」（不是取消勾选再勾选），因为 TCC 的 csreq 绑在 entry 上，单纯 toggle 开关不会更新 csreq

## 0.1.2

- 新增：VAD 静音自动停止录音——检测到说话起点后，连续静音超过阈值就自动结束，不用再按一次热键；录音参数页可开关 + 调静音时长（0.5–5.0 秒）和音量触发阈值；默认关闭，想用在设置里打开
- VAD 的 spike 惩罚：环境偶尔冒出的键盘声/椅子声不会再把静音累计清零，防止"永远停不下来"
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
- 改：sidebar 副标题的快捷键 `按 ⌘⇧F5` 跟随配置动态显示，改快捷键后立即同步（之前是硬编码）

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
