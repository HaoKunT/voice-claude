"""将文字发送到 claude CLI"""

import subprocess


def send_to_claude(text: str):
    """通过管道将文字送入 claude CLI"""
    print(f"📝 识别结果: {text}")
    print("🚀 发送到 Claude Code...")
    subprocess.run(["claude"], input=text.encode())
