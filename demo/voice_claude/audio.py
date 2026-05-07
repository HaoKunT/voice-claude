"""录音、增益、保存 wav 的公共逻辑"""

import wave
import tempfile
import shutil
import subprocess
import queue
from contextlib import contextmanager
from pathlib import Path

import numpy as np
import sounddevice as sd

SAMPLE_RATE = 16000
GAIN = 10  # 气声增益倍数


def to_wav(audio_path: str) -> str:
    """将任意音频文件转为 16kHz 16bit 单声道 WAV，返回 WAV 文件路径"""
    path = Path(audio_path)

    # 检查是否已经是目标格式的 WAV
    if path.suffix.lower() == ".wav":
        try:
            with wave.open(str(path), "rb") as wf:
                if (
                    wf.getsampwidth() == 2
                    and wf.getframerate() == SAMPLE_RATE
                    and wf.getnchannels() == 1
                ):
                    return str(path)
        except Exception:
            pass

    # 用 ffmpeg 转换
    if not shutil.which("ffmpeg"):
        raise RuntimeError("需要 ffmpeg 来转换音频格式，请先安装: brew install ffmpeg")

    out_path = tempfile.mktemp(suffix=".wav")
    subprocess.run(
        [
            "ffmpeg", "-y", "-i", str(path),
            "-ar", str(SAMPLE_RATE),
            "-ac", "1",
            "-sample_fmt", "s16",
            out_path,
        ],
        check=True,
        capture_output=True,
    )
    print(f"🔄 已转换: {path.name} → 16kHz 16bit 单声道 WAV")
    return out_path


def record(seconds: int = None) -> str:
    """录音并保存为临时 wav 文件，返回文件路径（批处理模式，供 zhipu 使用）

    按 Ctrl+C 停止录音。seconds 为可选最长录音秒数，默认无限制。
    """
    import time
    import threading

    if seconds:
        print(f"🎤 录音中（最多 {seconds} 秒，按 Ctrl+C 提前结束）...")
    else:
        print("🎤 录音中（按 Ctrl+C 停止）...")

    frames = []

    def callback(indata, frame_count, time_info, status):
        frames.append(indata[:, 0].copy())

    with sd.InputStream(
        samplerate=SAMPLE_RATE,
        channels=1,
        dtype="float32",
        callback=callback,
    ):
        try:
            start = time.time()
            while True:
                if seconds and (time.time() - start) >= seconds:
                    break
                time.sleep(0.05)
        except KeyboardInterrupt:
            print("⏹ 停止录音")

    audio = np.concatenate(frames) if frames else np.zeros(0, dtype=np.float32)

    # 去掉末尾静音
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


@contextmanager
def audio_stream_pcm(chunk_ms: int = 40):
    """流式录音上下文管理器，yield PCM 16bit 音频块（流式后端使用）

    chunk_ms: 每块时长（毫秒），讯飞建议 40ms，百度建议 160ms
    """
    q: queue.Queue[bytes] = queue.Queue()
    chunk_samples = int(SAMPLE_RATE * chunk_ms / 1000)
    gain = GAIN

    def callback(indata, frames, time_info, status):
        audio = indata[:, 0].copy()
        audio = audio * gain
        audio = np.clip(audio, -1.0, 1.0)
        pcm = (audio * 32767).astype(np.int16).tobytes()
        q.put(pcm)

    stream = sd.InputStream(
        samplerate=SAMPLE_RATE,
        channels=1,
        dtype="float32",
        blocksize=chunk_samples,
        callback=callback,
    )

    def chunks():
        with stream:
            while True:
                try:
                    yield q.get(timeout=0.5)
                except queue.Empty:
                    continue

    yield chunks


def read_wav_pcm(wav_path: str, chunk_ms: int = 40) -> list[bytes]:
    """读取 WAV 文件，返回 PCM 16bit 分块列表"""
    with wave.open(wav_path, "rb") as wf:
        assert wf.getsampwidth() == 2, "需要 16bit WAV"
        assert wf.getframerate() == SAMPLE_RATE, f"需要 {SAMPLE_RATE}Hz WAV"
        raw = wf.readframes(wf.getnframes())

    chunk_bytes = int(SAMPLE_RATE * chunk_ms / 1000) * 2  # 16bit = 2 bytes/sample
    chunks = []
    for i in range(0, len(raw), chunk_bytes):
        chunk = raw[i : i + chunk_bytes]
        if len(chunk) > 0:
            chunks.append(chunk)
    return chunks
