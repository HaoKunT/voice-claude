package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestApplyHotwords(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name     string
		text     string
		hotwords map[string]string
		want     string
	}{
		{
			name:     "空表直接返回",
			text:     "今天天气不错",
			hotwords: nil,
			want:     "今天天气不错",
		},
		{
			name:     "空文本直接返回",
			text:     "",
			hotwords: map[string]string{"a": "b"},
			want:     "",
		},
		{
			name:     "单词替换",
			text:     "使用克劳德写代码",
			hotwords: map[string]string{"克劳德": "Claude"},
			want:     "使用Claude写代码",
		},
		{
			name:     "多词替换",
			text:     "在吉他布上用克劳德调艾皮爱",
			hotwords: map[string]string{"克劳德": "Claude", "吉他布": "GitHub", "艾皮爱": "API"},
			want:     "在GitHub上用Claude调API",
		},
		{
			name:     "长词优先避免部分匹配",
			text:     "使用艾皮爱key访问",
			hotwords: map[string]string{"艾皮爱": "API", "艾皮爱key": "APIKey"},
			want:     "使用APIKey访问",
		},
		{
			name:     "忽略空 key",
			text:     "今天天气不错",
			hotwords: map[string]string{"": "X", "天气": "weather"},
			want:     "今天weather不错",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := ApplyHotwords(tt.text, tt.hotwords)
			assert.Equal(t, tt.want, got)
		})
	}
}
