package main

import (
	"bytes"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestBuildWAV(t *testing.T) {
	t.Parallel()
	pcm := make([]byte, 3200) // 0.1s @ 16kHz 16bit mono
	wav := buildWAV(pcm, 16000, 1, 16)

	assert.Equal(t, []byte("RIFF"), wav[0:4])
	assert.Equal(t, []byte("WAVE"), wav[8:12])
	assert.Equal(t, []byte("fmt "), wav[12:16])
	assert.Equal(t, []byte("data"), wav[36:40])
	assert.Len(t, wav, 44+len(pcm))
}

func TestSplitWAV(t *testing.T) {
	t.Parallel()
	secondBytes := 16000 * 2 // 16kHz 16bit mono = 32000 bytes/s

	tests := []struct {
		name       string
		pcm        []byte
		maxSeconds int
		wantChunks int
		wantRaw    bool // true = 期望返回原始字节（不重新打包）
	}{
		{
			name:       "短音频不分段",
			pcm:        make([]byte, 2*secondBytes),
			maxSeconds: 30,
			wantChunks: 1,
		},
		{
			name:       "75秒分成3段",
			pcm:        make([]byte, 75*secondBytes),
			maxSeconds: 30,
			wantChunks: 3,
		},
		{
			name:       "数据不足44字节直接返回",
			pcm:        nil,
			maxSeconds: 30,
			wantChunks: 1,
			wantRaw:    true,
		},
		{
			name:       "bitsPerSample为零安全返回",
			pcm:        nil, // 由 makeCorruptWAV 控制
			maxSeconds: 30,
			wantChunks: 1,
			wantRaw:    true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			var wav []byte
			if tt.name == "数据不足44字节直接返回" {
				wav = []byte("too short")
			} else if tt.name == "bitsPerSample为零安全返回" {
				wav = buildWAV(make([]byte, 100), 16000, 1, 16)
				wav[34] = 0
				wav[35] = 0
			} else {
				wav = buildWAV(tt.pcm, 16000, 1, 16)
			}

			chunks, err := splitWAV(wav, tt.maxSeconds)
			require.NoError(t, err)
			require.Len(t, chunks, tt.wantChunks)

			// 多段时每段都应是合法 WAV
			if tt.wantChunks > 1 {
				for _, chunk := range chunks {
					assert.Equal(t, []byte("RIFF"), chunk[0:4])
					assert.Equal(t, []byte("WAVE"), chunk[8:12])
				}
			}
		})
	}
}

func TestReadLimitedBody(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		input   []byte
		wantLen int
	}{
		{"正常数据完整读取", []byte("hello world"), len("hello world")},
		{"超过10MB截断", bytes.Repeat([]byte("x"), 11*1024*1024), 10 * 1024 * 1024},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := readLimitedBody(strings.NewReader(string(tt.input)))
			assert.Len(t, got, tt.wantLen)
		})
	}
}
