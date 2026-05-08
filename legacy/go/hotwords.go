package main

import "strings"

// ApplyHotwords 按热词表做字符串替换。
// 替换按 key 长度从长到短排序，避免短词匹配破坏长词（如 "API" 和 "APIKey"）。
func ApplyHotwords(text string, hotwords map[string]string) string {
	if len(hotwords) == 0 || text == "" {
		return text
	}

	// 收集 key 并按长度降序排序
	keys := make([]string, 0, len(hotwords))
	for k := range hotwords {
		if k == "" {
			continue
		}
		keys = append(keys, k)
	}
	// 插入排序（key 数量不多，常量时间）
	for i := 1; i < len(keys); i++ {
		for j := i; j > 0 && len(keys[j]) > len(keys[j-1]); j-- {
			keys[j], keys[j-1] = keys[j-1], keys[j]
		}
	}

	for _, k := range keys {
		text = strings.ReplaceAll(text, k, hotwords[k])
	}
	return text
}
