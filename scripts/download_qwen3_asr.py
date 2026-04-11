#!/usr/bin/env python3
"""
下载 Qwen3-ASR-0.6B ONNX 模型
模型来源: https://huggingface.co/wolfofbackstreet/Qwen3-ASR-0.6B-ONNX-CPU
"""

import os
import sys
import urllib.request
import urllib.error
from pathlib import Path

# 模型文件配置 - 使用多个镜像源
MODEL_REPOS = [
    "https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU/resolve/main",
    "https://hf-mirror.com/Daumee/Qwen3-ASR-0.6B-ONNX-CPU/resolve/main",
]

# 模型文件列表 (路径 -> 本地文件名)
MODEL_FILES = {
    "onnx_models/encoder_transformer.onnx": "model.onnx",
    "onnx_models/decoder_step.int8.onnx": "decoder.onnx",
}

# 目标目录
DEFAULT_MODEL_DIR = Path(__file__).parent.parent / "speakplain" / "models" / "qwen3-asr"


def download_file(url: str, dest_path: Path, chunk_size: int = 8192) -> bool:
    """下载文件并显示进度"""
    try:
        print(f"下载: {url}")
        print(f"目标: {dest_path}")
        
        # 创建目录
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        
        # 下载文件
        req = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.0'
        })
        
        with urllib.request.urlopen(req, timeout=300) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            
            with open(dest_path, 'wb') as f:
                while True:
                    chunk = response.read(chunk_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    
                    if total_size > 0:
                        percent = (downloaded / total_size) * 100
                        print(f"\r进度: {percent:.1f}% ({downloaded}/{total_size} bytes)", end='')
            
            print(f"\n✓ 完成: {dest_path.name}")
            return True
            
    except urllib.error.URLError as e:
        print(f"\n✗ 下载失败: {e}")
        return False
    except Exception as e:
        print(f"\n✗ 错误: {e}")
        return False


def main():
    # 允许通过命令行指定目标目录
    if len(sys.argv) > 1:
        model_dir = Path(sys.argv[1])
    else:
        model_dir = DEFAULT_MODEL_DIR
    
    print("=" * 60)
    print("Qwen3-ASR-0.6B ONNX 模型下载工具")
    print("=" * 60)
    print(f"目标目录: {model_dir}")
    print()
    
    # 检查目标目录
    model_dir.mkdir(parents=True, exist_ok=True)
    
    # 创建 tokens.txt 文件 (Qwen3-ASR 使用 Whisper 风格的词汇表)
    tokens_path = model_dir / "tokens.txt"
    if not tokens_path.exists():
        print("创建 tokens.txt 文件...")
        create_tokens_file(tokens_path)
        print(f"✓ 完成: tokens.txt\n")
    else:
        print(f"文件已存在，跳过: tokens.txt\n")
    
    # 下载每个模型文件
    success_count = 1  # tokens.txt 算一个
    for remote_path, local_name in MODEL_FILES.items():
        dest_path = model_dir / local_name
        
        # 检查文件是否已存在
        if dest_path.exists():
            print(f"文件已存在，跳过: {local_name}")
            success_count += 1
            continue
        
        # 尝试多个镜像源
        downloaded = False
        for repo in MODEL_REPOS:
            url = f"{repo}/{remote_path}"
            if download_file(url, dest_path):
                downloaded = True
                break
            print(f"  尝试备用源...")
        
        if downloaded:
            success_count += 1
        print()
    
    # 总结
    print("=" * 60)
    print(f"下载完成: {success_count}/{len(MODEL_FILES)} 个文件")
    
    total_files = len(MODEL_FILES) + 1  # +1 for tokens.txt
    if success_count == total_files:
        print("✓ 所有文件准备成功！")
        print()
        print("模型目录结构:")
        print(f"  {model_dir}/")
        print(f"    ├── model.onnx    (编码器)")
        print(f"    ├── decoder.onnx  (解码器，可选)")
        print(f"    └── tokens.txt    (词汇表)")
        return 0
    else:
        print("✗ 部分文件下载失败，请检查网络连接后重试")
        print()
        print("提示: 如果下载失败，您可以手动从以下地址下载:")
        for repo in MODEL_REPOS:
            print(f"  {repo}")
        return 1


def create_tokens_file(dest_path: Path):
    """创建 Qwen3-ASR 的 tokens.txt 文件"""
    # Qwen3-ASR 使用与 Whisper 类似的词汇表
    # 这里创建一个基本的 tokens 文件
    tokens = []
    
    # 特殊标记
    special_tokens = [
        "<|endoftext|>",
        "<|startoftranscript|>",
        "<|zh|>", "<|en|>", "<|ja|>", "<|ko|>",
        "<|transcribe|>", "<|translate|>",
        "<|notimestamps|>",
        "<|0.00|>", "<|1.00|>", "<|2.00|>", "<|3.00|>", "<|4.00|>", "<|5.00|>",
        "<|6.00|>", "<|7.00|>", "<|8.00|>", "<|9.00|>", "<|10.00|>", "<|11.00|>",
        "<|12.00|>", "<|13.00|>", "<|14.00|>", "<|15.00|>", "<|16.00|>", "<|17.00|>",
        "<|18.00|>", "<|19.00|>", "<|20.00|>", "<|21.00|>", "<|22.00|>", "<|23.00|>",
        "<|24.00|>", "<|25.00|>", "<|26.00|>", "<|27.00|>", "<|28.00|>", "<|29.00|>",
        "<|30.00|>",
    ]
    tokens.extend(special_tokens)
    
    # 添加基本字符 (ASCII + 常用中文)
    # 英文字母
    for i in range(26):
        tokens.append(chr(ord('a') + i))
    # 数字
    for i in range(10):
        tokens.append(str(i))
    # 常用标点
    tokens.extend([" ", ",", ".", "!", "?", ";", ":", "-", "'", '"', "(", ")", "[", "]", "{", "}"])
    
    # 写入文件
    with open(dest_path, 'w', encoding='utf-8') as f:
        for token in tokens:
            f.write(token + '\n')


if __name__ == "__main__":
    sys.exit(main())
