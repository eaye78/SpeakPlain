#!/usr/bin/env python3
"""下载Sherpa-ONNX预转换的SenseVoice模型"""
import os
import urllib.request
import tarfile
import sys

def download_file(url, output_path, chunk_size=8192):
    """带进度条的文件下载"""
    print(f"下载: {url}")
    print(f"保存到: {output_path}")
    
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        
        with urllib.request.urlopen(req, timeout=300) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            
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

def extract_tar_bz2(tar_path, extract_dir):
    """解压 tar.bz2 文件"""
    print(f"\n解压: {tar_path}")
    try:
        with tarfile.open(tar_path, 'r:bz2') as tar:
            tar.extractall(extract_dir)
        print(f"✓ 解压完成到: {extract_dir}")
        return True
    except Exception as e:
        print(f"✗ 解压失败: {e}")
        return False

def main():
    model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
    os.makedirs(model_dir, exist_ok=True)
    
    # Sherpa-ONNX SenseVoice 模型
    url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sense-voice-small.tar.bz2"
    tar_path = os.path.join(model_dir, "sense-voice-small.tar.bz2")
    
    print("=" * 60)
    print("下载 Sherpa-ONNX SenseVoice 模型")
    print("=" * 60)
    
    # 下载
    if not os.path.exists(tar_path):
        if not download_file(url, tar_path):
            print("\n尝试备用下载方式...")
            # 使用镜像或备用链接
            return
    else:
        print(f"文件已存在: {tar_path}")
    
    # 解压
    if os.path.exists(tar_path):
        extract_tar_bz2(tar_path, model_dir)
        
        # 列出解压后的文件
        print("\n模型文件:")
        for root, dirs, files in os.walk(model_dir):
            level = root.replace(model_dir, '').count(os.sep)
            indent = ' ' * 2 * level
            print(f'{indent}{os.path.basename(root)}/')
            subindent = ' ' * 2 * (level + 1)
            for file in files:
                filepath = os.path.join(root, file)
                size = os.path.getsize(filepath)
                size_mb = size / (1024 * 1024)
                print(f'{subindent}{file} ({size_mb:.2f} MB)')

if __name__ == "__main__":
    main()
