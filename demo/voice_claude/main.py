"""语音控制 Claude Code - 统一入口"""

import os
import argparse
from pathlib import Path

from dotenv import load_dotenv

load_dotenv(Path(__file__).resolve().parent.parent / ".env")

from voice_claude.claude import send_to_claude

BACKENDS = {
    "xfyun": "voice_claude.xfyun_asr",
    "baidu": "voice_claude.baidu_asr",
    "zhipu": "voice_claude.zhipu_asr",
}


def load_backend(name: str):
    """动态加载后端模块"""
    import importlib

    if name not in BACKENDS:
        print(f"❌ 未知后端: {name}，可选: {', '.join(BACKENDS)}")
        raise SystemExit(1)

    return importlib.import_module(BACKENDS[name])


def main():
    parser = argparse.ArgumentParser(description="语音控制 Claude Code")
    parser.add_argument(
        "--backend",
        choices=list(BACKENDS.keys()),
        default="xfyun",
        help="语音识别后端 (默认: xfyun)",
    )
    parser.add_argument(
        "--seconds",
        type=int,
        default=None,
        help="最长录音秒数，仅 zhipu 后端有效（默认无限制，Ctrl+C 停止）",
    )
    parser.add_argument(
        "--audio-file",
        help="用 WAV 文件代替麦克风录音（测试用）",
    )
    args = parser.parse_args()

    backend = load_backend(args.backend)
    is_streaming = hasattr(backend, "stream_transcribe")

    # 文件测试模式
    if args.audio_file:
        from voice_claude.audio import to_wav

        print(f"\n测试模式（后端: {args.backend}，文件: {args.audio_file}）")
        print("=" * 40)
        wav_path = to_wav(args.audio_file)
        if is_streaming:
            text = backend.stream_transcribe_file(wav_path)
        else:
            text = backend.transcribe(wav_path)
        if text.strip():
            print(f"\n✅ 识别结果: {text}")
        else:
            print("（未识别到内容）")
        return

    # 正常录音模式
    print(f"\n语音控制 Claude Code（后端: {args.backend}）")
    print("=" * 40)

    if is_streaming:
        print("按回车开始录音，Ctrl+C 停止\n")
        while True:
            try:
                cmd = input(">>> 按回车录音（q 退出）: ").strip()
                if cmd.lower() == "q":
                    break

                text = backend.stream_transcribe()

                if text.strip():
                    send_to_claude(text)
                else:
                    print("（未识别到内容）")
            except KeyboardInterrupt:
                print("\n退出")
                break
    else:
        # 批处理模式（zhipu）
        from voice_claude.audio import record

        print("按回车开始录音，气声说话即可")
        print("输入 q 退出\n")
        while True:
            try:
                cmd = input(">>> 按回车录音（q 退出）: ").strip()
                if cmd.lower() == "q":
                    break

                wav = record(seconds=args.seconds)
                text = backend.transcribe(wav)
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
