#!/usr/bin/env python3
"""
下载完整的 Qwen3-ASR ONNX 模型（包含编码器和解码器）
来源: https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU
"""

import os
import sys
import urllib.request
import urllib.error
from pathlib import Path
from tqdm import tqdm

# 模型文件列表
MODEL_FILES = [
    ("encoder_conv.onnx", 48_000_000),
    ("encoder_transformer.onnx", 669_000_000),
    ("decoder_init.int8.onnx", 571_000_000),
    ("decoder_step.int8.onnx", 571_000_000),
    ("embed_tokens.bin", 622_000_000),
    ("tokenizer.json", 11_000_000),
    ("onnx_inference.py", 50_000),
]

# 尝试多个镜像源
MIRROR_URLS = [
    "https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU/resolve/main",
    "https://hf-mirror.com/Daumee/Qwen3-ASR-0.6B-ONNX-CPU/resolve/main",
    "https://modelscope.cn/models/Daumee/Qwen3-ASR-0.6B-ONNX-CPU/resolve/master",
]


class DownloadProgressBar(tqdm):
    def update_to(self, b=1, bsize=1, tsize=None):
        if tsize is not None:
            self.total = tsize
        self.update(b * bsize - self.n)


def download_file(url: str, output_path: Path, expected_size: int = None, timeout: int = 300):
    """下载文件并显示进度"""
    print(f"下载: {url}")
    print(f"保存到: {output_path}")
    
    try:
        # 设置超时
        import socket
        socket.setdefaulttimeout(timeout)
        
        with DownloadProgressBar(unit='B', unit_scale=True, miniters=1, desc=output_path.name) as t:
            urllib.request.urlretrieve(url, filename=str(output_path), reporthook=t.update_to)
        
        # 验证文件大小
        actual_size = output_path.stat().st_size
        if expected_size and actual_size < expected_size * 0.9:  # 允许 10% 的误差
            print(f"警告: 文件大小异常 (期望 {expected_size}, 实际 {actual_size})")
            return False
        
        print(f"✓ 下载完成: {actual_size / 1024 / 1024:.1f} MB\n")
        return True
        
    except urllib.error.HTTPError as e:
        print(f"✗ 下载失败 (HTTP {e.code}): {e.reason}")
        return False
    except Exception as e:
        print(f"✗ 下载失败: {e}")
        return False


def main():
    # 确定模型保存目录
    script_dir = Path(__file__).parent.absolute()
    project_root = script_dir.parent
    model_dir = project_root / "speakplain" / "models" / "qwen3-asr-full"
    
    print(f"模型将保存到: {model_dir}")
    print("=" * 60)
    
    # 创建目录
    model_dir.mkdir(parents=True, exist_ok=True)
    
    # 下载每个文件
    success_count = 0
    failed_files = []
    
    for filename, expected_size in MODEL_FILES:
        output_path = model_dir / filename
        
        # 检查文件是否已存在
        if output_path.exists():
            actual_size = output_path.stat().st_size
            if actual_size >= expected_size * 0.9:
                print(f"✓ 文件已存在: {filename} ({actual_size / 1024 / 1024:.1f} MB)")
                success_count += 1
                continue
            else:
                print(f"文件不完整，重新下载: {filename}")
        
        # 尝试多个镜像源下载
        downloaded = False
        for base_url in MIRROR_URLS:
            url = f"{base_url}/{filename}"
            if download_file(url, output_path, expected_size):
                downloaded = True
                break
        
        if downloaded:
            success_count += 1
        else:
            failed_files.append(filename)
    
    # 打印总结
    print("=" * 60)
    print(f"下载完成: {success_count}/{len(MODEL_FILES)} 个文件")
    
    if failed_files:
        print(f"失败文件: {', '.join(failed_files)}")
        print("\n可能的解决方案:")
        print("1. 检查网络连接")
        print("2. 使用代理或 VPN 访问 HuggingFace")
        print("3. 手动从 https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU 下载")
        return 1
    else:
        print("所有文件下载成功!")
        print(f"\n模型目录: {model_dir}")
        print("\n文件列表:")
        for filename, _ in MODEL_FILES:
            file_path = model_dir / filename
            if file_path.exists():
                size_mb = file_path.stat().st_size / 1024 / 1024
                print(f"  - {filename}: {size_mb:.1f} MB")
        return 0


if __name__ == "__main__":
    sys.exit(main())
