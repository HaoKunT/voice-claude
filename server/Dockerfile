FROM pytorch/pytorch:2.6.0-cuda12.4-cudnn9-devel

WORKDIR /app

RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*

RUN git clone https://github.com/XiaomiMiMo/MiMo-V2.5-ASR.git .

COPY requirements.txt .
RUN pip install -r requirements.txt

# 模型通过 volume 挂载到 /app/models/
ENV MODEL_PATH=/app/models/MiMo-V2.5-ASR
ENV TOKENIZER_PATH=/app/models/MiMo-Audio-Tokenizer
ENV PORT=8900

EXPOSE 8900

CMD ["python", "mimo_asr_server.py"]
