//! LLM 单层提取 —— 把对话原文整段喂给用户配置的 LLM 后端,让
//! LLM 自己从原文里识别值得加入语音识别词典的术语 / 专名(中英文混合)。
//!
//! **设计**:不再做本地分词 + 频率统计 + 停用词过滤的两层结构。本地分词
//! 只能切英文 / ASCII token,中文专名完全没机会进候选,LLM 看到一份全
//! 英文候选列表也跟着只挑英文 —— 反而成了误导。删掉本地层后由 LLM 直接
//! 从原文挑词,中英文都能出。
//!
//! **架构 note**:0.4 起 LLM 调用统一走 `crate::llm_client`,跟 polish 流程
//! 共用同一份 backend 配置(url/model/api_key/mode),各自的 prompt 由调用方
//! 渲染后传入。这里只剩下 prompt 模板 + 响应解析。

use crate::config::LlmBackend;
use crate::llm_client;
use anyhow::Result;

/// 给 LLM 的对话原文最长字符数。云端大模型(Claude / GPT-4 / OpenRouter 上常用 32k+)
/// 都能容下;ollama 用户得自己保证 num_ctx 够大,默认 2048 撑不住,需要在 backend
/// url 里加 num_ctx 参数,或者用更小的天数 / 缩小这个值。
const MAX_RAW_TEXT_CHARS: usize = 30_000;

/// 把对话原文整段交给 LLM,让它识别并提取值得加入识别词典的术语 / 专名,
/// 返回 LLM 推荐的词列表(中英混合)。
///
/// - `raw_text`:对话原文(经 source 抽取 + 工具调用过滤 + thinking 过滤)
/// - `existing`:用户当前 cfg.hotwords,告知 LLM 不要重复推荐
/// - `backend`:LLM 后端连接(url/model/api_key/mode)
/// - `timeout_secs`:HTTP 超时
pub async fn extract_hotwords(
    raw_text: &str,
    existing: &[String],
    backend: &LlmBackend,
    timeout_secs: u64,
) -> Result<Vec<String>> {
    if !backend.is_active() {
        anyhow::bail!(
            "backend 「{}」mode 是 off,LLM 提取需要一个有效的后端",
            backend.name
        );
    }
    let prompt = build_prompt(raw_text, existing);
    let response = llm_client::call(backend, &prompt, timeout_secs).await?;
    Ok(parse_word_list(&response))
}

/// 取文本末尾不超过 `max_chars` 个字符。中文按 char 计算,不会切坏 UTF-8。
/// 末尾段落更代表用户当前关注的领域,优于头部 / 中间段落。
pub fn take_tail_excerpt(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().skip(total - max_chars).collect()
}

/// 截取原文末尾 `MAX_RAW_TEXT_CHARS` 字符给 LLM。封装一层方便外部用。
pub fn truncate_for_llm(text: &str) -> String {
    take_tail_excerpt(text, MAX_RAW_TEXT_CHARS)
}

/// 构造 prompt:让 LLM 从原文中挑词,返回 JSON array of strings。
/// 中文专名比英文优先(英文 ASR 一般不出错,中文专名经常错)。
/// 数量控制在 20-50,质量优先 —— 多了反而让用户筛得累。
fn build_prompt(raw_text: &str, existing: &[String]) -> String {
    let mut s = String::from(
        "你是语音识别词典审核员。下面是用户最近一段时间的 Claude Code 对话原文\
         (只含用户问题 + Claude 文字回复,工具调用 / 工具结果已过滤)。\n\n\
         任务:从原文里挑值得加入语音识别词典(ASR boosting / LLM 跨语种映射用)的术语,\
         返回 JSON array of strings。**质量优先,宁缺勿滥** —— 数量控制在 20-50 个。\n\n\
         **加入标准**(必须满足之一):\n\
         1. 专有名词、项目名、产品名、公司名(中英都要):\n\
            英文例:Claude / voice-claude / Anthropic / FireRedASR / sherpa-onnx / CGEventTap\n\
            中文例:克劳德 / 豆包 / 飞书 / 通义千问 / 思源笔记\n\
         2. 中文人名 / 中文领域术语(优先 —— 中文专名 ASR 容易错)\n\
         3. 易被语音识别错的英文技术名词(混读 / 拼写复杂的)\n\n\
         **绝对排除**(出现一个就丢,不要怀疑):\n\
         - 普通英文单词:about, problem, system, file, user, list, value, result, name, request\n\
         - 中文虚词 / 高频普通词:这个, 那个, 然后, 觉得, 知道, 用户, 问题, 时候, 方法, 比如\n\
         - 编程通用关键字:function, return, class, var, async, await, struct, enum, trait, impl\n\
         - 缩写歧义大:API, SDK, URL, JSON, YAML(放词典帮助有限)\n\
         - 单字符 / 双字符 ASCII\n\
         - 路径片段:./xxx, src/xxx, /usr/local, ../foo\n\
         - 命令前缀:git xxx, npm xxx, cargo xxx, make xxx, docker xxx\n\
         - 文件名后缀:.rs, .ts, .json, .md\n\
         - 通用动词 / 形容词:create, update, delete, get, set, check, run, start, stop\n\n\
         **同义词归一**:同一个词的大小写变体只挑一个最常见的形式(比如 voice-claude 跟\
         Voice-Claude 二选一,不要两个都返回)。\n\n",
    );
    if !existing.is_empty() {
        // 词典里同一个词的大小写变体只列一次(case-insensitive 去重,保留首次出现
        // 的形式),且 prompt 文案明确告知 LLM 比对时忽略大小写 —— 防止 LLM 看到
        // "Claude" 后又把 "claude" 当成新词推荐(后处理会兜底过滤,但提前在
        // prompt 层就避免重复能省 token / 提高 LLM 判断准确度)。
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut deduped: Vec<&str> = Vec::new();
        for w in existing.iter() {
            if seen.insert(w.to_lowercase()) {
                deduped.push(w.as_str());
            }
        }
        s.push_str("已在词典里的词(不区分大小写,不要重复推荐 —— 大小写变体也算重复):\n");
        for w in deduped.iter().take(500) {
            s.push_str(&format!("- {}\n", w));
        }
        s.push('\n');
    }
    s.push_str(
        "原文(中英混合,从中挑符合标准的词):\n\
         ----\n",
    );
    s.push_str(raw_text);
    s.push_str(
        "\n----\n\n\
         请输出 JSON array of strings,**只**输出 JSON,不要任何解释 / markdown 包装。\n\
         **再次提醒**:质量优先 20-50 个;宁少勿多。中文专名优先。\n\
         例:[\"voice-claude\", \"Claude\", \"克劳德\", \"豆包\", \"FireRedASR\", \"sherpa-onnx\", \"思源笔记\"]\n",
    );
    s
}

/// 从 LLM 响应里提取 JSON array of strings。LLM 偶尔会在前后加 markdown
/// fence (```json ... ```),宽容解析。如果完全 parse 失败,降级为按行 / 逗号
/// 分割提取看起来像词的 token。
fn parse_word_list(response: &str) -> Vec<String> {
    let start = response.find('[');
    let end = response.rfind(']');
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            let candidate_json = &response[s..=e];
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(candidate_json) {
                return arr
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    // 降级:每行一个词
    response
        .lines()
        .map(|l| {
            l.trim()
                .trim_matches(|c: char| c == '-' || c == '"' || c == ',' || c == '*')
                .trim()
        })
        .filter(|l| !l.is_empty() && l.chars().count() <= 60 && !l.contains(' '))
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pure_json_array() {
        let r = parse_word_list("[\"foo\", \"bar\", \"baz\"]");
        assert_eq!(r, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn parse_with_markdown_fence() {
        let r = parse_word_list("```json\n[\"foo\", \"bar\"]\n```");
        assert_eq!(r, vec!["foo", "bar"]);
    }

    #[test]
    fn parse_with_explanation_around() {
        let r =
            parse_word_list("好的,以下是筛选结果:\n[\"foo\", \"bar\", \"克劳德\"]\n希望对你有帮助");
        assert_eq!(r, vec!["foo", "bar", "克劳德"]);
    }

    #[test]
    fn parse_fallback_line_format() {
        let r = parse_word_list("- foo\n- bar\nbaz\n- 克劳德");
        assert_eq!(r, vec!["foo", "bar", "baz", "克劳德"]);
    }

    #[test]
    fn build_prompt_includes_raw_text_and_existing() {
        let p = build_prompt(
            "今天我们聊了 voice-claude 和 克劳德,还提到了豆包 ASR。",
            &["Claude".into(), "OpenAI".into()],
        );
        assert!(p.contains("voice-claude"));
        assert!(p.contains("克劳德"));
        assert!(p.contains("豆包"));
        assert!(p.contains("已在词典里的词"));
        assert!(p.contains("JSON array"));
    }

    #[test]
    fn build_prompt_skips_existing_section_when_empty() {
        let p = build_prompt("hello world", &[]);
        assert!(!p.contains("已在词典里的词"));
    }

    #[test]
    fn build_prompt_dedupes_existing_case_insensitive() {
        // 词典里有同一个词的大小写变体时,prompt 里只列一次(去重 by lowercase)。
        // 同时文案明确"不区分大小写"避免 LLM 把变体当新词。
        let p = build_prompt(
            "hello",
            &[
                "Claude".into(),
                "claude".into(),
                "CLAUDE".into(),
                "voice-claude".into(),
                "Voice-Claude".into(),
            ],
        );
        assert!(p.contains("不区分大小写"));
        // 首次出现的形式保留 —— "Claude" / "voice-claude"
        assert_eq!(p.matches("- Claude\n").count(), 1);
        assert_eq!(p.matches("- claude\n").count(), 0);
        assert_eq!(p.matches("- CLAUDE\n").count(), 0);
        assert_eq!(p.matches("- voice-claude\n").count(), 1);
        assert_eq!(p.matches("- Voice-Claude\n").count(), 0);
    }

    #[test]
    fn take_tail_excerpt_handles_short_text() {
        let r = take_tail_excerpt("hello", 100);
        assert_eq!(r, "hello");
    }

    #[test]
    fn take_tail_excerpt_takes_last_n_chars_utf8_safe() {
        let text = "前缀abcdef中文末尾";
        let r = take_tail_excerpt(text, 4);
        assert_eq!(r, "中文末尾");
    }
}
