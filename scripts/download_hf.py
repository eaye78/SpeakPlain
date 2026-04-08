#!/usr/bin/env python3
"""从 Hugging Face 下载 SenseVoice ONNX 模型"""
import os
import urllib.request
import sys

model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"

def download_with_progress(url, output_path):
    """带进度条的下载"""
    print(f"下载: {url}")
    
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        
        with urllib.request.urlopen(req, timeout=300) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            chunk_size = 8192
            
            with open(output_path, 'wb') as f:
                while True:
                    chunk = response.read(chunk_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    
                    if total_size > 0:
                        percent = (downloaded / total_size) * 100
                        mb = downloaded / (1024 * 1024)
                        total_mb = total_size / (1024 * 1024)
                        sys.stdout.write(f"\r进度: {percent:.1f}% ({mb:.1f}/{total_mb:.1f} MB)")
                        sys.stdout.flush()
            
            print("\n✓ 下载完成")
            return True
            
    except Exception as e:
        print(f"\n✗ 下载失败: {e}")
        return False

def main():
    print("=" * 60)
    print("从 Hugging Face 下载 SenseVoice ONNX 模型")
    print("=" * 60)
    
    # Hugging Face 镜像
    base_urls = [
        "https://huggingface.co/FunAudioLLM/SenseVoiceSmall/resolve/main/model.onnx",
        "https://hf-mirror.com/FunAudioLLM/SenseVoiceSmall/resolve/main/model.onnx",
    ]
    
    output_path = os.path.join(model_dir, "model.onnx")
    
    if os.path.exists(output_path):
        size = os.path.getsize(output_path) / (1024 * 1024)
        print(f"\n模型文件已存在: {output_path}")
        print(f"大小: {size:.2f} MB")
        return
    
    for url in base_urls:
        print(f"\n尝试: {url}")
        if download_with_progress(url, output_path):
            size = os.path.getsize(output_path) / (1024 * 1024)
            print(f"\n✓ 模型已保存: {output_path}")
            print(f"  大小: {size:.2f} MB")
            return
    
    print("\n✗ 所有下载源都失败了")
    print("\n请手动下载:")
    print("1. 访问 https://huggingface.co/FunAudioLLM/SenseVoiceSmall")
    print("2. 下载 model.onnx")
    print(f"3. 放置到: {model_dir}")

if __name__ == "__main__":
    main()
