package main

import (
	"github.com/go-vgo/robotgo"
)

// TypeText 将文字模拟键盘输入到当前焦点窗口。
func TypeText(text string) {
	robotgo.Type(text)
}
