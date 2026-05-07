//go:build !nosherpa

package main

import (
	"archive/tar"
	"compress/bzip2"
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
	senseVoiceModelDir = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
	senseVoiceModelURL = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"
)

func senseVoiceModelPath() string {
	return filepath.Join(configDir(), senseVoiceModelDir)
}

// DownloadSenseVoiceModel 下载并解压 SenseVoice 模型，onProgress 回调 0.0~1.0。
func DownloadSenseVoiceModel(onProgress func(float64)) error {
	destDir := configDir()
	os.MkdirAll(destDir, 0o755) //nolint:errcheck,gosec // best-effort directory creation

	slog.Info("开始下载 SenseVoice 模型", "url", senseVoiceModelURL)

	resp, err := http.Get(senseVoiceModelURL)
	if err != nil {
		return fmt.Errorf("下载失败: %w", err)
	}
	defer resp.Body.Close() //nolint:errcheck // deferred body close

	total := resp.ContentLength
	var downloaded int64

	// 带进度的读取
	reader := &progressReader{
		r: resp.Body,
		onRead: func(n int) {
			downloaded += int64(n)
			if total > 0 && onProgress != nil {
				onProgress(float64(downloaded) / float64(total) * 0.9) // 前 90% 给下载
			}
		},
	}

	// 解压 tar.bz2
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

		// 去掉顶层目录前缀，解压到 configDir/senseVoiceModelDir/
		_, after, ok := strings.Cut(header.Name, "/")
		if !ok || after == "" {
			continue
		}
		target := filepath.Join(destDir, senseVoiceModelDir, filepath.FromSlash(after))
		// Zip Slip 防护：确保解压路径在目标目录内
		safeRoot := filepath.Clean(filepath.Join(destDir, senseVoiceModelDir)) + string(os.PathSeparator)
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
				f.Close() //nolint:errcheck,gosec // error path, original error takes precedence
				return fmt.Errorf("写入文件失败 %q: %w", target, err)
			}
			if err := f.Close(); err != nil {
				return fmt.Errorf("关闭文件失败 %q: %w", target, err)
			}
		}
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
