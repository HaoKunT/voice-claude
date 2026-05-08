//go:build darwin

package main

import "golang.design/x/hotkey"

var modMap = map[string]hotkey.Modifier{
	"ctrl":     hotkey.ModCtrl,
	"command":  hotkey.ModCmd,
	"cmd":      hotkey.ModCmd,
	"option":   hotkey.ModOption,
	"alt":      hotkey.ModOption,
	"shift":    hotkey.ModShift,
	"rctrl":    hotkey.ModCtrl,
	"rcommand": hotkey.ModCmd,
	"rcmd":     hotkey.ModCmd,
	"roption":  hotkey.ModOption,
	"ralt":     hotkey.ModOption,
	"rshift":   hotkey.ModShift,
}
