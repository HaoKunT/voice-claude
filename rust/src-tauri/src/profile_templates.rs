//! 内置 polish profile 模板 —— 真源在 Rust 这一侧，前端只是镜像展示。
//!
//! 设计:
//! - 每个模板有稳定 id(`claude-code` / `fix-only` / `colloquial-to-written` / `zh-to-en`)
//! - PolishProfile.template_id = Some(id) 时该 profile 是「内置模板」,prompt 字段
//!   被 ignore,prompt 走 `current_prompt()` 从这里读 —— 这样升级新模板老用户自动同步
//! - 用户想改:在前端按"复制为自定义版本",会 clear template_id 并把当前 prompt
//!   文本物化到 PolishProfile.prompt 字段
//!
//! 历史版本:每个模板同时维护 `legacy_prompts()` —— 老 config.json 里的 prompt
//! 文本如果跟历史 fingerprint 匹配上,会自动迁移到 builtin(set template_id, clear prompt)。

use crate::config::{PolishProfile, POLISH_MODE_OLLAMA};

#[derive(Debug, Clone)]
pub struct ProfileTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub mode: &'static str,
    pub prompt: &'static str,
}

pub const TEMPLATE_CLAUDE_CODE: &str = "claude-code";
pub const TEMPLATE_FIX_ONLY: &str = "fix-only";
pub const TEMPLATE_COLLOQUIAL_TO_WRITTEN: &str = "colloquial-to-written";
pub const TEMPLATE_ZH_TO_EN: &str = "zh-to-en";

const PROMPT_CLAUDE_CODE: &str = r#"你是一个语音输入润色助手。用户正在用语音给 Claude Code 下达编程任务，
你的输出会被直接喂给 Claude Code 执行，所以优先保证**精准、简洁**。

**极端短 / 无实质内容的情况**(原文只有"。"、"嗯"、"好"这种十个字以内
的无意义文本):**直接原样返回原文**,绝对不要编造内容。

请做：
1. 修正同音字 / 漏字 / 多字错误（尤其编程术语：变量 / 函数 / 库 / 框架名等）
2. 去掉口头禅和填充词（"嗯"、"那个"、"然后"、"帮我看一下"、"啊这个"等）
3. 保留所有代码标识符、文件路径、英文术语、命令参数**原样不动**
4. 保留技术语境的精确表述（比如"二分查找"不要改成"二分法查找"这种无意义改写）
5. 遇到下面识别词典里有的词，**优先按词典里的写法 / 拼写输出**(尤其同音英文项目名、人名)

不要做：
- 不要扩写、不要补充用户没说的细节
- 不要臆测意图
- 不要加"好的"、"明白"这类回应
- 不要加 markdown 标题或额外解释

如果原文已经清晰简洁，直接原样返回。只输出润色后的指令，一个字都别多。

{glossary}

原文：
{text}"#;

const PROMPT_FIX_ONLY: &str = r#"你是一个语音识别纠错助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
遇到下面识别词典里有的词，优先按词典写法输出（同音字 / 跨语种映射场景关键）。
如果原文没有明显错误，直接返回原文。
只输出纠正后的文本，不要解释。

{glossary}

原文：{text}"#;

const PROMPT_COLLOQUIAL_TO_WRITTEN: &str = r#"你是一个语音润色助手。用户通过语音输入一段话，你需要把它转为规范、通顺的书面中文：

1. 纠正同音字和漏字
2. 去掉"嗯"、"那个"、"然后"、"就是"、"呃"等口头禅和填充词
3. 调整句子结构让表达更清晰（但**保留用户的原意**，不要扩写）
4. 保持语气自然，不要过度正式化
5. 遇到下面识别词典里有的术语 / 专名，按词典的写法输出

只输出润色后的文本，不要解释。

{glossary}

原文：{text}"#;

const PROMPT_ZH_TO_EN: &str = r#"用户用中文语音输入一段话，你需要：

1. 先在心里修正中文的同音字和漏字错误（不需要输出修正后的中文）
2. 然后把这段话翻译成自然、地道的英文
3. 保持原意和语气（不要过度正式化）
4. 遇到下面识别词典里有的术语 / 专名，翻译成英文时直接用词典里的写法
   （例：词典含 "Claude" 时，中文里说的"克劳德"译成 "Claude"）

只输出英文译文，不要解释，不要加双语对照。

{glossary}

原文：{text}"#;

pub const ALL_TEMPLATES: &[ProfileTemplate] = &[
    ProfileTemplate {
        id: TEMPLATE_CLAUDE_CODE,
        name: "Claude Code 指令",
        description: "纠错 + 去口头禅 + 保留代码标识符原样,专为给 Claude Code 下达编程任务优化",
        mode: POLISH_MODE_OLLAMA,
        prompt: PROMPT_CLAUDE_CODE,
    },
    ProfileTemplate {
        id: TEMPLATE_FIX_ONLY,
        name: "只纠错,不改写",
        description: "只修同音字/漏字/多字,保留原意和用语",
        mode: POLISH_MODE_OLLAMA,
        prompt: PROMPT_FIX_ONLY,
    },
    ProfileTemplate {
        id: TEMPLATE_COLLOQUIAL_TO_WRITTEN,
        name: "口语 → 规范书面中文",
        description: "去掉「嗯啊然后」等口头禅,变通顺的书面语;适合写文档 / 邮件 / 报告",
        mode: POLISH_MODE_OLLAMA,
        prompt: PROMPT_COLLOQUIAL_TO_WRITTEN,
    },
    ProfileTemplate {
        id: TEMPLATE_ZH_TO_EN,
        name: "中译英",
        description: "中文说、英文出——先纠错中文,再翻译成自然英文",
        mode: POLISH_MODE_OLLAMA,
        prompt: PROMPT_ZH_TO_EN,
    },
];

pub fn get(id: &str) -> Option<&'static ProfileTemplate> {
    ALL_TEMPLATES.iter().find(|t| t.id == id)
}

/// 历史 prompt 文本,用于迁移老 config.json:
/// 用户的 profile.prompt 跟某个历史/当前模板能匹配上时,自动转成 builtin
/// (set template_id + clear prompt 字段)。
///
/// fingerprint 比较用 normalize 后的全文 ——
/// 不做"包含关系",避免误把用户在模板上轻微魔改的 prompt 当成 builtin 强行替换。
fn legacy_prompts_for(template_id: &str) -> &'static [&'static str] {
    match template_id {
        TEMPLATE_CLAUDE_CODE => CLAUDE_CODE_LEGACY,
        TEMPLATE_FIX_ONLY => FIX_ONLY_LEGACY,
        TEMPLATE_COLLOQUIAL_TO_WRITTEN => COLLOQUIAL_LEGACY,
        TEMPLATE_ZH_TO_EN => ZH_TO_EN_LEGACY,
        _ => &[],
    }
}

/// 021f3a7 之前版本(只有 {text} 占位符,没有 {glossary}/识别词典指令)
const CLAUDE_CODE_PRE_021F3A7: &str = r#"你是一个语音输入润色助手。用户正在用语音给 Claude Code 下达编程任务，
你的输出会被直接喂给 Claude Code 执行，所以优先保证**精准、简洁**。

请做：
1. 修正同音字 / 漏字 / 多字错误（尤其编程术语：变量 / 函数 / 库 / 框架名等）
2. 去掉口头禅和填充词（"嗯"、"那个"、"然后"、"帮我看一下"、"啊这个"等）
3. 保留所有代码标识符、文件路径、英文术语、命令参数**原样不动**
4. 保留技术语境的精确表述（比如"二分查找"不要改成"二分法查找"这种无意义改写）

不要做：
- 不要扩写、不要补充用户没说的细节
- 不要臆测意图
- 不要加"好的"、"明白"这类回应
- 不要加 markdown 标题或额外解释

如果原文已经清晰简洁，直接原样返回。只输出润色后的指令，一个字都别多。

原文：
{text}"#;

const FIX_ONLY_PRE_021F3A7: &str = r#"你是一个语音识别纠错助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
如果原文没有明显错误，直接返回原文。
只输出纠正后的文本，不要解释。

原文：{text}"#;

const COLLOQUIAL_PRE_021F3A7: &str = r#"你是一个语音润色助手。用户通过语音输入一段话，你需要把它转为规范、通顺的书面中文：

1. 纠正同音字和漏字
2. 去掉"嗯"、"那个"、"然后"、"就是"、"呃"等口头禅和填充词
3. 调整句子结构让表达更清晰（但**保留用户的原意**，不要扩写）
4. 保持语气自然，不要过度正式化

只输出润色后的文本，不要解释。

原文：{text}"#;

const ZH_TO_EN_PRE_021F3A7: &str = r#"用户用中文语音输入一段话，你需要：

1. 先在心里修正中文的同音字和漏字错误（不需要输出修正后的中文）
2. 然后把这段话翻译成自然、地道的英文
3. 保持原意和语气（不要过度正式化）

只输出英文译文，不要解释，不要加双语对照。

原文：{text}"#;

const CLAUDE_CODE_LEGACY: &[&str] = &[CLAUDE_CODE_PRE_021F3A7];
const FIX_ONLY_LEGACY: &[&str] = &[FIX_ONLY_PRE_021F3A7];
const COLLOQUIAL_LEGACY: &[&str] = &[COLLOQUIAL_PRE_021F3A7];
const ZH_TO_EN_LEGACY: &[&str] = &[ZH_TO_EN_PRE_021F3A7];

/// 用 normalize 后的全文比对 prompt 是不是某个 builtin 模板的当前/历史版本。
/// 命中 → 返回 template_id;未命中 → None,保留为 custom。
pub fn detect_template_id(prompt: &str) -> Option<&'static str> {
    let target = normalize_prompt(prompt);
    for tpl in ALL_TEMPLATES {
        if normalize_prompt(tpl.prompt) == target {
            return Some(tpl.id);
        }
        for legacy in legacy_prompts_for(tpl.id) {
            if normalize_prompt(legacy) == target {
                return Some(tpl.id);
            }
        }
    }
    None
}

/// 容错对比:统一 CRLF、去首尾空白、压缩内部连续空白(空格+换行+tab)。
/// 这样手动从前端拷过来 / 序列化往返时多一个空格不影响识别。
fn normalize_prompt(s: &str) -> String {
    let s = s.replace("\r\n", "\n");
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

/// 给 PolishProfile 用 —— template_id 命中且模板还存在时返回模板 prompt;
/// 否则 fallback 到 profile 自己的 prompt 字段(custom 或被删除的 builtin)。
pub fn effective_prompt(profile: &PolishProfile) -> &str {
    if let Some(id) = profile.template_id.as_deref() {
        if let Some(tpl) = get(id) {
            return tpl.prompt;
        }
    }
    &profile.prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_current_template() {
        assert_eq!(
            detect_template_id(PROMPT_CLAUDE_CODE),
            Some(TEMPLATE_CLAUDE_CODE)
        );
        assert_eq!(detect_template_id(PROMPT_FIX_ONLY), Some(TEMPLATE_FIX_ONLY));
    }

    #[test]
    fn detect_legacy_pre_021f3a7() {
        assert_eq!(
            detect_template_id(CLAUDE_CODE_PRE_021F3A7),
            Some(TEMPLATE_CLAUDE_CODE)
        );
        assert_eq!(
            detect_template_id(FIX_ONLY_PRE_021F3A7),
            Some(TEMPLATE_FIX_ONLY)
        );
        assert_eq!(
            detect_template_id(COLLOQUIAL_PRE_021F3A7),
            Some(TEMPLATE_COLLOQUIAL_TO_WRITTEN)
        );
        assert_eq!(
            detect_template_id(ZH_TO_EN_PRE_021F3A7),
            Some(TEMPLATE_ZH_TO_EN)
        );
    }

    #[test]
    fn detect_with_extra_whitespace() {
        let extra = "  你是一个语音识别纠错助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。\n请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。\n遇到下面识别词典里有的词，优先按词典写法输出（同音字 / 跨语种映射场景关键）。\n如果原文没有明显错误，直接返回原文。\n只输出纠正后的文本，不要解释。\n\n{glossary}\n\n原文：{text}  ";
        assert_eq!(detect_template_id(extra), Some(TEMPLATE_FIX_ONLY));
    }

    #[test]
    fn detect_user_modified_does_not_match() {
        let user_edited = format!("{}\n\n额外加一行用户自定义指令", PROMPT_FIX_ONLY);
        assert_eq!(detect_template_id(&user_edited), None);
    }

    #[test]
    fn detect_unknown_returns_none() {
        assert_eq!(detect_template_id("随便写的 prompt {text}"), None);
        assert_eq!(detect_template_id(""), None);
    }

    #[test]
    fn effective_prompt_uses_template_when_set() {
        let mut p = PolishProfile::default_named("x", "x");
        p.template_id = Some(TEMPLATE_FIX_ONLY.into());
        p.prompt = "should be ignored".into();
        assert_eq!(effective_prompt(&p), PROMPT_FIX_ONLY);
    }

    #[test]
    fn effective_prompt_falls_back_to_prompt_field() {
        let mut p = PolishProfile::default_named("x", "x");
        p.template_id = None;
        p.prompt = "custom prompt".into();
        assert_eq!(effective_prompt(&p), "custom prompt");
    }

    #[test]
    fn effective_prompt_falls_back_when_template_missing() {
        let mut p = PolishProfile::default_named("x", "x");
        p.template_id = Some("not-a-real-template".into());
        p.prompt = "fallback".into();
        assert_eq!(effective_prompt(&p), "fallback");
    }
}
