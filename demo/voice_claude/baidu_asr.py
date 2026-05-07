"""百度实时语音识别 - WebSocket 流式后端"""

import os
import json
import uuid
import threading
import time

import websocket

from voice_claude.audio import audio_stream_pcm, read_wav_pcm

APP_ID = int(os.environ.get("BAIDU_APP_ID", "0"))
APP_KEY = os.environ.get("BAIDU_APP_KEY", "")
DEV_PID = int(os.environ.get("BAIDU_DEV_PID", "15372"))  # 15372=普通话+强标点


def _check_env():
    if not APP_ID or not APP_KEY:
        raise RuntimeError("请设置环境变量：BAIDU_APP_ID, BAIDU_APP_KEY")


def _ws_transcribe(pcm_chunks) -> str:
    """通过 WebSocket 发送 PCM 音频块并收集识别结果"""
    sn = str(uuid.uuid4())
    ws_url = f"wss://vop.baidu.com/realtime_asr?sn={sn}"

    results = []
    done_event = threading.Event()
    lock = threading.Lock()

    def on_message(ws, message):
        data = json.loads(message)
        err_no = data.get("err_no", 0)
        msg_type = data.get("type", "")

        if err_no != 0:
            print(f"❌ 百度错误 [{err_no}]: {data.get('err_msg', '')}")
            return

        if msg_type == "MID_TEXT":
            print(f"  {data.get('result', '')}\r", end="", flush=True)
        elif msg_type == "FIN_TEXT":
            text = data.get("result", "")
            if text:
                with lock:
                    results.append(text)
                print(f"  {text}")

    def on_error(ws, error):
        print(f"❌ WebSocket 错误: {error}")

    def on_close(ws, close_status_code, close_msg):
        done_event.set()

    def on_open(ws):
        start_frame = {
            "type": "START",
            "data": {
                "appid": APP_ID,
                "appkey": APP_KEY,
                "dev_pid": DEV_PID,
                "cuid": "voice-claude",
                "format": "pcm",
                "sample": 16000,
            },
        }
        ws.send(json.dumps(start_frame))

    ws = websocket.WebSocketApp(
        ws_url,
        on_open=on_open,
        on_message=on_message,
        on_error=on_error,
        on_close=on_close,
    )

    ws_thread = threading.Thread(target=ws.run_forever, daemon=True)
    ws_thread.start()
    time.sleep(1)

    try:
        for chunk in pcm_chunks:
            if done_event.is_set():
                break
            try:
                ws.send(chunk, opcode=websocket.ABNF.OPCODE_BINARY)
            except Exception:
                break
    except KeyboardInterrupt:
        print("\n⏹ 停止录音")

    try:
        ws.send(json.dumps({"type": "FINISH"}))
    except Exception:
        pass

    done_event.wait(timeout=3)
    ws.close()

    return "".join(results)


def stream_transcribe() -> str:
    """流式转写：从麦克风录音，实时识别"""
    _check_env()
    print("🎤 开始录音，按 Ctrl+C 停止...")
    with audio_stream_pcm(chunk_ms=160) as pcm_chunks:
        return _ws_transcribe(pcm_chunks())


def stream_transcribe_file(wav_path: str) -> str:
    """从 WAV 文件流式转写（测试用）"""
    _check_env()
    print(f"📂 从文件转写: {wav_path}")
    chunks = read_wav_pcm(wav_path, chunk_ms=160)
    return _ws_transcribe(chunks)
