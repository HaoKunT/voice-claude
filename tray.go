package main

import (
	"os/exec"
	"runtime"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
)

// SetupTray 初始化系统托盘菜单，仅在支持桌面扩展的平台上生效。
func SetupTray(app fyne.App, onSettings, onHistory, onQuit func()) {
	if desk, ok := app.(desktop.App); ok {
		menu := fyne.NewMenu("voice-claude",
			fyne.NewMenuItem("设置", onSettings),
			fyne.NewMenuItem("历史记录", onHistory),
			fyne.NewMenuItem("打开日志", openLogFile),
			fyne.NewMenuItemSeparator(),
			fyne.NewMenuItem("退出", onQuit),
		)
		desk.SetSystemTrayMenu(menu)
	}
}

func openLogFile() {
	logPath := appLogDir() + "/voice-claude.log"
	switch runtime.GOOS {
	case "darwin":
		_ = exec.Command("open", logPath).Start()
	case "windows":
		_ = exec.Command("notepad", logPath).Start()
	default:
		_ = exec.Command("xdg-open", logPath).Start()
	}
}
