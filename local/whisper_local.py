"""
语音控制 Claude Code - 本地 Whisper 方案（Mac）
录音 → 增益放大 → Whisper 本地识别 → 送入 Claude Code
"""

import os
import sys
import wave
import tempfile
import subprocess

import numpy as np
import sounddevice as sd
from faster_whisper import WhisperModel

SAMPLE_RATE = 16000
GAIN = 10  # 气声增益倍数
MODEL_SIZE = os.environ.get("WHISPER_MODEL", "large-v3")


def load_model() -> WhisperModel:
    """加载 Whisper 模型"""
    print(f"📦 加载 Whisper {MODEL_SIZE} 模型...")
    model = WhisperModel(MODEL_SIZE, device="cpu", compute_type="int8")
    print("✅ 模型加载完成")
    return model


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


def transcribe(model: WhisperModel, wav_path: str) -> str:
    """Whisper 本地识别"""
    segments, _ = model.transcribe(
        wav_path,
        language="zh",
        beam_size=5,
        vad_filter=True,
    )
    return "".join(seg.text for seg in segments)


def send_to_claude(text: str):
    """通过管道将文字送入 claude CLI"""
    print(f"📝 识别结果: {text}")
    print("🚀 发送到 Claude Code...")
    subprocess.run(["claude"], input=text.encode())


def main():
    model = load_model()

    print("\n语音控制 Claude Code（本地 Whisper）")
    print("=" * 40)
    print("按回车开始录音，气声说话即可")
    print("输入 q 退出\n")

    while True:
        try:
            cmd = input(">>> 按回车录音（q 退出）: ").strip()
            if cmd.lower() == "q":
                break

            wav = record()
            text = transcribe(model, wav)
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
