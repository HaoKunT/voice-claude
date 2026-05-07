package main

import (
	"errors"
	"fmt"
	"strings"

	"golang.design/x/hotkey"
)

var keyMap = map[string]hotkey.Key{
	"a": hotkey.KeyA, "b": hotkey.KeyB, "c": hotkey.KeyC, "d": hotkey.KeyD,
	"e": hotkey.KeyE, "f": hotkey.KeyF, "g": hotkey.KeyG, "h": hotkey.KeyH,
	"i": hotkey.KeyI, "j": hotkey.KeyJ, "k": hotkey.KeyK, "l": hotkey.KeyL,
	"m": hotkey.KeyM, "n": hotkey.KeyN, "o": hotkey.KeyO, "p": hotkey.KeyP,
	"q": hotkey.KeyQ, "r": hotkey.KeyR, "s": hotkey.KeyS, "t": hotkey.KeyT,
	"u": hotkey.KeyU, "v": hotkey.KeyV, "w": hotkey.KeyW, "x": hotkey.KeyX,
	"y": hotkey.KeyY, "z": hotkey.KeyZ,
	"0": hotkey.Key0, "1": hotkey.Key1, "2": hotkey.Key2, "3": hotkey.Key3,
	"4": hotkey.Key4, "5": hotkey.Key5, "6": hotkey.Key6, "7": hotkey.Key7,
	"8": hotkey.Key8, "9": hotkey.Key9,
	"f1": hotkey.KeyF1, "f2": hotkey.KeyF2, "f3": hotkey.KeyF3, "f4": hotkey.KeyF4,
	"f5": hotkey.KeyF5, "f6": hotkey.KeyF6, "f7": hotkey.KeyF7, "f8": hotkey.KeyF8,
	"f9": hotkey.KeyF9, "f10": hotkey.KeyF10, "f11": hotkey.KeyF11, "f12": hotkey.KeyF12,
	"space": hotkey.KeySpace, "return": hotkey.KeyReturn, "tab": hotkey.KeyTab,
	"esc": hotkey.KeyEscape, "delete": hotkey.KeyDelete,
}

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

// ParsedHotkey 保存解析后的热键修饰键和主键。
type ParsedHotkey struct {
	Mods []hotkey.Modifier
	Key  hotkey.Key
}

// ParseHotkey 解析热键字符串（如 "cmd+shift+f5"），返回修饰键列表和主键。
// 格式：一个或多个修饰键加号连接主键，大小写不敏感。
func ParseHotkey(s string) (*ParsedHotkey, error) {
	parts := strings.Split(strings.ToLower(strings.TrimSpace(s)), "+")
	if len(parts) < 2 {
		return nil, errors.New("hotkey 格式应为 mod+key，如 rcommand+roption+space")
	}

	var mods []hotkey.Modifier
	var key hotkey.Key
	foundKey := false

	for _, p := range parts {
		p = strings.TrimSpace(p)
		if m, ok := modMap[p]; ok {
			mods = append(mods, m)
		} else if k, ok := keyMap[p]; ok {
			key = k
			foundKey = true
		} else {
			return nil, fmt.Errorf("未知按键: %q", p)
		}
	}

	if !foundKey {
		return nil, errors.New("需要一个主键（如 space、a-z），当前只有修饰键")
	}

	return &ParsedHotkey{Mods: mods, Key: key}, nil
}
