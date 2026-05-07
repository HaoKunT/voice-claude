"""智谱 GLM-ASR-2512 语音识别后端"""

import os
import wave
import tempfile

import numpy as np
import requests

ZHIPU_API_KEY = os.environ.get("ZHIPU_API_KEY", "")
API_URL = "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions"
MAX_DURATION = 30  # 智谱 API 限制最长 30 秒


def _transcribe_chunk(wav_path: str) -> str:
    """调用智谱 ASR API 识别单个音频片段"""
    with open(wav_path, "rb") as f:
        resp = requests.post(
            API_URL,
            headers={"Authorization": f"Bearer {ZHIPU_API_KEY}"},
            files={"file": ("audio.wav", f, "audio/wav")},
            data={"model": "glm-asr-2512"},
            timeout=60,
        )
    resp.raise_for_status()
    return resp.json()["text"]


def _split_wav(wav_path: str, max_seconds: int = MAX_DURATION) -> list[str]:
    """将长音频按 max_seconds 切分为多个 WAV 文件"""
    with wave.open(wav_path, "rb") as wf:
        sr = wf.getframerate()
        sw = wf.getsampwidth()
        ch = wf.getnchannels()
        total_frames = wf.getnframes()
        duration = total_frames / sr

        if duration <= max_seconds:
            return [wav_path]

        chunk_frames = int(max_seconds * sr)
        paths = []
        for start in range(0, total_frames, chunk_frames):
            wf.setpos(start)
            frames = wf.readframes(min(chunk_frames, total_frames - start))
            path = tempfile.mktemp(suffix=".wav")
            with wave.open(path, "wb") as out:
                out.setnchannels(ch)
                out.setsampwidth(sw)
                out.setframerate(sr)
                out.writeframes(frames)
            paths.append(path)

    print(f"📎 音频 {duration:.1f}s 超过 {max_seconds}s 限制，切分为 {len(paths)} 段")
    return paths


def transcribe(wav_path: str) -> str:
    """调用智谱 ASR API 识别音频，自动处理超长音频"""
    if not ZHIPU_API_KEY:
        raise RuntimeError("请设置 ZHIPU_API_KEY 环境变量")

    chunks = _split_wav(wav_path)
    results = []
    for i, chunk_path in enumerate(chunks):
        if len(chunks) > 1:
            print(f"  转写第 {i+1}/{len(chunks)} 段...")
        text = _transcribe_chunk(chunk_path)
        if chunk_path != wav_path:
            os.unlink(chunk_path)
        results.append(text)

    return "".join(results)
