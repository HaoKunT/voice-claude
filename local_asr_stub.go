//go:build nosherpa

package main

import "fmt"

func IsSenseVoiceAvailable() bool { return false }
func senseVoiceModelPath() string { return "" }

func DownloadSenseVoiceModel(_ func(float64)) error {
	return fmt.Errorf("本地 ASR 在此平台不支持")
}

func TranscribeLocal(_ []byte) (string, error) {
	return "", fmt.Errorf("本地 ASR 未编译，请使用云端后端")
}
