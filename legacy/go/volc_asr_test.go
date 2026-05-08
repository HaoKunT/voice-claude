package main

import (
	"encoding/binary"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestVolcEncodeHeader(t *testing.T) {
	t.Parallel()
	hdr := volcEncodeHeader(volcMsgFullClientRequest, volcFlagNoSequence, volcSerJSON, volcCompNone)
	require.Len(t, hdr, 4)
	assert.Equal(t, uint8((volcVersion<<4)|volcHeaderSize), hdr[0])
	assert.Equal(t, uint8((volcMsgFullClientRequest<<4)|volcFlagNoSequence), hdr[1])
	assert.Equal(t, uint8((volcSerJSON<<4)|volcCompNone), hdr[2])
	assert.Equal(t, uint8(0x00), hdr[3])
}

func TestVolcEncodeMessage(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name        string
		payload     []byte
		wantMsgLen  int
		wantPayload []byte
	}{
		{
			name:        "普通 payload",
			payload:     []byte(`{"key":"value"}`),
			wantMsgLen:  4 + 4 + len(`{"key":"value"}`),
			wantPayload: []byte(`{"key":"value"}`),
		},
		{
			name:       "空 payload",
			payload:    []byte{},
			wantMsgLen: 8,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			msg := volcEncodeMessage(volcMsgFullClientRequest, volcFlagNoSequence, volcSerJSON, volcCompNone, tt.payload)
			require.Len(t, msg, tt.wantMsgLen)
			size := binary.BigEndian.Uint32(msg[4:8])
			assert.Equal(t, uint32(len(tt.payload)), size)
			if len(tt.payload) > 0 {
				assert.Equal(t, tt.payload, msg[8:])
			}
		})
	}
}

func TestVolcBuildClientRequest(t *testing.T) {
	t.Parallel()
	data, err := volcBuildClientRequest("test-uid")
	require.NoError(t, err)

	var payload map[string]any
	require.NoError(t, json.Unmarshal(data, &payload))

	user, ok := payload["user"].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, "test-uid", user["uid"])

	audio, ok := payload["audio"].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, "pcm", audio["format"])
}

func TestVolcDecodeResponse(t *testing.T) {
	t.Parallel()

	makeFrame := func(msgType, flags uint8, payloadBytes []byte) []byte {
		msg := make([]byte, 8+len(payloadBytes))
		msg[1] = (msgType << 4) | flags
		binary.BigEndian.PutUint32(msg[4:8], uint32(len(payloadBytes)))
		copy(msg[8:], payloadBytes)
		return msg
	}

	finalPayload, _ := json.Marshal(volcServerPayload{Result: volcServerResult{Text: "你好世界"}})
	midPayload, _ := json.Marshal(volcServerPayload{Result: volcServerResult{Text: "中间结果"}})

	tests := []struct {
		name        string
		data        []byte
		wantText    string
		wantFinal   bool
		wantErr     bool
		errContains string
	}{
		{
			name:        "数据过短",
			data:        []byte{0x01, 0x02, 0x03},
			wantErr:     true,
			errContains: "过短",
		},
		{
			name:        "服务端错误帧",
			data:        makeFrame(volcMsgServerError, 0x00, nil),
			wantErr:     true,
			errContains: "服务端错误",
		},
		{
			name:      "最终结果",
			data:      makeFrame(volcMsgServerResponse, volcFlagAsyncFinal, finalPayload),
			wantText:  "你好世界",
			wantFinal: true,
		},
		{
			name:      "中间结果",
			data:      makeFrame(volcMsgServerResponse, volcFlagNoSequence, midPayload),
			wantText:  "中间结果",
			wantFinal: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			text, isFinal, err := volcDecodeResponse(tt.data)
			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errContains)
				return
			}
			require.NoError(t, err)
			assert.Equal(t, tt.wantText, text)
			assert.Equal(t, tt.wantFinal, isFinal)
		})
	}
}
