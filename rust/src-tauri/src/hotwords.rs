//! 热词替换：ASR 识别后自动修正专有名词。
//! 对应 Go 版的 hotwords.go。

use std::collections::HashMap;

/// 按热词表做字符串替换。
///
/// 替换顺序按 key 长度降序，避免短词匹配破坏长词（如 "API" 和 "APIKey"）。
/// 空 key 会被忽略。
pub fn apply(text: &str, hotwords: &HashMap<String, String>) -> String {
    if hotwords.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let mut keys: Vec<&String> = hotwords.keys().filter(|k| !k.is_empty()).collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut result = text.to_string();
    for k in keys {
        if let Some(v) = hotwords.get(k) {
            result = result.replace(k.as_str(), v.as_str());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn empty_hotwords_returns_original() {
        let m = HashMap::new();
        assert_eq!(apply("今天天气不错", &m), "今天天气不错");
    }

    #[test]
    fn empty_text_returns_empty() {
        let m = map(&[("a", "b")]);
        assert_eq!(apply("", &m), "");
    }

    #[test]
    fn single_replacement() {
        let m = map(&[("克劳德", "Claude")]);
        assert_eq!(apply("使用克劳德写代码", &m), "使用Claude写代码");
    }

    #[test]
    fn multiple_replacements() {
        let m = map(&[("克劳德", "Claude"), ("吉他布", "GitHub"), ("艾皮爱", "API")]);
        assert_eq!(apply("在吉他布上用克劳德调艾皮爱", &m), "在GitHub上用Claude调API");
    }

    #[test]
    fn longer_key_wins() {
        let m = map(&[("艾皮爱", "API"), ("艾皮爱key", "APIKey")]);
        assert_eq!(apply("使用艾皮爱key访问", &m), "使用APIKey访问");
    }

    #[test]
    fn ignores_empty_key() {
        let m = map(&[("", "X"), ("天气", "weather")]);
        assert_eq!(apply("今天天气不错", &m), "今天weather不错");
    }
}
