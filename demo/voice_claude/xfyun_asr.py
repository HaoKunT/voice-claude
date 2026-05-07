"""讯飞实时语音转写大模型 - WebSocket 流式后端"""

import os
import json
import uuid
import hmac
import hashlib
import base64
import threading
import time
from urllib.parse import urlencode, quote
from datetime import datetime, timezone, timedelta

from websocket import create_connection, WebSocketException

from voice_claude.audio import audio_stream_pcm, read_wav_pcm

APP_ID = os.environ.get("XFYUN_APP_ID", "")
ACCESS_KEY_ID = os.environ.get("XFYUN_ACCESS_KEY_ID", "")
ACCESS_KEY_SECRET = os.environ.get("XFYUN_ACCESS_KEY_SECRET", "")
BASE_WS_URL = "wss://office-api-ast-dx.iflyaisol.com/ast/communicate/v1"


def _check_env():
    if not all([APP_ID, ACCESS_KEY_ID, ACCESS_KEY_SECRET]):
        raise RuntimeError(
            "请设置环境变量：XFYUN_APP_ID, XFYUN_ACCESS_KEY_ID, XFYUN_ACCESS_KEY_SECRET"
        )


def _build_auth_params() -> dict:
    """生成鉴权参数"""
    now = datetime.now(timezone(timedelta(hours=8)))
    utc_str = now.strftime("%Y-%m-%dT%H:%M:%S%z")

    params = {
        "accessKeyId": ACCESS_KEY_ID,
        "appId": APP_ID,
        "uuid": uuid.uuid4().hex,
        "utc": utc_str,
        "audio_encode": "pcm_s16le",
        "lang": "autodialect",
        "samplerate": "16000",
    }

    # 过滤空值 → 字典序排序 → URL编码 → 拼接基础字符串
    sorted_params = sorted(
        [(k, v) for k, v in params.items() if v is not None and str(v).strip() != ""]
    )
    base_str = "&".join(
        f"{quote(k, safe='')}={quote(v, safe='')}" for k, v in sorted_params
    )

    # HMAC-SHA1 + Base64
    signature = hmac.new(
        ACCESS_KEY_SECRET.encode("utf-8"),
        base_str.encode("utf-8"),
        hashlib.sha1,
    ).digest()
    params["signature"] = base64.b64encode(signature).decode("utf-8")
    return params


def _ws_transcribe(pcm_chunks) -> str:
    """通过 WebSocket 发送 PCM 音频块并收集识别结果"""
    auth_params = _build_auth_params()
    ws_url = f"{BASE_WS_URL}?{urlencode(auth_params)}"

    ws = create_connection(ws_url, timeout=15, enable_multithread=True)
    print("✅ 讯飞连接成功")

    results = []
    session_id = None
    lock = threading.Lock()
    recv_done = threading.Event()

    def recv_msg():
        nonlocal session_id
        while not recv_done.is_set():
            try:
                msg = ws.recv()
                if not msg:
                    break
                if isinstance(msg, str):
                    data = json.loads(msg)
                    action = data.get("action")

                    if action == "started":
                        continue

                    if action == "error":
                        print(f"❌ 讯飞错误: {data.get('desc', '未知错误')}")
                        break

                    if action == "result":
                        # 获取 sessionId
                        sid = data.get("data", {}).get("sessionId")
                        if sid:
                            session_id = sid

                        cn = data.get("data", {}).get("cn", {})
                        st = cn.get("st", {})
                        result_type = st.get("type", "1")

                        text = ""
                        for rt_item in st.get("rt", []):
                            for ws_item in rt_item.get("ws", []):
                                for cw_item in ws_item.get("cw", []):
                                    text += cw_item.get("w", "")

                        if not text:
                            continue

                        if result_type == "0":
                            with lock:
                                results.append(text)
                            print(f"  {text}")
                        else:
                            print(f"  {text}\r", end="", flush=True)
            except Exception:
                break
        recv_done.set()

    recv_thread = threading.Thread(target=recv_msg, daemon=True)
    recv_thread.start()

    # 发送音频
    print("🎤 开始发送音频...")
    try:
        for chunk in pcm_chunks:
            ws.send_binary(chunk)
            time.sleep(0.04)  # 40ms 间隔
    except KeyboardInterrupt:
        print("\n⏹ 停止发送")

    # 发送结束标记
    end_msg = {"end": True}
    if session_id:
        end_msg["sessionId"] = session_id
    ws.send(json.dumps(end_msg))
    print("📤 已发送结束标记")

    # 等待最后的结果
    recv_done.wait(timeout=5)
    ws.close()

    return "".join(results)


def stream_transcribe() -> str:
    """流式转写：从麦克风录音，实时识别"""
    _check_env()
    print("🎤 开始录音，按 Ctrl+C 停止...")
    with audio_stream_pcm(chunk_ms=40) as pcm_chunks:
        return _ws_transcribe(pcm_chunks())


def stream_transcribe_file(wav_path: str) -> str:
    """从 WAV 文件流式转写（测试用）"""
    _check_env()
    print(f"📂 从文件转写: {wav_path}")
    chunks = read_wav_pcm(wav_path, chunk_ms=40)
    return _ws_transcribe(chunks)
