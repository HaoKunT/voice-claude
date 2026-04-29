"""
语音控制 Claude Code 客户端（Mac）
录音 → 增益放大 → 远程 MiMo ASR 识别 → 送入 Claude Code
"""

import os
import sys
import wave
import tempfile
import subprocess

import numpy as np
import sounddevice as sd
import requests

ASR_SERVER = os.environ.get("ASR_SERVER", "http://localhost:8900")
SAMPLE_RATE = 16000
GAIN = 10  # 气声增益倍数


def record(seconds: int = 10) -> str:
    """录音并保存为临时 wav 文件"""
    print(f"🎤 录音中（最多 {seconds} 秒，按 Ctrl+C 提前结束）...")
    try:
        audio = sd.rec(
            int(seconds * SAMPLE_RATE),
            samplerate=SAMPLE_RATE,
            channels=1,
            dtype="float32",
        )
        sd.wait()
    except KeyboardInterrupt:
        sd.stop()
        print("⏹ 提前结束录音")

    # 去掉末尾静音
    audio = audio.flatten()
    threshold = 0.01
    non_silent = np.where(np.abs(audio) > threshold)[0]
    if len(non_silent) > 0:
        audio = audio[: non_silent[-1] + 1]

    # 增益放大（适配气声）
    audio = audio * GAIN
    audio = np.clip(audio, -1.0, 1.0)

    # 保存 wav
    path = tempfile.mktemp(suffix=".wav")
    with wave.open(path, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(SAMPLE_RATE)
        wf.writeframes((audio * 32767).astype(np.int16).tobytes())
    return path


def transcribe(wav_path: str) -> str:
    """调用远程 MiMo ASR 服务"""
    with open(wav_path, "rb") as f:
        resp = requests.post(
            f"{ASR_SERVER}/transcribe",
            files={"audio": ("audio.wav", f, "audio/wav")},
            timeout=30,
        )
    resp.raise_for_status()
    return resp.json()["text"]


def send_to_claude(text: str):
    """通过管道将文字送入 claude CLI"""
    print(f"📝 识别结果: {text}")
    print("🚀 发送到 Claude Code...")
    subprocess.run(["claude"], input=text.encode())


def check_server():
    """检查服务端是否可达"""
    try:
        resp = requests.get(f"{ASR_SERVER}/health", timeout=5)
        resp.raise_for_status()
        print(f"✅ 服务端连接成功: {ASR_SERVER}")
        return True
    except Exception as e:
        print(f"❌ 无法连接服务端 {ASR_SERVER}: {e}")
        return False


def main():
    if not check_server():
        print("请确保服务端已启动，或设置 ASR_SERVER 环境变量")
        sys.exit(1)

    print("\n语音控制 Claude Code")
    print("=" * 40)
    print("按回车开始录音，气声说话即可")
    print("输入 q 退出\n")

    while True:
        try:
            cmd = input(">>> 按回车录音（q 退出）: ").strip()
            if cmd.lower() == "q":
                break

            wav = record()
            text = transcribe(wav)
            os.unlink(wav)

            if text.strip():
                send_to_claude(text)
            else:
                print("（未识别到内容）")

        except KeyboardInterrupt:
            print("\n退出")
            break


if __name__ == "__main__":
    main()
