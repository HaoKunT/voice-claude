package main

import (
	"github.com/go-vgo/robotgo"
)

// TypeText 将文字模拟键盘输入到当前焦点窗口。
func TypeText(text string) {
	robotgo.Type(text)
}

// DeleteChars 发送 n 个退格键，用于删除之前输入的中间结果。
func DeleteChars(n int) {
	for range n {
		_ = robotgo.KeyTap("backspace")
	}
}
