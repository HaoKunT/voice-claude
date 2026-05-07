package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"log/slog"
	"math"
	"sync"
	"unsafe"

	"github.com/gen2brain/malgo"
)

// CaptureDevice 录音设备信息
type CaptureDevice struct {
	Name string
	ID   malgo.DeviceID
}

// ListCaptureDevices 枚举所有可用录音设备
func ListCaptureDevices() ([]CaptureDevice, error) {
	ctx, err := malgo.InitContext(nil, malgo.ContextConfig{}, nil)
	if err != nil {
		return nil, err
	}
	defer ctx.Free()

	infos, err := ctx.Devices(malgo.Capture)
	if err != nil {
		return nil, err
	}

	var devices []CaptureDevice
	for _, info := range infos {
		devices = append(devices, CaptureDevice{
			Name: info.Name(),
			ID:   info.ID,
		})
	}
	return devices, nil
}

type Recorder struct {
	gain       int
	deviceName string
	ctx        *malgo.AllocatedContext
	device     *malgo.Device
	buffer     []byte
	mu         sync.Mutex
	recording  bool
	streamCh   chan []byte
}

// NewRecorder 创建录音器，gain 为信号增益倍数（1-10），deviceName 为空时使用默认设备。
func NewRecorder(gain int, deviceName string) (*Recorder, error) {
	ctx, err := malgo.InitContext(nil, malgo.ContextConfig{}, nil)
	if err != nil {
		return nil, fmt.Errorf("init audio context: %w", err)
	}
	return &Recorder{ctx: ctx, gain: gain, deviceName: deviceName}, nil
}

func (r *Recorder) findDeviceID() unsafe.Pointer {
	if r.deviceName == "" {
		return nil
	}
	infos, err := r.ctx.Devices(malgo.Capture)
	if err != nil {
		return nil
	}
	for _, info := range infos {
		if info.Name() == r.deviceName {
			return info.ID.Pointer()
		}
	}
	return nil
}

func (r *Recorder) Close() {
	if r.device != nil {
		r.device.Uninit()
	}
	r.ctx.Free()
}

func (r *Recorder) Start() error {
	r.mu.Lock()
	r.buffer = nil
	r.recording = true
	r.mu.Unlock()

	deviceConfig := malgo.DefaultDeviceConfig(malgo.Capture)
	deviceConfig.Capture.Format = malgo.FormatS16
	deviceConfig.Capture.Channels = 1
	deviceConfig.SampleRate = 16000
	deviceConfig.Alsa.NoMMap = 1

	selectedDevice := "(默认)"
	if devID := r.findDeviceID(); devID != nil {
		deviceConfig.Capture.DeviceID = devID
		selectedDevice = r.deviceName
	}
	slog.Info("录音设备", "device", selectedDevice, "format", "S16", "rate", 16000, "channels", 1, "gain", r.gain)

	callbacks := malgo.DeviceCallbacks{
		Data: func(_, pInputSample []byte, _ uint32) {
			r.mu.Lock()
			defer r.mu.Unlock()
			if r.recording {
				chunk := make([]byte, len(pInputSample))
				copy(chunk, pInputSample)
				r.buffer = append(r.buffer, chunk...)
				if r.streamCh != nil {
					select {
					case r.streamCh <- chunk:
					default:
					}
				}
			}
		},
	}

	var err error
	r.device, err = malgo.InitDevice(r.ctx.Context, deviceConfig, callbacks)
	if err != nil {
		return fmt.Errorf("init device: %w", err)
	}

	return r.device.Start()
}

// StartStream 开启流式 PCM 输出，录音的每个音频块都会推送到返回的 channel。
// 必须在 Start() 之前调用。
func (r *Recorder) StartStream() <-chan []byte {
	ch := make(chan []byte, 64)
	r.mu.Lock()
	r.streamCh = ch
	r.mu.Unlock()
	return ch
}

// StopStream 停止流式输出并关闭 channel。
func (r *Recorder) StopStream() {
	r.mu.Lock()
	ch := r.streamCh
	r.streamCh = nil
	r.mu.Unlock()
	if ch != nil {
		close(ch)
	}
}

func (r *Recorder) Stop() []byte {
	r.mu.Lock()
	r.recording = false
	r.mu.Unlock()

	if r.device != nil {
		r.device.Uninit()
		r.device = nil
	}

	r.mu.Lock()
	defer r.mu.Unlock()
	return r.buffer
}

func applyGain(pcm []byte, gain int) []int16 {
	samples := make([]int16, len(pcm)/2)
	for i := range samples {
		s := int16(binary.LittleEndian.Uint16(pcm[i*2 : i*2+2]))
		gained := math.Max(-32768, math.Min(32767, float64(s)*float64(gain)))
		samples[i] = int16(gained)
	}
	return samples
}

func (r *Recorder) ToWAV(pcm []byte) []byte {
	slog.Debug("音频处理", "pcm_bytes", len(pcm), "gain", r.gain)

	samples := applyGain(pcm, r.gain)

	// 统计音频强度
	var maxAbs int16
	for _, s := range samples {
		if abs16(s) > maxAbs {
			maxAbs = abs16(s)
		}
	}
	slog.Debug("音频强度", "max_amplitude", int(maxAbs))

	// 静音裁剪：去掉首尾静音
	threshold := int16(30)
	firstNonSilent := 0
	for firstNonSilent < len(samples) && abs16(samples[firstNonSilent]) < threshold {
		firstNonSilent++
	}
	lastNonSilent := len(samples) - 1
	for lastNonSilent > firstNonSilent && abs16(samples[lastNonSilent]) < threshold {
		lastNonSilent--
	}
	if firstNonSilent < len(samples) {
		samples = samples[firstNonSilent : lastNonSilent+1]
	}

	// 最少保留 0.5 秒音频，避免裁剪过度
	minSamples := 16000 / 2
	if len(samples) < minSamples {
		slog.Warn("裁剪后音频过短，使用原始数据", "trimmed", len(samples), "original", len(pcm)/2)
		samples = applyGain(pcm, r.gain)
	}
	slog.Debug("静音裁剪", "samples_before", len(pcm)/2, "samples_after", len(samples))

	pcmBytes := make([]byte, len(samples)*2)
	for i, s := range samples {
		binary.LittleEndian.PutUint16(pcmBytes[i*2:i*2+2], uint16(s))
	}

	var buf bytes.Buffer
	dataSize := uint32(len(pcmBytes))
	fileSize := 36 + dataSize

	buf.WriteString("RIFF")
	binary.Write(&buf, binary.LittleEndian, fileSize) //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	buf.WriteString("WAVE")

	buf.WriteString("fmt ")
	binary.Write(&buf, binary.LittleEndian, uint32(16))    //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint16(1))     //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint16(1))     //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint32(16000)) //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint32(32000)) //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint16(2))     //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	binary.Write(&buf, binary.LittleEndian, uint16(16))    //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails

	buf.WriteString("data")
	binary.Write(&buf, binary.LittleEndian, dataSize) //nolint:errcheck,gosec // binary.Write to bytes.Buffer never fails
	buf.Write(pcmBytes)

	wav := buf.Bytes()
	slog.Debug("WAV 生成", "wav_bytes", len(wav))
	return wav
}

func abs16(x int16) int16 {
	if x < 0 {
		return -x
	}
	return x
}
