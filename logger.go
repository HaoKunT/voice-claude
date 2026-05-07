package main

import (
	"io"
	"log/slog"
	"os"
	"path/filepath"
)

var logLevel = new(slog.LevelVar)

func init() {
	dir := appLogDir()
	os.MkdirAll(dir, 0o755) //nolint:errcheck,gosec // best-effort directory creation

	logFile, err := os.OpenFile(
		filepath.Join(dir, "voice-claude.log"),
		os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644,
	)

	var out io.Writer
	if err != nil {
		out = os.Stderr
	} else {
		out = io.MultiWriter(os.Stderr, logFile)
	}

	handler := slog.NewTextHandler(out, &slog.HandlerOptions{Level: logLevel})
	slog.SetDefault(slog.New(handler))
}

// SetLogLevel 动态修改日志级别
func SetLogLevel(level string) {
	switch level {
	case "debug":
		logLevel.Set(slog.LevelDebug)
	case "info":
		logLevel.Set(slog.LevelInfo)
	case "warn":
		logLevel.Set(slog.LevelWarn)
	case "error":
		logLevel.Set(slog.LevelError)
	default:
		logLevel.Set(slog.LevelInfo)
	}
}
