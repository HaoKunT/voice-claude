package main

import (
	"context"
	_ "embed"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sync"

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
			toggleRecording(cfg)
		}
	}()

	slog.Info("voice-claude 已启动")
	fyneApp.Run()
}

func handleRecord(stopCh <-chan struct{}, cfg *Config) {
	slog.Info("开始录音")

	rec, err := NewRecorder(cfg.Gain, cfg.DeviceName)
	if err != nil {
		slog.Error("录音初始化失败", "error", err)
		return
	}

	var rawText string

	switch cfg.ASRProvider {
	case ASRProviderXfyun:
		rawText, err = handleRecordStream(stopCh, rec, cfg, TranscribeXfyunStream)
	case ASRProviderVolc:
		rawText, err = handleRecordStream(stopCh, rec, cfg, TranscribeVolc)
	default:
		rawText, err = handleRecordBatch(stopCh, rec, cfg)
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

// handleRecordStream 流式录音识别：等连接就绪后开始录音，中间结果实时输出，
// 第二次按热键（stopCh 关闭）后停止，用退格删掉中间结果再输入最终结果。
func handleRecordStream(stopCh <-chan struct{}, rec *Recorder, cfg *Config, fn streamASRFunc) (string, error) {
	ready := make(chan struct{})
	pcmCh := rec.StartStream()

	var (
		partialMu   sync.Mutex
		partialRunes int // 已输入的中间结果字符数
	)

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
			partialMu.Lock()
			prev := partialRunes
			partialRunes = len([]rune(partial))
			partialMu.Unlock()
			// 删掉上一次的中间结果，输入新的
			DeleteChars(prev)
			TypeText(partial)
		}, ready)
		done <- result{text, err}
	}()

	<-ready
	if err := rec.Start(); err != nil {
		rec.StopStream()
		return "", fmt.Errorf("开始录音失败: %w", err)
	}

	<-stopCh
	rec.StopStream()
	rec.Stop()

	r := <-done
	if r.err != nil {
		return "", r.err
	}

	// 删掉全部中间结果，由外层统一输入最终文字
	partialMu.Lock()
	prev := partialRunes
	partialRunes = 0
	partialMu.Unlock()
	DeleteChars(prev)

	return r.text, nil
}

// handleRecordBatch 录完整段再识别（智谱 / OpenRouter / 本地）。
func handleRecordBatch(stopCh <-chan struct{}, rec *Recorder, cfg *Config) (string, error) {
	if err := rec.Start(); err != nil {
		return "", fmt.Errorf("开始录音失败: %w", err)
	}

	<-stopCh
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

// toggleRecording 切换录音状态：第一次调用开始录音，第二次调用停止录音。
func toggleRecording(cfg *Config) {
	recordingStopMu.Lock()
	if recordingStopCh != nil {
		// 正在录音，停止
		close(recordingStopCh)
		recordingStopCh = nil
		recordingStopMu.Unlock()
		return
	}
	// 开始录音
	ch := make(chan struct{})
	recordingStopCh = ch
	recordingStopMu.Unlock()

	go func() {
		handleRecord(ch, cfg)
		recordingStopMu.Lock()
		if recordingStopCh == ch {
			recordingStopCh = nil
		}
		recordingStopMu.Unlock()
	}()
}
