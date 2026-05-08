package main

import (
	"encoding/json"
	"os"
	"path/filepath"
)

const (
	ASRProviderZhipu      = "zhipu"
	ASRProviderXfyun      = "xfyun"
	ASRProviderVolc       = "volc"
	ASRProviderOpenRouter = "openrouter"
	ASRProviderLocal      = "local"

	CorrectModeOff        = "off"
	CorrectModeOllama     = "ollama"
	CorrectModeOpenRouter = "openrouter"
	CorrectModeCloud      = "cloud"
)

type Config struct {
	ASRProvider       string `json:"asr_provider"` // zhipu / xfyun / openrouter
	ASRAPIKey         string `json:"asr_api_key"`  // 智谱 API Key
	XfyunAppID        string `json:"xfyun_app_id"` // 讯飞 App ID
	XfyunAccessKeyID  string `json:"xfyun_access_key_id"`
	XfyunAccessSecret string `json:"xfyun_access_key_secret"`
	OpenRouterAPIKey  string `json:"openrouter_api_key"` // OpenRouter API Key（ASR + 纠错共用）
	VolcAppKey        string `json:"volc_app_key"`       // 豆包/火山 App Key
	VolcAccessToken   string `json:"volc_access_token"`  // 豆包/火山 Access Token
	VolcResourceID    string `json:"volc_resource_id"`   // 豆包识别模型
	CorrectMode       string `json:"correct_mode"`       // off / ollama / openrouter / cloud
	CorrectURL        string `json:"correct_url"`
	CorrectModel      string `json:"correct_model"`
	CorrectAPIKey     string `json:"correct_api_key"`
	Hotkey            string `json:"hotkey"`
	Gain              int    `json:"gain"`
	DeviceName        string `json:"device_name"`
	CorrectTimeout    int    `json:"correct_timeout"`
	LogLevel          string `json:"log_level"`
}

func configPath() string {
	return filepath.Join(configDir(), "config.json")
}

// LoadConfig 从配置文件加载设置，文件不存在或解析失败时返回默认配置。
func LoadConfig() *Config {
	cfg := &Config{
		ASRProvider:    "zhipu",
		CorrectMode:    "off",
		CorrectURL:     "http://localhost:11434/api/generate",
		CorrectModel:   "qwen2.5:3b",
		Hotkey:         "cmd+shift+f5",
		Gain:           1,
		CorrectTimeout: 10,
		LogLevel:       "info",
		VolcResourceID: "volc.seedasr.sauc.duration",
	}

	data, err := os.ReadFile(configPath())
	if err != nil {
		return cfg
	}
	if err := json.Unmarshal(data, cfg); err != nil {
		return cfg
	}
	return cfg
}

func (c *Config) Save() error {
	path := configPath()
	os.MkdirAll(filepath.Dir(path), 0o755) //nolint:errcheck,gosec // best-effort directory creation
	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o600)
}
