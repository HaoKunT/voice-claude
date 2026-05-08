package main

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestCorrectTimeout(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		timeout int
		want    time.Duration
	}{
		{"零值返回默认10秒", 0, 10 * time.Second},
		{"自定义30秒", 30, 30 * time.Second},
		{"负值返回默认10秒", -5, 10 * time.Second},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			cfg := &Config{CorrectTimeout: tt.timeout}
			assert.Equal(t, tt.want, correctTimeout(cfg))
		})
	}
}

func TestCorrectText_OffModes(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		mode string
	}{
		{"off 模式直接返回", CorrectModeOff},
		{"空字符串直接返回", ""},
		{"未知模式直接返回", "unknown"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			cfg := &Config{CorrectMode: tt.mode}
			got, err := CorrectText(context.Background(), "text", cfg)
			assert.NoError(t, err)
			assert.Equal(t, "text", got)
		})
	}
}
