package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.design/x/hotkey"
)

func TestParseHotkey(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		input   string
		wantKey hotkey.Key
		wantErr bool
	}{
		{
			name:    "cmd+shift+f5",
			input:   "cmd+shift+f5",
			wantKey: hotkey.KeyF5,
		},
		{
			name:    "大写不敏感",
			input:   "CMD+SHIFT+F5",
			wantKey: hotkey.KeyF5,
		},
		{
			name:    "空格前后修剪",
			input:   " cmd + shift + space ",
			wantKey: hotkey.KeySpace,
		},
		{
			name:    "command 别名",
			input:   "command+a",
			wantKey: hotkey.KeyA,
		},
		{
			name:    "option 别名",
			input:   "option+b",
			wantKey: hotkey.KeyB,
		},
		{
			name:    "数字键",
			input:   "ctrl+1",
			wantKey: hotkey.Key1,
		},
		{
			name:    "缺少主键",
			input:   "cmd+shift",
			wantErr: true,
		},
		{
			name:    "格式错误：无加号",
			input:   "cmdshift",
			wantErr: true,
		},
		{
			name:    "未知按键",
			input:   "cmd+unknownkey",
			wantErr: true,
		},
		{
			name:    "空字符串",
			input:   "",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got, err := ParseHotkey(tt.input)
			if tt.wantErr {
				require.Error(t, err)
				return
			}
			require.NoError(t, err)
			assert.Equal(t, tt.wantKey, got.Key)
			assert.NotEmpty(t, got.Mods)
		})
	}
}
