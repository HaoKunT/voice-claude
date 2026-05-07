//go:build windows

package main

import "golang.design/x/hotkey"

// Windows 上没有 Cmd/Option 键，cmd/command 映射到 Win 键，option/alt 映射到 Alt 键。
var modMap = map[string]hotkey.Modifier{
	"ctrl":     hotkey.ModCtrl,
	"command":  hotkey.ModWin,
	"cmd":      hotkey.ModWin,
	"win":      hotkey.ModWin,
	"option":   hotkey.ModAlt,
	"alt":      hotkey.ModAlt,
	"shift":    hotkey.ModShift,
	"rctrl":    hotkey.ModCtrl,
	"rcommand": hotkey.ModWin,
	"rcmd":     hotkey.ModWin,
	"rwin":     hotkey.ModWin,
	"roption":  hotkey.ModAlt,
	"ralt":     hotkey.ModAlt,
	"rshift":   hotkey.ModShift,
}
