package main

import (
	"context"
	_ "embed"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"golang.design/x/hotkey"
)

//go:embed icon.png
var iconData []byte

func main() {
	cfg := LoadConfig()
	SetLogLevel(cfg.LogLevel)

	if err := InitHistory(); err != nil {
		slog.Warn("历史记录初始化失败", "error", err)
	}

	fyneApp := app.New()
	fyneApp.Settings().SetTheme(&voiceTheme{})
	fyneApp.SetIcon(fyne.NewStaticResource("icon.png", iconData))

	SetupTray(fyneApp, func() {
		ShowSettings(fyneApp, cfg)
	}, func() {
		ShowHistory(fyneApp)
	}, func() {
		fyneApp.Quit()
	})

	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("快捷键协程 panic", "recover", r)
			}
		}()

		parsed, err := ParseHotkey(cfg.Hotkey)
		if err != nil {
			slog.Error("快捷键解析失败", "error", err)
			return
		}

		hk := hotkey.New(parsed.Mods, parsed.Key)
		if err := hk.Register(); err != nil {
			slog.Error("快捷键注册失败", "error", err, "hint", "请在「系统设置 > 隐私与安全性 > 辅助功能」中授权")
			notif := fyne.NewNotification("voice-claude", fmt.Sprintf("快捷键失败: %v\n请在辅助功能中授权", err))
			fyneApp.SendNotification(notif)
			return
		}

		currentHotkeyMu.Lock()
		currentHotkey = hk
		currentHotkeyMu.Unlock()

		slog.Info("快捷键已注册", "hotkey", cfg.Hotkey)
		for range hk.Keydown() {
			handleRecord(hk, cfg)
		}
	}()

	slog.Info("voice-claude 已启动")
	fyneApp.Run()
}

func handleRecord(hk *hotkey.Hotkey, cfg *Config) {
	slog.Info("开始录音")

	rec, err := NewRecorder(cfg.Gain, cfg.DeviceName)
	if err != nil {
		slog.Error("录音初始化失败", "error", err)
		<-hk.Keyup()
		return
	}

	var rawText string

	switch cfg.ASRProvider {
	case ASRProviderXfyun:
		rawText, err = handleRecordStream(hk, rec, cfg, TranscribeXfyunStream)
	case ASRProviderVolc:
		rawText, err = handleRecordStream(hk, rec, cfg, TranscribeVolc)
	default:
		rawText, err = handleRecordBatch(hk, rec, cfg)
	}

	rec.Close()

	if err != nil {
		slog.Error("识别失败", "error", err)
		return
	}
	if rawText == "" {
		slog.Warn("未识别到内容")
		return
	}

	slog.Info("识别结果", "text", rawText)

	corrected, err := CorrectText(context.Background(), rawText, cfg)
	if err != nil {
		slog.Warn("纠错失败，使用原文", "error", err)
		corrected = rawText
	}
	if corrected != rawText {
		slog.Info("纠错结果", "text", corrected)
	}

	SaveHistory(rawText, corrected, cfg.ASRProvider)

	slog.Info("输入文字", "text", corrected)
	TypeText(corrected)
}

type streamASRFunc func(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error)

// handleRecordStream 通用流式录音识别：等连接就绪后开始录音，松键后停止推流。
func handleRecordStream(hk *hotkey.Hotkey, rec *Recorder, cfg *Config, fn streamASRFunc) (string, error) {
	ready := make(chan struct{})
	pcmCh := rec.StartStream()

	type result struct {
		text string
		err  error
	}
	done := make(chan result, 1)
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("ASR 协程 panic", "recover", r)
				done <- result{"", fmt.Errorf("ASR panic: %v", r)}
			}
		}()
		text, err := fn(cfg, pcmCh, func(partial string) {
			slog.Debug("实时识别", "text", partial)
		}, ready)
		done <- result{text, err}
	}()

	<-ready
	if err := rec.Start(); err != nil {
		rec.StopStream()
		<-hk.Keyup()
		return "", fmt.Errorf("开始录音失败: %w", err)
	}

	<-hk.Keyup()
	rec.StopStream()
	rec.Stop()

	r := <-done
	return r.text, r.err
}

// handleRecordBatch 录完整段再识别（智谱 / OpenRouter / 本地）。
func handleRecordBatch(hk *hotkey.Hotkey, rec *Recorder, cfg *Config) (string, error) {
	if err := rec.Start(); err != nil {
		<-hk.Keyup()
		return "", fmt.Errorf("开始录音失败: %w", err)
	}

	<-hk.Keyup()
	pcm := rec.Stop()
	wav := rec.ToWAV(pcm)

	if len(wav) < 100 {
		slog.Warn("未录到声音", "wav_bytes", len(wav))
		return "", nil
	}

	if slog.Default().Enabled(context.Background(), slog.LevelDebug) {
		wavPath := filepath.Join(os.TempDir(), "voice-claude-last.wav")
		_ = os.WriteFile(wavPath, wav, 0o600)
		slog.Debug("已保存录音", "path", wavPath)
	}

	slog.Info("识别中", "wav_bytes", len(wav))
	return asrTranscribe(context.Background(), wav, cfg)
}

func asrTranscribe(ctx context.Context, wavBytes []byte, cfg *Config) (string, error) {
	switch cfg.ASRProvider {
	case ASRProviderLocal:
		return TranscribeLocal(wavBytes)
	case ASRProviderOpenRouter:
		return TranscribeOpenRouter(ctx, wavBytes, cfg)
	default:
		return TranscribeZhipu(ctx, wavBytes, cfg.ASRAPIKey)
	}
}
