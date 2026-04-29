"""
MiMo-V2.5-ASR 语音识别服务端
运行环境：Linux + CUDA >= 12.0 + Python 3.12
"""

import os
import tempfile

from fastapi import FastAPI, UploadFile, File
from fastapi.responses import JSONResponse
import uvicorn

from src.mimo_audio.mimo_audio import MimoAudio

MODEL_PATH = os.environ.get("MODEL_PATH", "./models/MiMo-V2.5-ASR")
TOKENIZER_PATH = os.environ.get("TOKENIZER_PATH", "./models/MiMo-Audio-Tokenizer")
PORT = int(os.environ.get("PORT", "8900"))

app = FastAPI(title="MiMo-V2.5-ASR Service")
model: MimoAudio | None = None


@app.on_event("startup")
def load_model():
    global model
    print(f"Loading model from {MODEL_PATH} ...")
    model = MimoAudio(model_path=MODEL_PATH, tokenizer_path=TOKENIZER_PATH)
    print("Model loaded.")


@app.post("/transcribe")
async def transcribe(
    audio: UploadFile = File(...),
    language: str = "auto",  # auto / chinese / english
):
    """接收音频文件，返回识别文字"""
    suffix = os.path.splitext(audio.filename or "audio.wav")[1]
    with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as f:
        f.write(await audio.read())
        tmp_path = f.name

    try:
        tag_map = {"chinese": "<chinese>", "english": "<english>"}
        audio_tag = tag_map.get(language)
        if audio_tag:
            text = model.asr_sft(tmp_path, audio_tag=audio_tag)
        else:
            text = model.asr_sft(tmp_path)
        return {"text": text}
    finally:
        os.unlink(tmp_path)


@app.get("/health")
def health():
    return {"status": "ok", "model": MODEL_PATH}


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=PORT)
