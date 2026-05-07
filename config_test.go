package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadConfig_Defaults(t *testing.T) {
	// 使用不存在的配置目录，让 LoadConfig 返回默认值
	t.Setenv("HOME", t.TempDir())

	cfg := LoadConfig()
	assert.Equal(t, "zhipu", cfg.ASRProvider)
	assert.Equal(t, "off", cfg.CorrectMode)
	assert.Equal(t, "cmd+shift+f5", cfg.Hotkey)
	assert.Equal(t, 1, cfg.Gain)
	assert.Equal(t, 10, cfg.CorrectTimeout)
	assert.Equal(t, "info", cfg.LogLevel)
}

func TestConfigSaveAndLoad(t *testing.T) {
	// 将配置目录重定向到临时目录
	tmpDir := t.TempDir()
	t.Setenv("HOME", tmpDir)

	cfg := &Config{
		ASRProvider:  "volc",
		Hotkey:       "cmd+shift+space",
		Gain:         3,
		LogLevel:     "debug",
		CorrectMode:  CorrectModeOff,
		CorrectTimeout: 20,
	}

	require.NoError(t, cfg.Save())

	// 读回并验证
	loaded := LoadConfig()
	assert.Equal(t, "volc", loaded.ASRProvider)
	assert.Equal(t, "cmd+shift+space", loaded.Hotkey)
	assert.Equal(t, 3, loaded.Gain)
	assert.Equal(t, "debug", loaded.LogLevel)
	assert.Equal(t, 20, loaded.CorrectTimeout)
}

func TestConfigSave_FilePermissions(t *testing.T) {
	tmpDir := t.TempDir()
	t.Setenv("HOME", tmpDir)

	cfg := &Config{Hotkey: "cmd+shift+f5"}
	require.NoError(t, cfg.Save())

	path := configPath()
	info, err := os.Stat(path)
	require.NoError(t, err)
	// 配置文件含 API Key，权限应为 0o600
	assert.Equal(t, os.FileMode(0o600), info.Mode().Perm())
}

func TestLoadConfig_InvalidJSON(t *testing.T) {
	tmpDir := t.TempDir()
	t.Setenv("HOME", tmpDir)

	// 写入非法 JSON
	cfgPath := filepath.Join(tmpDir, "Library", "Application Support", "voice-claude")
	require.NoError(t, os.MkdirAll(cfgPath, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(cfgPath, "config.json"), []byte("not json"), 0o600))

	// 解析失败应返回默认值
	cfg := LoadConfig()
	assert.Equal(t, "zhipu", cfg.ASRProvider)
}

func TestConfigJSON_RoundTrip(t *testing.T) {
	original := &Config{
		ASRProvider:      ASRProviderVolc,
		VolcAppKey:       "my-app-key",
		VolcAccessToken:  "my-token",
		VolcResourceID:   "volc.seedasr.sauc.duration",
		CorrectMode:      CorrectModeCloud,
		CorrectModel:     "gpt-4o",
		CorrectAPIKey:    "sk-xxx",
		CorrectTimeout:   15,
		Hotkey:           "option+shift+r",
		Gain:             2,
		LogLevel:         "debug",
	}

	data, err := json.Marshal(original)
	require.NoError(t, err)

	var restored Config
	require.NoError(t, json.Unmarshal(data, &restored))

	assert.Equal(t, *original, restored)
}
