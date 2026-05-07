//go:build !nosherpa

package main

import (
	"archive/tar"
	"compress/bzip2"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	sherpa "github.com/k2-fsa/sherpa-onnx-go/sherpa_onnx"
)

const (
	senseVoiceModelDir    = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
	senseVoiceModelURL    = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"
	senseVoiceModelSHA256 = "8148030f23c4bc0848239c80b635f3a0a1c275a2ae7ae37469bbe2341aa96d3f"
)

func senseVoiceModelPath() string {
	return filepath.Join(configDir(), senseVoiceModelDir)
}

// DownloadSenseVoiceModel 下载并解压 SenseVoice 模型，onProgress 回调 0.0~1.0。
// 下载完成后对 tar.bz2 做 SHA256 校验，校验失败会删除已下载数据。
func DownloadSenseVoiceModel(onProgress func(float64)) error {
	destDir := configDir()
	os.MkdirAll(destDir, 0o755) //nolint:errcheck,gosec // best-effort directory creation

	slog.Info("开始下载 SenseVoice 模型", "url", senseVoiceModelURL)

	resp, err := http.Get(senseVoiceModelURL) //nolint:gosec,noctx // URL 为硬编码常量，无需 context
	if err != nil {
		return fmt.Errorf("下载失败: %w", err)
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	total := resp.ContentLength
	var downloaded int64
	hasher := sha256.New()

	// 带进度的读取，同时计算 SHA256
	reader := &progressReader{
		r: io.TeeReader(resp.Body, hasher),
		onRead: func(n int) {
			downloaded += int64(n)
			if total > 0 && onProgress != nil {
				onProgress(float64(downloaded) / float64(total) * 0.9) // 前 90% 给下载
			}
		},
	}

	// 先解压到临时目录，校验通过后再移动到目标目录
	tmpDir, err := os.MkdirTemp(destDir, "sense-voice-tmp-")
	if err != nil {
		return fmt.Errorf("创建临时目录失败: %w", err)
	}
	defer os.RemoveAll(tmpDir) //nolint:errcheck // cleanup on any exit path

	bzReader := bzip2.NewReader(reader)
	tarReader := tar.NewReader(bzReader)

	for {
		header, err := tarReader.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return fmt.Errorf("解压失败: %w", err)
		}

		// 去掉顶层目录前缀，解压到临时目录
		_, after, ok := strings.Cut(header.Name, "/")
		if !ok || after == "" {
			continue
		}
		target := filepath.Join(tmpDir, filepath.FromSlash(after))
		// Zip Slip 防护
		safeRoot := filepath.Clean(tmpDir) + string(os.PathSeparator)
		if !strings.HasPrefix(filepath.Clean(target)+string(os.PathSeparator), safeRoot) {
			return fmt.Errorf("非法路径: %s", header.Name)
		}

		switch header.Typeflag {
		case tar.TypeDir:
			os.MkdirAll(target, 0o755) //nolint:errcheck,gosec // best-effort directory creation
		case tar.TypeReg:
			os.MkdirAll(filepath.Dir(target), 0o755) //nolint:errcheck,gosec // best-effort directory creation
			f, err := os.Create(target)
			if err != nil {
				return fmt.Errorf("创建文件失败 %q: %w", target, err)
			}
			if _, err := io.Copy(f, tarReader); err != nil {
				f.Close() //nolint:errcheck // error path
				return fmt.Errorf("写入文件失败 %q: %w", target, err)
			}
			if err := f.Close(); err != nil {
				return fmt.Errorf("关闭文件失败 %q: %w", target, err)
			}
		}
	}

	// SHA256 校验（在全部数据流经 hasher 之后）
	got := hex.EncodeToString(hasher.Sum(nil))
	if got != senseVoiceModelSHA256 {
		return fmt.Errorf("SHA256 校验失败（期望 %s，实际 %s），文件可能损坏", senseVoiceModelSHA256, got)
	}
	slog.Info("SHA256 校验通过", "hash", got)

	// 移动临时目录到最终位置
	finalDir := filepath.Join(destDir, senseVoiceModelDir)
	os.RemoveAll(finalDir) //nolint:errcheck // replace existing
	if err := os.Rename(tmpDir, finalDir); err != nil {
		return fmt.Errorf("移动模型目录失败: %w", err)
	}

	if onProgress != nil {
		onProgress(1.0)
	}
	slog.Info("SenseVoice 模型下载完成", "dir", filepath.Join(destDir, senseVoiceModelDir))
	return nil
}

type progressReader struct {
	r      io.Reader
	onRead func(int)
}

func (p *progressReader) Read(buf []byte) (int, error) {
	n, err := p.r.Read(buf)
	if n > 0 && p.onRead != nil {
		p.onRead(n)
	}
	return n, err
}

// IsSenseVoiceAvailable 检查模型文件是否已下载
func IsSenseVoiceAvailable() bool {
	modelFile := filepath.Join(senseVoiceModelPath(), "model.int8.onnx")
	_, err := os.Stat(modelFile)
	return err == nil
}

// TranscribeLocal 使用本地 SenseVoice 识别 WAV 字节数据
func TranscribeLocal(wavBytes []byte) (string, error) {
	modelDir := senseVoiceModelPath()
	modelFile := filepath.Join(modelDir, "model.int8.onnx")
	tokensFile := filepath.Join(modelDir, "tokens.txt")

	if _, err := os.Stat(modelFile); err != nil {
		return "", fmt.Errorf("模型文件不存在，请先下载: %q", modelFile)
	}

	config := sherpa.OfflineRecognizerConfig{}
	config.FeatConfig.SampleRate = 16000
	config.FeatConfig.FeatureDim = 80
	config.ModelConfig.SenseVoice.Model = modelFile
	config.ModelConfig.SenseVoice.Language = "zh"
	config.ModelConfig.SenseVoice.UseInverseTextNormalization = 1
	config.ModelConfig.Tokens = tokensFile
	config.ModelConfig.NumThreads = 2
	config.ModelConfig.Provider = "cpu"
	config.DecodingMethod = "greedy_search"

	slog.Debug("初始化 SenseVoice 识别器")
	recognizer := sherpa.NewOfflineRecognizer(&config)
	if recognizer == nil {
		return "", errors.New("SenseVoice 初始化失败，请重新下载模型")
	}
	defer sherpa.DeleteOfflineRecognizer(recognizer)

	stream := sherpa.NewOfflineStream(recognizer)
	if stream == nil {
		return "", errors.New("创建识别流失败")
	}
	defer sherpa.DeleteOfflineStream(stream)

	// 从 WAV 字节解析 PCM samples
	samples, sampleRate, err := wavBytesToSamples(wavBytes)
	if err != nil {
		return "", fmt.Errorf("解析 WAV 失败: %w", err)
	}

	stream.AcceptWaveform(sampleRate, samples)
	recognizer.Decode(stream)

	result := stream.GetResult()
	slog.Debug("SenseVoice 识别结果", "text", result.Text, "lang", result.Lang)
	return result.Text, nil
}

// wavBytesToSamples 从 WAV 字节数组解析 float32 PCM 采样
func wavBytesToSamples(wavBytes []byte) ([]float32, int, error) {
	if len(wavBytes) < 44 {
		return nil, 0, errors.New("WAV 数据过短")
	}

	sampleRate := int(uint32(wavBytes[24]) | uint32(wavBytes[25])<<8 |
		uint32(wavBytes[26])<<16 | uint32(wavBytes[27])<<24)

	pcm := wavBytes[44:]
	samples := make([]float32, len(pcm)/2)
	for i := range samples {
		s := int16(uint16(pcm[i*2]) | uint16(pcm[i*2+1])<<8)
		samples[i] = float32(s) / 32768.0
	}
	return samples, sampleRate, nil
}
