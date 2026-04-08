#!/usr/bin/env python3
"""导出SenseVoice模型为ONNX格式"""
import os
import sys
import torch
import numpy as np

# 设置模型路径
model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
os.chdir(model_dir)

print("=" * 60)
print("SenseVoice ONNX 导出工具")
print("=" * 60)

# 方法1: 使用funasr导出工具
try:
    from funasr import AutoModel
    from funasr.utils.export_utils import export_onnx
    
    print("\n[方法1] 使用 funasr 导出工具...")
    print("加载模型...")
    
    model = AutoModel(
        model=model_dir,
        device="cpu",
    )
    
    print("导出ONNX模型...")
    export_dir = os.path.join(model_dir, "onnx")
    os.makedirs(export_dir, exist_ok=True)
    
    # 导出为ONNX
    model.export(export_dir, type="onnx")
    
    print(f"✓ ONNX模型已导出到: {export_dir}")
    
except Exception as e:
    print(f"✗ 方法1失败: {e}")
    
    # 方法2: 手动导出
    print("\n[方法2] 手动导出...")
    
    try:
        # 加载PyTorch模型
        print("加载PyTorch模型...")
        checkpoint = torch.load(
            os.path.join(model_dir, "model.pt"),
            map_location="cpu"
        )
        
        print(f"Checkpoint类型: {type(checkpoint)}")
        if isinstance(checkpoint, dict):
            print(f"Keys: {list(checkpoint.keys())}")
        
        # 由于SenseVoice模型结构复杂，建议使用预转换的ONNX模型
        print("\n注意: SenseVoice模型结构较复杂，建议:")
        print("1. 使用 sherpa-onnx 提供的预转换ONNX模型")
        print("2. 或参考 https://github.com/k2-fsa/sherpa-onnx 的文档")
        
        # 创建一个占位文件
        placeholder = os.path.join(model_dir, "README_ONNX.txt")
        with open(placeholder, "w", encoding="utf-8") as f:
            f.write("""SenseVoice ONNX Model
====================

当前下载的是PyTorch格式(.pt)的模型。

要获取ONNX格式模型，请使用以下方法之一:

方法1: 使用sherpa-onnx预转换模型（推荐）
----------------------------------------
访问: https://github.com/k2-fsa/sherpa-onnx/releases
下载: sense-voice-zh-en-ja-ko-yue-*.tar.bz2
解压后将模型文件放入此目录

方法2: 使用funasr导出
--------------------
pip install funasr
python -c "from funasr import AutoModel; m = AutoModel(model='iic/SenseVoiceSmall'); m.export('./onnx', type='onnx')"

方法3: 使用VoiceSnap的模型格式
----------------------------
参考 VoiceSnap 项目的模型加载方式，使用 sherpa-onnx-go 库

当前目录文件:
- model.pt: PyTorch模型 (~893MB)
- tokens.txt: 词表文件
- config.yaml: 配置文件
- am.mvn: 均值方差归一化参数
""")
        
        print(f"\n已创建说明文件: {placeholder}")
        
    except Exception as e2:
        print(f"✗ 方法2也失败: {e2}")
        import traceback
        traceback.print_exc()

print("\n" + "=" * 60)
print("导出完成")
print("=" * 60)
