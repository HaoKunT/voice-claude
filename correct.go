package main

import (
	"bytes"
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"
)

// ollamaCheckClient 复用连接池，专用于 CheckOllama 健康检测。
var ollamaCheckClient = &http.Client{Timeout: 2 * time.Second}

// CorrectText 对 ASR 识别文本进行 AI 纠错，模式由 cfg.CorrectMode 控制。
// 纠错模式为 off 时直接返回原文。
func CorrectText(ctx context.Context, text string, cfg *Config) (string, error) {
	switch cfg.CorrectMode {
	case CorrectModeOff, "":
		return text, nil
	case CorrectModeOllama:
		return correctOllama(ctx, text, cfg)
	case CorrectModeOpenRouter:
		return correctOpenRouter(ctx, text, cfg)
	case CorrectModeCloud:
		return correctCloud(ctx, text, cfg)
	default:
		return text, nil
	}
}

func correctOpenRouter(ctx context.Context, text string, cfg *Config) (string, error) {
	apiKey := cfg.OpenRouterAPIKey
	if apiKey == "" {
		return text, errors.New("请配置 OpenRouter API Key")
	}

	model := cmp.Or(cfg.CorrectModel, "qwen/qwen3-8b")

	reqBody := openaiRequest{
		Model: model,
		Messages: []openaiMessage{
			{Role: "system", Content: correctionPrompt},
			{Role: "user", Content: text},
		},
	}

	body, err := json.Marshal(reqBody)
	if err != nil {
		return text, fmt.Errorf("marshal request: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, "https://openrouter.ai/api/v1/chat/completions", bytes.NewReader(body))
	if err != nil {
		return text, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+apiKey)

	client := &http.Client{Timeout: correctTimeout(cfg)}
	resp, err := client.Do(req)
	if err != nil {
		return text, err
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	respBody := readLimitedBody(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return text, fmt.Errorf("openrouter API %d: %s", resp.StatusCode, string(respBody))
	}

	var result openaiResponse
	if err := json.Unmarshal(respBody, &result); err != nil {
		return text, err
	}
	if len(result.Choices) > 0 {
		return strings.TrimSpace(result.Choices[0].Message.Content), nil
	}
	return text, nil
}

const correctionPrompt = `你是一个语音识别纠错助手。用户通过语音输入文字，可能有同音字错误、漏字、多字等问题。
请只纠正明显的语音识别错误，不要改变用户的意思，不要添加或删除内容。
如果原文没有明显错误，直接返回原文。
只输出纠正后的文本，不要解释。`

// CheckOllama 检测 ollama 是否在运行
func CheckOllama(url string) error {
	if url == "" {
		url = "http://localhost:11434"
	}
	resp, err := ollamaCheckClient.Get(url + "/api/tags")
	if err != nil {
		return errors.New("ollama 未运行，请先启动: ollama serve")
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("ollama 异常: HTTP %d", resp.StatusCode)
	}
	return nil
}

func correctOllama(ctx context.Context, text string, cfg *Config) (string, error) {
	reqBody := map[string]any{
		"model":  cfg.CorrectModel,
		"prompt": fmt.Sprintf("%s\n\n原文：%s", correctionPrompt, text),
		"stream": false,
	}
	return postJSON(ctx, cfg.CorrectURL, reqBody, correctTimeout(cfg), func(resp map[string]any) string {
		if r, ok := resp["response"].(string); ok {
			return strings.TrimSpace(r)
		}
		return text
	})
}

func correctTimeout(cfg *Config) time.Duration {
	if cfg.CorrectTimeout > 0 {
		return time.Duration(cfg.CorrectTimeout) * time.Second
	}
	return 10 * time.Second
}

type openaiMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type openaiRequest struct {
	Model    string          `json:"model"`
	Messages []openaiMessage `json:"messages"`
}

type openaiResponse struct {
	Choices []struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	} `json:"choices"`
}

func correctCloud(ctx context.Context, text string, cfg *Config) (string, error) {
	reqBody := openaiRequest{
		Model: cfg.CorrectModel,
		Messages: []openaiMessage{
			{Role: "system", Content: correctionPrompt},
			{Role: "user", Content: text},
		},
	}

	url := cfg.CorrectURL
	if !strings.HasSuffix(url, "/chat/completions") {
		url = strings.TrimRight(url, "/") + "/v1/chat/completions"
	}

	body, err := json.Marshal(reqBody)
	if err != nil {
		return text, fmt.Errorf("marshal request: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return text, err
	}
	req.Header.Set("Content-Type", "application/json")
	if cfg.CorrectAPIKey != "" {
		req.Header.Set("Authorization", "Bearer "+cfg.CorrectAPIKey)
	}

	client := &http.Client{Timeout: correctTimeout(cfg)}
	resp, err := client.Do(req)
	if err != nil {
		return text, err
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	respBody := readLimitedBody(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return text, fmt.Errorf("cloud API %d: %s", resp.StatusCode, string(respBody))
	}

	var result openaiResponse
	if err := json.Unmarshal(respBody, &result); err != nil {
		return text, err
	}
	if len(result.Choices) > 0 {
		return strings.TrimSpace(result.Choices[0].Message.Content), nil
	}
	return text, nil
}

func postJSON(ctx context.Context, url string, reqBody any, timeout time.Duration, extract func(map[string]any) string) (string, error) {
	body, err := json.Marshal(reqBody)
	if err != nil {
		return "", fmt.Errorf("marshal request: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")

	client := &http.Client{Timeout: timeout}
	resp, err := client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	respBody := readLimitedBody(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("API %d: %s", resp.StatusCode, string(respBody))
	}

	var result map[string]any
	if err := json.Unmarshal(respBody, &result); err != nil {
		return "", err
	}
	return extract(result), nil
}
