package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestExtractXfyunText(t *testing.T) {
	t.Parallel()

	makeCW := func(words ...string) *xfyunResultData {
		cw := make([]struct {
			W  string `json:"w"`
			WP string `json:"wp"`
		}, len(words))
		for i, w := range words {
			cw[i].W = w
		}
		d := &xfyunResultData{}
		d.CN.ST.RT = []struct {
			WS []struct {
				CW []struct {
					W  string `json:"w"`
					WP string `json:"wp"`
				} `json:"cw"`
			} `json:"ws"`
		}{{WS: []struct {
			CW []struct {
				W  string `json:"w"`
				WP string `json:"wp"`
			} `json:"cw"`
		}{{CW: cw}}}}
		return d
	}

	tests := []struct {
		name string
		data *xfyunResultData
		want string
	}{
		{"空结果", &xfyunResultData{}, ""},
		{"单个词", makeCW("你好"), "你好"},
		{"多个词拼接", makeCW("今天", "天气", "很好"), "今天天气很好"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			assert.Equal(t, tt.want, extractXfyunText(tt.data))
		})
	}
}

func TestJoinStrings(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name  string
		input []string
		want  string
	}{
		{"空切片", []string{}, ""},
		{"单个元素", []string{"hello"}, "hello"},
		{"多个元素", []string{"你好", "世界"}, "你好世界"},
		{"含空字符串", []string{"a", "", "b"}, "ab"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			assert.Equal(t, tt.want, joinStrings(tt.input))
		})
	}
}
