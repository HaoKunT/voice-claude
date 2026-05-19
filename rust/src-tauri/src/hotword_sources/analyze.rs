//! 候选词分析:从大块文本中抽取出现频率高的英文 / 混合 token,过滤停用词
//! 与已存在的 hotwords,得到候选列表给 LLM 二次筛选。
//!
//! 仅提取英文 / 数字 / 连字符 token。中文术语对正则切分友好度低(N-gram 会
//! 产生大量子串噪声),v1 不在本地候选阶段处理 —— 中文术语由 LLM 二次筛选
//! 阶段从原文上下文中挑出来(原文也一并传给 LLM)。

use std::collections::HashSet;

/// 单个候选词及其频率。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub word: String,
    pub freq: u32,
}

/// 从文本提取候选词。
/// - `existing`:用户已配的 hotwords(case-insensitive 比较去重)
/// - `min_freq`:出现次数门槛(< 该值过滤)
/// - `max_count`:返回 top N(按 freq 降序)
pub fn candidates_from_text(
    text: &str,
    existing: &[String],
    min_freq: u32,
    max_count: usize,
) -> Vec<Candidate> {
    use std::collections::HashMap;
    let existing_lc: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    let stop = stopwords();

    let mut counts: HashMap<String, u32> = HashMap::new();
    for token in tokenize(text) {
        if token.len() < 3 {
            continue;
        }
        let lc = token.to_lowercase();
        if stop.contains(lc.as_str()) {
            continue;
        }
        if existing_lc.contains(&lc) {
            continue;
        }
        // 用原 case 做 key —— 同一个词不同大小写按原文出现频率分别记,
        // 避免 "Claude" 跟 "claude" 合并成小写丢失原书写
        *counts.entry(token.to_string()).or_insert(0) += 1;
    }

    let mut v: Vec<Candidate> = counts
        .into_iter()
        .filter(|(_, c)| *c >= min_freq)
        .map(|(word, freq)| Candidate { word, freq })
        .collect();
    v.sort_by(|a, b| b.freq.cmp(&a.freq).then_with(|| a.word.cmp(&b.word)));
    v.truncate(max_count);
    v
}

/// 简单 token 切分:`[A-Za-z][A-Za-z0-9_-]*[A-Za-z0-9]?` 风格的连续段。
/// 不引入 regex crate(够用,且 voice-claude 已经 build 慢,不想再加依赖)。
fn tokenize(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .filter(|s| {
            !s.is_empty()
                && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars().last().is_some_and(|c| c.is_ascii_alphanumeric())
        })
}

/// 常见英文停用词(短小一批,够过滤掉绝大多数高频 garbage)。
/// 编程常用 keyword(function/return/class)也放进来,大多数项目里都高频
/// 但不是用户领域术语。LLM 二次筛选还会再过一遍,这里粗滤即可。
fn stopwords() -> HashSet<&'static str> {
    [
        // 英文虚词
        "the",
        "a",
        "an",
        "and",
        "or",
        "but",
        "if",
        "then",
        "else",
        "when",
        "for",
        "while",
        "do",
        "this",
        "that",
        "these",
        "those",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "having",
        "of",
        "in",
        "on",
        "at",
        "by",
        "to",
        "from",
        "with",
        "as",
        "into",
        "you",
        "your",
        "yours",
        "we",
        "our",
        "ours",
        "they",
        "them",
        "their",
        "it",
        "its",
        "he",
        "she",
        "his",
        "her",
        "hers",
        "him",
        "what",
        "which",
        "who",
        "whom",
        "where",
        "why",
        "how",
        "all",
        "any",
        "some",
        "no",
        "not",
        "only",
        "own",
        "same",
        "so",
        "than",
        "too",
        "very",
        "can",
        "will",
        "just",
        "don",
        "should",
        "now",
        "out",
        "up",
        "down",
        "off",
        "over",
        "under",
        "again",
        "more",
        "most",
        "other",
        "such",
        "may",
        "might",
        // 编程通用
        "function",
        "return",
        "class",
        "let",
        "const",
        "var",
        "fn",
        "pub",
        "use",
        "mod",
        "import",
        "from",
        "export",
        "const",
        "type",
        "struct",
        "enum",
        "impl",
        "trait",
        "self",
        "true",
        "false",
        "null",
        "none",
        "match",
        "case",
        "switch",
        "break",
        "continue",
        "throw",
        "catch",
        "try",
        "finally",
        "async",
        "await",
        "yield",
        "new",
        "delete",
        "void",
        "int",
        "string",
        "bool",
        "float",
        "double",
        "char",
        "byte",
        "long",
        "short",
        "static",
        "final",
        "private",
        "public",
        "protected",
        "abstract",
        "interface",
        "extends",
        "implements",
        "this",
        "super",
        // 常见 boilerplate
        "test",
        "tests",
        "example",
        "examples",
        "todo",
        "fixme",
        "note",
        "log",
        "logs",
        "debug",
        "info",
        "warn",
        "warning",
        "error",
        "fatal",
        "data",
        "result",
        "value",
        "name",
        "key",
        "size",
        "length",
        "count",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_existing_case_insensitive() {
        let text = "Claude code uses Claude often";
        let r = candidates_from_text(text, &["claude".into()], 1, 100);
        assert!(!r.iter().any(|c| c.word.to_lowercase() == "claude"));
        assert!(r.iter().any(|c| c.word == "code"));
    }

    #[test]
    fn freq_filter_works() {
        let text = "alpha alpha alpha beta beta gamma";
        let r = candidates_from_text(text, &[], 2, 100);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].word, "alpha");
        assert_eq!(r[0].freq, 3);
        assert_eq!(r[1].word, "beta");
    }

    #[test]
    fn stopwords_filtered() {
        let text = "the the function function the function";
        let r = candidates_from_text(text, &[], 1, 100);
        assert!(r.is_empty()); // 全是停用词
    }

    #[test]
    fn min_length_3_filter() {
        let text = "go go go is is is rust rust rust";
        let r = candidates_from_text(text, &[], 1, 100);
        // "go" 是 2 字符被过滤,"is" 是停用词被过滤,只剩 rust
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "rust");
    }

    #[test]
    fn hyphen_token_kept() {
        let text = "voice-claude voice-claude voice-claude";
        let r = candidates_from_text(text, &[], 2, 100);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "voice-claude");
    }

    #[test]
    fn case_preserved() {
        let text = "Claude Claude Claude FireRedASR FireRedASR FireRedASR";
        let r = candidates_from_text(text, &[], 2, 100);
        let words: Vec<&str> = r.iter().map(|c| c.word.as_str()).collect();
        assert!(words.contains(&"Claude"));
        assert!(words.contains(&"FireRedASR"));
    }

    #[test]
    fn sort_by_freq_desc_then_alpha() {
        let text = "alpha alpha beta beta gamma gamma gamma";
        let r = candidates_from_text(text, &[], 1, 100);
        assert_eq!(r[0].word, "gamma"); // 最高 freq
                                        // alpha 跟 beta 同 freq,字典序 alpha 在前
        assert_eq!(r[1].word, "alpha");
        assert_eq!(r[2].word, "beta");
    }
}
