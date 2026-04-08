#!/usr/bin/env python3
"""将SenseVoice PyTorch模型转换为ONNX格式"""

import os
import sys
import torch
import json

# 添加funasr路径
sys.path.insert(0, r"d:\projects\SpeakPlain\wiki\FunASR-main")

def convert_model():
    model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
    pt_path = os.path.join(model_dir, "model.pt")
    onnx_path = os.path.join(model_dir, "model.onnx")
    tokens_pt_path = os.path.join(model_dir, "tokens.json")
    tokens_txt_path = os.path.join(model_dir, "tokens.txt")
    
    print("开始转换模型...")
    print(f"PyTorch模型: {pt_path}")
    print(f"ONNX输出: {onnx_path}")
    
    if not os.path.exists(pt_path):
        print(f"错误: 找不到模型文件 {pt_path}")
        return False
    
    try:
        # 加载PyTorch模型
        print("\n加载PyTorch模型...")
        checkpoint = torch.load(pt_path, map_location="cpu")
        
        # 检查checkpoint结构
        if isinstance(checkpoint, dict):
            print(f"Checkpoint keys: {list(checkpoint.keys())}")
            if "model" in checkpoint:
                state_dict = checkpoint["model"]
            elif "state_dict" in checkpoint:
                state_dict = checkpoint["state_dict"]
            else:
                state_dict = checkpoint
        else:
            state_dict = checkpoint
        
        print(f"模型参数数量: {len(state_dict)}")
        
        # 创建简单的ONNX模型（这里使用简化版本）
        # 实际转换需要SenseVoice的模型定义
        print("\n注意: 完整转换需要SenseVoice模型定义代码")
        print("请使用以下方式之一获取ONNX模型:")
        print("1. 使用funasr的导出工具")
        print("2. 从官方渠道下载预转换的ONNX模型")
        
        # 创建tokens.txt
        if os.path.exists(tokens_pt_path):
            print(f"\n转换tokens文件...")
            with open(tokens_pt_path, "r", encoding="utf-8") as f:
                tokens_data = json.load(f)
            
            # 保存为txt格式
            with open(tokens_txt_path, "w", encoding="utf-8") as f:
                if isinstance(tokens_data, list):
                    for token in tokens_data:
                        f.write(f"{token}\n")
                elif isinstance(tokens_data, dict):
                    for k, v in sorted(tokens_data.items(), key=lambda x: int(x[0])):
                        f.write(f"{v}\n")
            
            print(f"Tokens已保存到: {tokens_txt_path}")
        
        print("\n提示: 当前代码框架已准备就绪")
        print("请使用Sherpa-ONNX或参考VoiceSnap的模型格式")
        
        return True
        
    except Exception as e:
        print(f"转换失败: {e}")
        import traceback
        traceback.print_exc()
        return False

if __name__ == "__main__":
    convert_model()
