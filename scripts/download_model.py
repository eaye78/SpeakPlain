#!/usr/bin/env python3
"""下载SenseVoice Small模型"""

import os
from modelscope import snapshot_download

def download_sensevoice():
    # 目标目录
    target_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
    os.makedirs(target_dir, exist_ok=True)
    
    print("开始下载SenseVoice Small模型...")
    print(f"目标目录: {target_dir}")
    
    try:
        # 从ModelScope下载
        model_dir = snapshot_download(
            "iic/SenseVoiceSmall",
            local_dir=target_dir
        )
        
        print(f"\n模型下载完成!")
        print(f"模型目录: {model_dir}")
        
        # 列出下载的文件
        print("\n下载的文件:")
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
        
        return True
        
    except Exception as e:
        print(f"下载失败: {e}")
        return False

if __name__ == "__main__":
    download_sensevoice()
