package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"mime/multipart"
	"net/http"
	"strings"
	"time"
)

const (
	zhipuAPIURL     = "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions"
	zhipuMaxSeconds = 30
	zhipuSampleRate = 16000
)

// asrHTTPClient 复用连接池，避免每次请求重建 TCP 连接。
var asrHTTPClient = &http.Client{Timeout: 60 * time.Second}

// TranscribeZhipu 使用智谱 GLM-ASR 识别 WAV 音频，超过 30 秒自动分段识别。
func TranscribeZhipu(ctx context.Context, wavBytes []byte, apiKey string) (string, error) {
	chunks, err := splitWAV(wavBytes, zhipuMaxSeconds)
	if err != nil {
		return "", fmt.Errorf("split wav: %w", err)
	}

	results := make([]string, len(chunks))
	for i, chunk := range chunks {
		if len(chunks) > 1 {
			slog.Info("转写分段", "current", i+1, "total", len(chunks))
		}
		text, err := zhipuTranscribeChunk(ctx, chunk, apiKey)
		if err != nil {
			return "", fmt.Errorf("transcribe chunk %d: %w", i+1, err)
		}
		results[i] = text
	}

	var b strings.Builder
	for _, t := range results {
		b.WriteString(t)
	}
	return b.String(), nil
}

func zhipuTranscribeChunk(ctx context.Context, wavBytes []byte, apiKey string) (string, error) {
	var body bytes.Buffer
	writer := multipart.NewWriter(&body)

	part, err := writer.CreateFormFile("file", "audio.wav")
	if err != nil {
		return "", err
	}
	if _, err := part.Write(wavBytes); err != nil {
		return "", err
	}

	if err := writer.WriteField("model", "glm-asr-2512"); err != nil {
		return "", err
	}
	if err := writer.Close(); err != nil {
		return "", err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, zhipuAPIURL, &body)
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", writer.FormDataContentType())
	req.Header.Set("Authorization", "Bearer "+apiKey)

	resp, err := asrHTTPClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close, error not actionable

	respBody := readLimitedBody(resp.Body)
	slog.Debug("zhipu API 响应", "status", resp.StatusCode)
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("zhipu API %d: %s", resp.StatusCode, string(respBody))
	}

	var result struct {
		Text string `json:"text"`
	}
	if err := json.Unmarshal(respBody, &result); err != nil {
		return "", fmt.Errorf("parse response: %w", err)
	}
	slog.Debug("zhipu 识别文本", "text", result.Text)
	return result.Text, nil
}

func splitWAV(wavBytes []byte, maxSeconds int) ([][]byte, error) {
	// WAV header: 44 bytes, then PCM data
	if len(wavBytes) < 44 {
		return [][]byte{wavBytes}, nil
	}

	// Parse sample rate from header (offset 24, uint32 LE)
	sampleRate := uint32(wavBytes[24]) | uint32(wavBytes[25])<<8 | uint32(wavBytes[26])<<16 | uint32(wavBytes[27])<<24
	channels := uint16(wavBytes[22]) | uint16(wavBytes[23])<<8
	bitsPerSample := uint16(wavBytes[34]) | uint16(wavBytes[35])<<8

	pcmData := wavBytes[44:]
	bytesPerSample := int(channels) * int(bitsPerSample/8)
	if bytesPerSample == 0 || sampleRate == 0 {
		return [][]byte{wavBytes}, nil
	}
	totalSamples := len(pcmData) / bytesPerSample
	duration := float64(totalSamples) / float64(sampleRate)

	if duration <= float64(maxSeconds) {
		return [][]byte{wavBytes}, nil
	}

	slog.Info("音频分段", "duration_seconds", duration, "limit_seconds", maxSeconds)

	chunkSamples := maxSeconds * int(sampleRate)
	chunkBytes := chunkSamples * bytesPerSample
	// Align to sample boundary
	chunkBytes -= chunkBytes % bytesPerSample

	chunks := make([][]byte, 0, len(pcmData)/chunkBytes+1)
	for offset := 0; offset < len(pcmData); offset += chunkBytes {
		end := min(offset+chunkBytes, len(pcmData))
		chunkPCM := pcmData[offset:end]

		// Build WAV for this chunk
		chunkWAV := buildWAV(chunkPCM, sampleRate, channels, bitsPerSample)
		chunks = append(chunks, chunkWAV)
	}

	return chunks, nil
}

func buildWAV(pcm []byte, sampleRate uint32, channels, bitsPerSample uint16) []byte {
	var buf bytes.Buffer
	dataSize := uint32(len(pcm))
	fileSize := 36 + dataSize

	buf.WriteString("RIFF")
	writeUint32LE(&buf, fileSize)
	buf.WriteString("WAVE")

	buf.WriteString("fmt ")
	writeUint32LE(&buf, 16)
	writeUint16LE(&buf, 1) // PCM
	writeUint16LE(&buf, channels)
	writeUint32LE(&buf, sampleRate)
	writeUint32LE(&buf, sampleRate*uint32(channels)*uint32(bitsPerSample/8))
	writeUint16LE(&buf, channels*bitsPerSample/8)
	writeUint16LE(&buf, bitsPerSample)

	buf.WriteString("data")
	writeUint32LE(&buf, dataSize)
	buf.Write(pcm)

	return buf.Bytes()
}

func writeUint32LE(buf *bytes.Buffer, v uint32) {
	b := [4]byte{byte(v), byte(v >> 8), byte(v >> 16), byte(v >> 24)}
	buf.Write(b[:])
}

func writeUint16LE(buf *bytes.Buffer, v uint16) {
	b := [2]byte{byte(v), byte(v >> 8)}
	buf.Write(b[:])
}

// readLimitedBody 读取 HTTP 响应体，最多读取 10MB 防止内存耗尽。
func readLimitedBody(r io.Reader) []byte {
	b, _ := io.ReadAll(io.LimitReader(r, 10*1024*1024))
	return b
}
