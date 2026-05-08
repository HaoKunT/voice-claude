package main

import (
	"os"
	"path/filepath"
	"runtime"
)

// configDir 返回平台惯例的配置目录。
//
//	macOS:   ~/Library/Application Support/voice-claude
//	Windows: %APPDATA%\voice-claude
//	Linux:   ~/.voice-claude
func configDir() string {
	home, _ := os.UserHomeDir()
	switch runtime.GOOS {
	case "darwin":
		return filepath.Join(home, "Library", "Application Support", "voice-claude")
	case "windows":
		if appdata := os.Getenv("APPDATA"); appdata != "" {
			return filepath.Join(appdata, "voice-claude")
		}
		return filepath.Join(home, "AppData", "Roaming", "voice-claude")
	default:
		return filepath.Join(home, ".voice-claude")
	}
}

// appLogDir 返回平台惯例的日志目录。
//
//	macOS:   ~/Library/Logs/voice-claude
//	Windows: %LOCALAPPDATA%\voice-claude\logs
//	Linux:   ~/.voice-claude
func appLogDir() string {
	home, _ := os.UserHomeDir()
	switch runtime.GOOS {
	case "darwin":
		return filepath.Join(home, "Library", "Logs", "voice-claude")
	case "windows":
		if localappdata := os.Getenv("LOCALAPPDATA"); localappdata != "" {
			return filepath.Join(localappdata, "voice-claude", "logs")
		}
		return filepath.Join(home, "AppData", "Local", "voice-claude", "logs")
	default:
		return filepath.Join(home, ".voice-claude")
	}
}
