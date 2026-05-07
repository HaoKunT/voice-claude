package main

import (
	"crypto/hmac"
	"crypto/sha1" //nolint:gosec // xfyun API requires HMAC-SHA1 for authentication
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/url"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

const xfyunLLMWSURL = "wss://office-api-ast-dx.iflyaisol.com/ast/communicate/v1"

func xfyunBuildAuthParams(appID, accessKeyID, accessKeySecret string) (string, error) {
	now := time.Now().In(time.FixedZone("CST", 8*3600))
	utcStr := now.Format("2006-01-02T15:04:05-0700")

	params := map[string]string{
		"accessKeyId":  accessKeyID,
		"appId":        appID,
		"uuid":         strconv.FormatInt(now.UnixNano(), 10),
		"utc":          utcStr,
		"audio_encode": "pcm_s16le",
		"lang":         "autodialect",
		"samplerate":   "16000",
	}

	keys := make([]string, 0, len(params))
	for k := range params {
		keys = append(keys, k)
	}
	slices.Sort(keys)

	parts := make([]string, 0, len(params))
	for _, k := range keys {
		parts = append(parts, url.QueryEscape(k)+"="+url.QueryEscape(params[k]))
	}
	baseStr := strings.Join(parts, "&")

	mac := hmac.New(sha1.New, []byte(accessKeySecret))
	mac.Write([]byte(baseStr))
	signature := base64.StdEncoding.EncodeToString(mac.Sum(nil))

	q := url.Values{}
	for _, k := range keys {
		q.Set(k, params[k])
	}
	q.Set("signature", signature)

	return xfyunLLMWSURL + "?" + q.Encode(), nil
}

type xfyunMsg struct {
	Action  string          `json:"action"`
	MsgType string          `json:"msg_type"`
	ResType string          `json:"res_type"`
	Desc    string          `json:"desc"`
	Data    json.RawMessage `json:"data"`
}

type xfyunResultData struct {
	SessionID string `json:"sessionId"`
	SegID     int    `json:"seg_id"`
	LS        bool   `json:"ls"`
	CN        struct {
		ST struct {
			Type string `json:"type"`
			RT   []struct {
				WS []struct {
					CW []struct {
						W  string `json:"w"`
						WP string `json:"wp"`
					} `json:"cw"`
				} `json:"ws"`
			} `json:"rt"`
		} `json:"st"`
	} `json:"cn"`
}

type xfyunErrorData struct {
	Desc   string `json:"desc"`
	Detail struct {
		Domain string `json:"domain"`
	} `json:"detail"`
}

// TranscribeXfyun 录完整段后识别（批量模式）
func TranscribeXfyun(pcmData []byte, cfg *Config) (string, error) {
	ch := make(chan []byte, 1)
	ch <- pcmData
	close(ch)
	return xfyunStream(cfg, ch, nil, nil)
}

// TranscribeXfyunStream 实时流式识别：从 pcmCh 读取录音块边录边识别，
// onPartial 每次收到中间结果时回调（可为 nil）。
// ready 在 WebSocket 连接建立成功后关闭，调用方应等待 ready 再开始推送 PCM。
// 调用方关闭 pcmCh 表示录音结束，函数返回完整识别文本。
func TranscribeXfyunStream(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error) {
	return xfyunStream(cfg, pcmCh, onPartial, ready)
}

func xfyunStream(cfg *Config, pcmCh <-chan []byte, onPartial func(string), ready chan<- struct{}) (string, error) { //nolint:gocyclo // WebSocket state machine with concurrent send/receive goroutines requires complex dispatch logic
	if cfg.XfyunAppID == "" || cfg.XfyunAccessKeyID == "" || cfg.XfyunAccessSecret == "" {
		return "", errors.New("请配置讯飞 AppID、AccessKeyID、AccessKeySecret")
	}

	wsURL, err := xfyunBuildAuthParams(cfg.XfyunAppID, cfg.XfyunAccessKeyID, cfg.XfyunAccessSecret)
	if err != nil {
		return "", fmt.Errorf("生成鉴权参数失败: %w", err)
	}

	dialer := &websocket.Dialer{
		Proxy:            websocket.DefaultDialer.Proxy,
		HandshakeTimeout: 10 * time.Second,
	}
	conn, wsResp, err := dialer.Dial(wsURL, nil)
	if wsResp != nil {
		wsResp.Body.Close() //nolint:errcheck,gosec // WebSocket upgrade response body, close is best-effort
	}
	if err != nil {
		return "", fmt.Errorf("连接讯飞失败: %w", err)
	}
	defer conn.Close() //nolint:errcheck // WebSocket close on exit, error not actionable

	slog.Info("讯飞连接成功")

	// 通知调用方连接已就绪，可以开始推送 PCM
	if ready != nil {
		close(ready)
	}

	var (
		finals    = []string{}
		sessionID string
		mu        sync.Mutex
		recvDone  = make(chan struct{})
	)

	// 接收协程
	go func() {
		defer close(recvDone)
		defer func() {
			if r := recover(); r != nil {
				slog.Error("讯飞接收协程 panic", "recover", r)
			}
		}()
		for {
			_, msg, err := conn.ReadMessage()
			if err != nil {
				return
			}

			var m xfyunMsg
			if err := json.Unmarshal(msg, &m); err != nil {
				continue
			}

			if m.Action != "" {
				switch m.Action {
				case "started":
					continue
				case "error":
					slog.Error("讯飞错误", "desc", m.Desc)
					return
				}
			}

			if m.MsgType == "result" && m.ResType == "asr" {
				var data xfyunResultData
				if err := json.Unmarshal(m.Data, &data); err != nil {
					continue
				}
				if data.SessionID != "" {
					mu.Lock()
					sessionID = data.SessionID
					mu.Unlock()
				}

				text := extractXfyunText(&data)
				if text == "" {
					if data.LS {
						return
					}
					continue
				}

				if data.CN.ST.Type == "0" {
					// 最终结果：追加到 finals
					mu.Lock()
					finals = append(finals, text)
					mu.Unlock()
					slog.Debug("讯飞最终结果", "text", text)
				} else {
					// 中间结果：回调通知
					slog.Debug("讯飞中间结果", "text", text)
					if onPartial != nil {
						onPartial(text)
					}
				}

				if data.LS {
					return
				}
			}

			if m.MsgType == "result" && m.ResType == "frc" {
				var errData xfyunErrorData
				if err := json.Unmarshal(m.Data, &errData); err == nil {
					slog.Error("讯飞识别错误", "desc", errData.Desc)
				}
				return
			}
		}
	}()

	// 发送协程：从 channel 读取 PCM 块，每 40ms 发送 1280 字节
	const chunkSize = 1280
	buf := make([]byte, 0, chunkSize*2)

	for chunk := range pcmCh {
		buf = append(buf, chunk...)
		for len(buf) >= chunkSize {
			if err := conn.WriteMessage(websocket.BinaryMessage, buf[:chunkSize]); err != nil {
				return "", fmt.Errorf("发送音频失败: %w", err)
			}
			buf = buf[chunkSize:]
			time.Sleep(40 * time.Millisecond)
		}
	}

	// 发送剩余不足一帧的数据
	if len(buf) > 0 {
		if err := conn.WriteMessage(websocket.BinaryMessage, buf); err != nil {
			slog.Warn("发送尾帧失败", "error", err)
		}
	}

	// 发送结束标记
	mu.Lock()
	sid := sessionID
	mu.Unlock()

	endMsg := map[string]any{"end": true}
	if sid != "" {
		endMsg["sessionId"] = sid
	}
	if endBytes, err := json.Marshal(endMsg); err == nil {
		if err := conn.WriteMessage(websocket.TextMessage, endBytes); err != nil {
			slog.Warn("发送结束标记失败", "error", err)
		}
	}

	select {
	case <-recvDone:
	case <-time.After(5 * time.Second):
	}

	mu.Lock()
	defer mu.Unlock()
	return joinStrings(finals), nil
}

func extractXfyunText(data *xfyunResultData) string {
	var b strings.Builder
	for _, rt := range data.CN.ST.RT {
		for _, ws := range rt.WS {
			for _, cw := range ws.CW {
				b.WriteString(cw.W)
			}
		}
	}
	return b.String()
}

func joinStrings(ss []string) string {
	var b strings.Builder
	for _, s := range ss {
		b.WriteString(s)
	}
	return b.String()
}
