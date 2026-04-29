# voice-claude

用语音（气声/悄悄话）控制 Claude Code，在办公室不打扰同事。

## 架构

```
气声说话 → 麦杆耳麦录音 → 增益放大 → STT 识别 → Claude Code
```

两种 STT 方案：

| 方案 | 模型 | 运行位置 | 中文 WER | 适合场景 |
|------|------|----------|----------|----------|
| **远程 MiMo** | MiMo-V2.5-ASR (8B) | Linux GPU 服务器 | 2.52 | 追求最高准确率 |
| **本地 Whisper** | Whisper large-v3 | Mac 本地 | ~7.4 | 不想搭服务器 |

## 硬件要求

- **耳麦**：带麦杆的有线 USB 耳麦，麦杆贴近嘴角 2-3cm
- 推荐：EPOS IMPACT 400 / Jabra Evolve2 40 / 缤特力 Blackwire 5220
- **不要用 AirPods**，麦克风离嘴太远，气声收不到

## 快速开始

### 方案一：本地 Whisper（Mac 直接用）

```bash
cd local
pip install -r requirements.txt
python whisper_local.py
```

按回车开始录音，气声说话，文字自动送入 Claude Code。

### 方案二：远程 MiMo（需要 Linux GPU 服务器）

**服务端（Linux + CUDA >= 12.0）：**

```bash
cd server
pip install -r requirements.txt

# 下载模型
huggingface-cli download XiaomiMiMo/MiMo-Audio-Tokenizer --local-dir ./models/MiMo-Audio-Tokenizer
huggingface-cli download XiaomiMiMo/MiMo-V2.5-ASR --local-dir ./models/MiMo-V2.5-ASR

# 启动服务
python mimo_asr_server.py
```

**客户端（Mac）：**

```bash
cd client
pip install -r requirements.txt

# 设置服务器地址
export ASR_SERVER="http://你的服务器IP:8900"

python voice_to_claude.py
```

## 气声识别技巧

1. 麦杆贴近嘴角 2-3cm
2. 不用振动声带，只送气（像说悄悄话）
3. 说话速度适中，不要过快
4. 录音时增益会自动放大 10 倍，不需要大声

## 项目结构

```
voice-claude/
├── client/           # Mac 客户端（录音 + 远程调用）
│   ├── voice_to_claude.py
│   └── requirements.txt
├── server/           # Linux GPU 服务端（MiMo ASR）
│   ├── mimo_asr_server.py
│   ├── Dockerfile
│   └── requirements.txt
└── local/            # Mac 本地方案（Whisper）
    ├── whisper_local.py
    └── requirements.txt
```
