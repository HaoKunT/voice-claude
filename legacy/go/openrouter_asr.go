package main

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
)

const openrouterASRURL = "https://openrouter.ai/api/v1/audio/transcriptions"

type openrouterASRRequest struct {
	Model      string `json:"model"`
	InputAudio struct {
		Data   string `json:"data"`
		Format string `json:"format"`
	} `json:"input_audio"`
}

// TranscribeOpenRouter 通过 OpenRouter Whisper 模型识别音频
func TranscribeOpenRouter(ctx context.Context, wavBytes []byte, cfg *Config) (string, error) {
	apiKey := cfg.OpenRouterAPIKey
	if apiKey == "" {
		return "", errors.New("请配置 OpenRouter API Key")
	}

	reqBody := openrouterASRRequest{
		Model: "openai/whisper-large-v3-turbo",
	}
	reqBody.InputAudio.Data = base64.StdEncoding.EncodeToString(wavBytes)
	reqBody.InputAudio.Format = "wav"

	body, _ := json.Marshal(reqBody)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, openrouterASRURL, bytes.NewReader(body))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+apiKey)

	resp, err := asrHTTPClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	respBody := readLimitedBody(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("openrouter ASR %d: %s", resp.StatusCode, string(respBody))
	}

	var result struct {
		Text string `json:"text"`
	}
	if err := json.Unmarshal(respBody, &result); err != nil {
		return "", fmt.Errorf("parse response: %w", err)
	}
	return result.Text, nil
}
