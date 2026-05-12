# voice-claude TODO

跨会话的产品/功能待办。本次能做完的用 TaskCreate 追,不在这里列。

## 产品设计待推进

### 变量插值 `{selected}` / `{clipboard}`(助手模式 Profile)

**目标**:把 voice-claude 从语音输入法升级成"光标处 AI 助手"。
典型场景:选中代码按热键说"解释这段" → AI 解释显示在悬浮窗。

**为什么搁置**:产品交互面比单纯"加变量"大很多,需要先把引导做好再动工。

**关键产品决策**:

- Profile 按 prompt 是否含变量自动分成两类
  - **输入法模式**(只有 `{text}`)→ 直接键入光标处
  - **助手模式**(含 `{selected}` / `{clipboard}`)→ 强制 panel 输出
- 助手模式下流式 ASR 降级为批处理(partial 会覆盖选中)
- 内置模板库新增"助手模式"分类(解释/翻译/重写/总结选中)
- Profile 编辑器的 prompt textarea 下显示变量 hint + 即时反馈("检测到 `{selected}`,本 Profile 已切换为助手模式")
- Profile 卡片右上角显示模式徽章

**待定问题**:

- 热键按下时没选中任何内容 → 通知并取消 / 静默继续 / 降级为 `{clipboard}`?
- Cmd+C 读选中的可靠性(Terminal/VSCode 某些模式响应慢)+ 原剪贴板恢复时机
- 历史记录是否保存 `{selected}` / `{clipboard}` 原文(与"重新润色"功能耦合)

**技术约束**:

- 按热键瞬间立即 Cmd+C 读剪贴板 → 存 `{selected}` → 延迟恢复原剪贴板(录音结束后)
- `correct.rs` 渲染 prompt 时检测变量 → 临时覆盖本次 `output_mode = panel`
- 跨平台读选中:macOS 走 Cmd+C trick,Windows 同等
