package main

import (
	"fmt"
	"image/color"
	"log/slog"
	"os/exec"
	"runtime"
	"strconv"
	"sync"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"golang.design/x/hotkey"
)

var (
	currentHotkey   *hotkey.Hotkey
	currentHotkeyMu sync.Mutex

	// 录音切换状态：非 nil 表示正在录音，关闭它触发停止
	recordingStopCh chan struct{}
	recordingStopMu sync.Mutex
)

// 自定义暗色主题
type voiceTheme struct{}

func (t *voiceTheme) Color(n fyne.ThemeColorName, v fyne.ThemeVariant) color.Color {
	switch n {
	case theme.ColorNameBackground:
		return color.NRGBA{R: 18, G: 18, B: 24, A: 255}
	case theme.ColorNameForeground:
		return color.NRGBA{R: 230, G: 230, B: 240, A: 255}
	case theme.ColorNamePrimary:
		return color.NRGBA{R: 100, G: 180, B: 255, A: 255}
	case theme.ColorNameInputBackground:
		return color.NRGBA{R: 30, G: 30, B: 40, A: 255}
	case theme.ColorNamePlaceHolder:
		return color.NRGBA{R: 100, G: 100, B: 120, A: 255}
	case theme.ColorNameSeparator:
		return color.NRGBA{R: 50, G: 50, B: 65, A: 255}
	case theme.ColorNameButton:
		return color.NRGBA{R: 60, G: 130, B: 230, A: 255}
	case theme.ColorNameDisabled:
		return color.NRGBA{R: 70, G: 70, B: 90, A: 255}
	case theme.ColorNameHover:
		return color.NRGBA{R: 40, G: 40, B: 55, A: 255}
	case theme.ColorNamePressed:
		return color.NRGBA{R: 50, G: 100, B: 200, A: 255}
	}
	return theme.DefaultTheme().Color(n, v)
}

func (t *voiceTheme) Size(s fyne.ThemeSizeName) float32 {
	switch s {
	case theme.SizeNamePadding:
		return 10
	case theme.SizeNameInnerPadding:
		return 6
	case theme.SizeNameText:
		return 13
	case theme.SizeNameHeadingText:
		return 18
	case theme.SizeNameSubHeadingText:
		return 15
	}
	return theme.DefaultTheme().Size(s)
}

func (t *voiceTheme) Font(style fyne.TextStyle) fyne.Resource {
	return theme.DefaultTheme().Font(style)
}

func (t *voiceTheme) Icon(name fyne.ThemeIconName) fyne.Resource {
	return theme.DefaultTheme().Icon(name)
}

// 创建带标题的卡片区块
func sectionCard(title, emoji string, content fyne.CanvasObject) fyne.CanvasObject {
	header := container.NewHBox(
		canvas.NewText(emoji, color.NRGBA{R: 100, G: 180, B: 255, A: 255}),
		canvas.NewText(title, color.NRGBA{R: 100, G: 180, B: 255, A: 255}),
	)
	header.Objects[0].(*canvas.Text).TextSize = 16                          //nolint:forcetypeassert // widget tree constructed in this function, type is known
	header.Objects[1].(*canvas.Text).TextSize = 14                          //nolint:forcetypeassert // widget tree constructed in this function, type is known
	header.Objects[1].(*canvas.Text).TextStyle = fyne.TextStyle{Bold: true} //nolint:forcetypeassert // widget tree constructed in this function, type is known

	bg := canvas.NewRectangle(color.NRGBA{R: 24, G: 24, B: 34, A: 255})
	bg.CornerRadius = 12

	inner := container.NewPadded(
		container.NewVBox(
			header,
			widget.NewSeparator(),
			content,
		),
	)

	return container.NewStack(bg, inner)
}

func ShowSettings(app fyne.App, cfg *Config) { //nolint:gocyclo,funlen // GUI settings form with multiple ASR provider sections is inherently complex
	w := app.NewWindow("voice-claude 设置")
	w.Resize(fyne.NewSize(520, 640))

	// === 语音识别 ===
	providerLabels := []string{
		ASRProviderVolc + "（实时）",
		ASRProviderXfyun + "（实时）",
		ASRProviderZhipu + "（准确优先）",
		ASRProviderOpenRouter + "（准确优先）",
		ASRProviderLocal + "（离线/隐私）",
	}
	providerValues := []string{
		ASRProviderVolc, ASRProviderXfyun, ASRProviderZhipu, ASRProviderOpenRouter, ASRProviderLocal,
	}
	providerLabelOf := func(v string) string {
		for i, pv := range providerValues {
			if pv == v {
				return providerLabels[i]
			}
		}
		return v
	}
	providerValueOf := func(label string) string {
		for i, pl := range providerLabels {
			if pl == label {
				return providerValues[i]
			}
		}
		return label
	}
	providerSelect := widget.NewSelect(providerLabels, nil)
	providerSelect.SetSelected(providerLabelOf(cfg.ASRProvider))

	// 智谱配置
	apiKeyEntry := widget.NewPasswordEntry()
	apiKeyEntry.SetText(cfg.ASRAPIKey)
	apiKeyEntry.SetPlaceHolder("粘贴智谱 API Key...")
	zhipuSection := container.NewVBox(
		widget.NewLabel("智谱 API Key"),
		apiKeyEntry,
	)

	// 讯飞配置
	xfyunAppIDEntry := widget.NewEntry()
	xfyunAppIDEntry.SetText(cfg.XfyunAppID)
	xfyunAppIDEntry.SetPlaceHolder("App ID")
	xfyunKeyIDEntry := widget.NewEntry()
	xfyunKeyIDEntry.SetText(cfg.XfyunAccessKeyID)
	xfyunKeyIDEntry.SetPlaceHolder("Access Key ID")
	xfyunSecretEntry := widget.NewPasswordEntry()
	xfyunSecretEntry.SetText(cfg.XfyunAccessSecret)
	xfyunSecretEntry.SetPlaceHolder("Access Key Secret")
	xfyunSection := container.NewVBox(
		widget.NewLabel("App ID"),
		xfyunAppIDEntry,
		widget.NewLabel("Access Key ID"),
		xfyunKeyIDEntry,
		widget.NewLabel("Access Key Secret"),
		xfyunSecretEntry,
	)

	// OpenRouter 配置（ASR + 纠错共用）
	openrouterKeyEntry := widget.NewPasswordEntry()
	openrouterKeyEntry.SetText(cfg.OpenRouterAPIKey)
	openrouterKeyEntry.SetPlaceHolder("OpenRouter API Key")
	openrouterSection := container.NewVBox(
		widget.NewLabel("OpenRouter API Key"),
		openrouterKeyEntry,
		widget.NewLabel("（ASR 用 whisper-large-v3-turbo，纠错可自选模型）"),
	)

	// 豆包/火山配置
	volcAppKeyEntry := widget.NewEntry()
	volcAppKeyEntry.SetText(cfg.VolcAppKey)
	volcAppKeyEntry.SetPlaceHolder("App ID")
	volcAccessTokenEntry := widget.NewPasswordEntry()
	volcAccessTokenEntry.SetText(cfg.VolcAccessToken)
	volcAccessTokenEntry.SetPlaceHolder("Access Token")
	volcResourceSelect := widget.NewSelect([]string{
		"volc.seedasr.sauc.duration",
		"volc.bigasr.sauc.duration",
	}, nil)
	if cfg.VolcResourceID == "" {
		volcResourceSelect.SetSelected("volc.seedasr.sauc.duration")
	} else {
		volcResourceSelect.SetSelected(cfg.VolcResourceID)
	}
	volcSection := container.NewVBox(
		widget.NewLabel("App ID"),
		volcAppKeyEntry,
		widget.NewLabel("Access Token"),
		volcAccessTokenEntry,
		widget.NewLabel("识别模型"),
		volcResourceSelect,
		widget.NewLabel("（2.0 = SeedASR，1.0 = BigASR，注册送 40 小时额度）"),
	)

	// 本地 SenseVoice 配置
	localStatusLabel := widget.NewLabel("")
	localProgressBar := widget.NewProgressBar()
	localProgressBar.Hide()
	downloadBtn := widget.NewButton("下载模型", nil)

	refreshLocalStatus := func() {
		if IsSenseVoiceAvailable() {
			localStatusLabel.SetText("状态：模型已就绪 ✓")
			downloadBtn.SetText("重新下载")
		} else {
			localStatusLabel.SetText("状态：模型未下载")
			downloadBtn.SetText("下载模型（约 1GB）")
		}
	}
	refreshLocalStatus()

	downloadBtn.OnTapped = func() {
		downloadBtn.Disable()
		localProgressBar.SetValue(0)
		localProgressBar.Show()
		localStatusLabel.SetText("下载中…")

		go func() {
			defer func() {
				if r := recover(); r != nil {
					slog.Error("模型下载 panic", "recover", r)
					localStatusLabel.SetText("下载失败：内部错误")
					localProgressBar.Hide()
					downloadBtn.Enable()
				}
			}()
			err := DownloadSenseVoiceModel(func(progress float64) {
				localProgressBar.SetValue(progress)
			})
			if err != nil {
				localStatusLabel.SetText("下载失败：" + err.Error())
				localProgressBar.Hide()
				downloadBtn.Enable()
				return
			}
			localProgressBar.Hide()
			downloadBtn.Enable()
			refreshLocalStatus()
		}()
	}

	openDirBtn := widget.NewButton("打开模型目录", func() {
		dir := senseVoiceModelPath()
		var cmd *exec.Cmd
		switch runtime.GOOS {
		case "darwin":
			cmd = exec.Command("open", dir)
		case "windows":
			cmd = exec.Command("explorer", dir)
		default:
			cmd = exec.Command("xdg-open", dir)
		}
		if err := cmd.Start(); err != nil {
			slog.Warn("打开目录失败", "error", err)
		}
	})

	localSection := container.NewVBox(
		widget.NewLabel("本地 SenseVoice（离线，无需 API Key）"),
		localStatusLabel,
		localProgressBar,
		container.NewHBox(downloadBtn, openDirBtn),
	)

	asrFields := container.NewVBox(zhipuSection, xfyunSection, volcSection, openrouterSection, localSection)

	updateASRVisibility := func(label string) {
		zhipuSection.Hide()
		xfyunSection.Hide()
		volcSection.Hide()
		openrouterSection.Hide()
		localSection.Hide()
		switch providerValueOf(label) {
		case ASRProviderXfyun:
			xfyunSection.Show()
		case ASRProviderVolc:
			volcSection.Show()
		case ASRProviderOpenRouter:
			openrouterSection.Show()
		case ASRProviderLocal:
			localSection.Show()
		default:
			zhipuSection.Show()
		}
	}
	providerSelect.OnChanged = updateASRVisibility
	updateASRVisibility(providerLabelOf(cfg.ASRProvider))

	asrSection := sectionCard("语音识别引擎", "🎙", container.NewVBox(
		widget.NewLabel("识别后端"),
		providerSelect,
		asrFields,
	))

	// === AI 纠错 ===
	correctSelect := widget.NewSelect([]string{CorrectModeOff, CorrectModeOllama, CorrectModeOpenRouter, CorrectModeCloud}, nil)
	correctSelect.SetSelected(cfg.CorrectMode)

	correctURLEntry := widget.NewEntry()
	correctURLEntry.SetText(cfg.CorrectURL)
	correctURLEntry.SetPlaceHolder("http://localhost:11434/api/generate")

	correctModelEntry := widget.NewEntry()
	correctModelEntry.SetText(cfg.CorrectModel)
	correctModelEntry.SetPlaceHolder("qwen2.5:3b")

	correctKeyEntry := widget.NewPasswordEntry()
	correctKeyEntry.SetText(cfg.CorrectAPIKey)
	correctKeyEntry.SetPlaceHolder("仅云端模式需要")

	correctTimeoutEntry := widget.NewEntry()
	correctTimeoutEntry.SetText(strconv.Itoa(cfg.CorrectTimeout))
	correctTimeoutEntry.SetPlaceHolder("10")

	correctExtra := container.NewVBox(
		widget.NewLabel("API 地址"),
		correctURLEntry,
		widget.NewLabel("模型名称"),
		correctModelEntry,
		widget.NewLabel("API Key"),
		correctKeyEntry,
		widget.NewLabel("超时（秒）"),
		correctTimeoutEntry,
	)
	correctSelect.OnChanged = func(mode string) {
		switch mode {
		case CorrectModeOff:
			correctExtra.Hide()
		case CorrectModeOllama:
			correctExtra.Show()
			if err := CheckOllama(correctURLEntry.Text); err != nil {
				dialog.ShowError(err, w)
			}
		default:
			correctExtra.Show()
		}
	}
	if cfg.CorrectMode == CorrectModeOff {
		correctExtra.Hide()
	}

	aiSection := sectionCard("AI 语音纠错", "🧠", container.NewVBox(
		widget.NewLabel("纠错模式"),
		correctSelect,
		correctExtra,
	))

	// === 录音设置 ===
	// 枚举麦克风设备
	deviceNames := []string{"(默认设备)"}
	devices, _ := ListCaptureDevices()
	for _, d := range devices {
		deviceNames = append(deviceNames, d.Name)
	}
	deviceSelect := widget.NewSelect(deviceNames, nil)
	if cfg.DeviceName == "" {
		deviceSelect.SetSelected("(默认设备)")
	} else {
		deviceSelect.SetSelected(cfg.DeviceName)
	}

	hotkeyEntry := widget.NewEntry()
	hotkeyEntry.SetText(cfg.Hotkey)
	hotkeyEntry.SetPlaceHolder("cmd+shift+f5")

	gainLabel := widget.NewLabel(fmt.Sprintf("增益: %dx", cfg.Gain))
	gainSlider := widget.NewSlider(1, 10)
	gainSlider.Step = 1
	gainSlider.Value = float64(cfg.Gain)
	gainSlider.OnChanged = func(v float64) {
		n := int(v)
		label := fmt.Sprintf("增益: %dx", n)
		if n == 1 {
			label = "增益: 1x (无增益)"
		}
		gainLabel.SetText(label)
	}

	recordSection := sectionCard("录音参数", "🎤", container.NewVBox(
		widget.NewLabel("麦克风"),
		deviceSelect,
		widget.NewLabel("录音快捷键"),
		hotkeyEntry,
		widget.NewLabel("信号增益"),
		container.NewBorder(nil, nil, nil, gainLabel, gainSlider),
	))

	// === 日志设置 ===
	logLevelSelect := widget.NewSelect([]string{"debug", "info", "warn", "error"}, nil)
	logLevelSelect.SetSelected(cfg.LogLevel)

	logSection := sectionCard("日志", "📋", container.NewVBox(
		widget.NewLabel("日志级别"),
		logLevelSelect,
	))

	// === 保存按钮 ===
	saveBtn := widget.NewButton("保存配置", func() {
		oldHotkey := cfg.Hotkey

		cfg.ASRProvider = providerValueOf(providerSelect.Selected)
		cfg.ASRAPIKey = apiKeyEntry.Text
		cfg.XfyunAppID = xfyunAppIDEntry.Text
		cfg.XfyunAccessKeyID = xfyunKeyIDEntry.Text
		cfg.XfyunAccessSecret = xfyunSecretEntry.Text
		cfg.OpenRouterAPIKey = openrouterKeyEntry.Text
		cfg.VolcAppKey = volcAppKeyEntry.Text
		cfg.VolcAccessToken = volcAccessTokenEntry.Text
		cfg.VolcResourceID = volcResourceSelect.Selected
		cfg.CorrectMode = correctSelect.Selected
		cfg.CorrectURL = correctURLEntry.Text
		cfg.CorrectModel = correctModelEntry.Text
		cfg.CorrectAPIKey = correctKeyEntry.Text
		if t, err := strconv.Atoi(correctTimeoutEntry.Text); err == nil && t > 0 {
			cfg.CorrectTimeout = t
		} else {
			cfg.CorrectTimeout = 10
		}
		cfg.Hotkey = hotkeyEntry.Text
		cfg.Gain = int(gainSlider.Value)
		cfg.DeviceName = deviceSelect.Selected
		if cfg.DeviceName == "(默认设备)" {
			cfg.DeviceName = ""
		}
		cfg.LogLevel = logLevelSelect.Selected
		SetLogLevel(cfg.LogLevel)

		if err := cfg.Save(); err != nil {
			dialog.ShowError(err, w)
			return
		}

		if cfg.Hotkey != oldHotkey {
			if err := reRegisterHotkey(cfg); err != nil {
				dialog.ShowError(fmt.Errorf("快捷键注册失败: %w", err), w)
				return
			}
		}

		dialog.ShowInformation("保存成功", "配置已生效", w)
	})
	saveBtn.Importance = widget.HighImportance

	// === 整体布局 ===
	content := container.NewVBox(
		asrSection,
		widget.NewSeparator(),
		aiSection,
		widget.NewSeparator(),
		recordSection,
		widget.NewSeparator(),
		logSection,
		layout.NewSpacer(),
		container.NewCenter(saveBtn),
	)

	scrollContent := container.NewVScroll(container.NewPadded(content))
	w.SetContent(scrollContent)
	w.Show()
}

func reRegisterHotkey(cfg *Config) error {
	currentHotkeyMu.Lock()
	old := currentHotkey
	currentHotkeyMu.Unlock()

	if old != nil {
		_ = old.Unregister()
	}

	parsed, err := ParseHotkey(cfg.Hotkey)
	if err != nil {
		return err
	}

	hk := hotkey.New(parsed.Mods, parsed.Key)
	if err := hk.Register(); err != nil {
		return err
	}

	currentHotkeyMu.Lock()
	currentHotkey = hk
	currentHotkeyMu.Unlock()

	go func() {
		for range hk.Keydown() {
			toggleRecording(cfg)
		}
	}()
	slog.Info("快捷键已更新", "hotkey", cfg.Hotkey)
	return nil
}
