package main

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"strconv"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

const volcEndpoint = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async"

// VolcProtocol 帧头常量
const (
	volcVersion    = 0x01
	volcHeaderSize = 0x01 // 1 unit = 4 bytes

	// message type
	volcMsgFullClientRequest = 0x01
	volcMsgAudioOnlyRequest  = 0x02
	volcMsgServerResponse    = 0x09
	volcMsgServerError       = 0x0F

	// flags
	volcFlagNoSequence           = 0x00
	volcFlagLastPacketNoSequence = 0x02
	volcFlagAsyncFinal           = 0x04

	// serialization
	volcSerNone = 0x00
	volcSerJSON = 0x01

	// compression
	volcCompNone = 0x00
)

// volcEncodeHeader 编码 4 字节帧头
func volcEncodeHeader(msgType, flags, ser, comp uint8) []byte {
	return []byte{
		(volcVersion << 4) | (volcHeaderSize & 0x0F),
		(msgType << 4) | (flags & 0x0F),
		(ser << 4) | (comp & 0x0F),
		0x00,
	}
}

// volcEncodeMessage 编码完整消息：帧头 + 4字节payload长度 + payload
func volcEncodeMessage(msgType, flags, ser, comp uint8, payload []byte) []byte {
	header := volcEncodeHeader(msgType, flags, ser, comp)
	size := make([]byte, 4)
	binary.BigEndian.PutUint32(size, uint32(len(payload)))
	msg := make([]byte, 0, len(header)+4+len(payload))
	msg = append(msg, header...)
	msg = append(msg, size...)
	msg = append(msg, payload...)
	return msg
}

// volcBuildClientRequest 构建连接初始化 JSON payload
func volcBuildClientRequest(uid string) ([]byte, error) {
	payload := map[string]any{
		"user": map[string]any{
			"uid": uid,
		},
		"audio": map[string]any{
			"format":  "pcm",
			"codec":   "raw",
			"rate":    16000,
			"bits":    16,
			"channel": 1,
		},
		"request": map[string]any{
			"model_name":       "bigmodel",
			"enable_punc":      true,
			"enable_ddc":       true,
			"enable_nonstream": true,
			"show_utterances":  true,
			"result_type":      "full",
			"end_window_size":  3000,
		},
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("marshal client request: %w", err)
	}
	return data, nil
}

type volcServerResult struct {
	Text       string `json:"text"`
	Utterances []struct {
		Text     string `json:"text"`
		Definite bool   `json:"definite"`
	} `json:"utterances"`
}

type volcServerPayload struct {
	Result  volcServerResult `json:"result"`
	Code    int              `json:"code"`
	Message string           `json:"message"`
}

// volcDecodeResponse 解码服务端响应，返回识别文本和是否是最终结果
func volcDecodeResponse(data []byte) (text string, isFinal bool, err error) {
	if len(data) < 8 {
		return "", false, errors.New("响应数据过短")
	}

	msgType := (data[1] >> 4) & 0x0F
	flags := data[1] & 0x0F

	if msgType == volcMsgServerError {
		return "", false, errors.New("服务端错误")
	}

	// 读取 payload
	payloadSize := binary.BigEndian.Uint32(data[4:8])
	if len(data) < 8+int(payloadSize) {
		return "", false, errors.New("payload 不完整")
	}
	payload := data[8 : 8+payloadSize]

	var result volcServerPayload
	if err := json.Unmarshal(payload, &result); err != nil {
		return "", false, fmt.Errorf("解析响应失败: %w", err)
	}

	text = result.Result.Text
	isFinal = flags == volcFlagAsyncFinal
	return text, isFinal, nil
}

// TranscribeVolc 豆包/火山引擎流式 ASR
func TranscribeVolc(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error) {
	if cfg.VolcAppKey == "" || cfg.VolcAccessToken == "" {
		return "", errors.New("请配置豆包 App Key 和 Access Token")
	}

	dialer := &websocket.Dialer{
		Proxy:            websocket.DefaultDialer.Proxy,
		HandshakeTimeout: 10 * time.Second,
	}
	header := map[string][]string{
		"X-Api-App-Key":     {cfg.VolcAppKey},
		"X-Api-Access-Key":  {cfg.VolcAccessToken},
		"X-Api-Resource-Id": {cfg.VolcResourceID},
		"X-Api-Connect-Id":  {strconv.FormatInt(time.Now().UnixNano(), 10)},
	}

	conn, resp, err := dialer.Dial(volcEndpoint, header)
	if resp != nil {
		resp.Body.Close() //nolint:errcheck,gosec // WebSocket upgrade response body, close is best-effort
	}
	if err != nil {
		return "", fmt.Errorf("连接豆包失败: %w", err)
	}
	defer conn.Close() //nolint:errcheck // WebSocket close on exit, error not actionable

	// 发送初始化请求
	clientReq, err := volcBuildClientRequest(strconv.FormatInt(time.Now().UnixNano(), 10))
	if err != nil {
		return "", fmt.Errorf("构建初始化请求失败: %w", err)
	}
	initMsg := volcEncodeMessage(volcMsgFullClientRequest, volcFlagNoSequence, volcSerJSON, volcCompNone, clientReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, initMsg); err != nil {
		return "", fmt.Errorf("发送初始化请求失败: %w", err)
	}

	slog.Info("豆包连接成功")

	// 连接就绪，通知开始录音
	if ready != nil {
		close(ready)
	}

	var (
		finalText string
		mu        sync.Mutex
		recvDone  = make(chan struct{})
	)

	// 接收协程
	go func() {
		defer close(recvDone)
		defer func() {
			if r := recover(); r != nil {
				slog.Error("豆包接收协程 panic", "recover", r)
			}
		}()
		for {
			_, msg, err := conn.ReadMessage()
			if err != nil {
				return
			}
			text, isFinal, err := volcDecodeResponse(msg)
			if err != nil {
				slog.Error("豆包响应解析失败", "error", err)
				continue
			}
			if text == "" {
				continue
			}
			if isFinal {
				mu.Lock()
				finalText = text
				mu.Unlock()
				slog.Debug("豆包最终结果", "text", text)
				return
			}
			slog.Debug("豆包中间结果", "text", text)
			if onPartial != nil {
				onPartial(text)
			}
		}
	}()

	// 发送 PCM 音频块
	for chunk := range pcmCh {
		audioMsg := volcEncodeMessage(volcMsgAudioOnlyRequest, volcFlagNoSequence, volcSerNone, volcCompNone, chunk)
		if err := conn.WriteMessage(websocket.BinaryMessage, audioMsg); err != nil {
			return "", fmt.Errorf("发送音频失败: %w", err)
		}
	}

	// 发送结束帧（空 payload + lastPacket flag）
	endMsg := volcEncodeMessage(volcMsgAudioOnlyRequest, volcFlagLastPacketNoSequence, volcSerNone, volcCompNone, []byte{})
	if err := conn.WriteMessage(websocket.BinaryMessage, endMsg); err != nil {
		slog.Warn("发送结束帧失败", "error", err)
	}

	select {
	case <-recvDone:
	case <-time.After(10 * time.Second):
		slog.Warn("豆包响应超时")
	}

	mu.Lock()
	defer mu.Unlock()
	return finalText, nil
}
