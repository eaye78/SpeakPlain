#!/usr/bin/env python3
"""
测试 Qwen3-ASR ONNX 模型，了解正确的输入输出格式
"""

import onnxruntime as ort
import numpy as np
from pathlib import Path

def inspect_model(model_path: str, name: str):
    """检查 ONNX 模型的输入输出"""
    print(f"\n{'='*60}")
    print(f"模型: {name}")
    print(f"路径: {model_path}")
    print(f"{'='*60}")
    
    if not Path(model_path).exists():
        print(f"错误: 模型文件不存在")
        return
    
    # 创建推理会话
    session = ort.InferenceSession(model_path, providers=['CPUExecutionProvider'])
    
    # 打印输入信息
    print("\n【输入】")
    for i, inp in enumerate(session.get_inputs()):
        print(f"  {i+1}. 名称: {inp.name}")
        print(f"     形状: {inp.shape}")
        print(f"     类型: {inp.type}")
    
    # 打印输出信息
    print("\n【输出】")
    for i, out in enumerate(session.get_outputs()):
        print(f"  {i+1}. 名称: {out.name}")
        print(f"     形状: {out.shape}")
        print(f"     类型: {out.type}")
    
    return session

def test_model_inference(session, test_inputs: dict, name: str):
    """测试模型推理"""
    print(f"\n{'-'*60}")
    print(f"测试推理: {name}")
    print(f"{'-'*60}")
    
    try:
        outputs = session.run(None, test_inputs)
        print(f"成功! 输出数量: {len(outputs)}")
        for i, out in enumerate(outputs):
            print(f"  输出 {i+1}: 形状={out.shape}, 类型={out.dtype}")
        return outputs
    except Exception as e:
        print(f"失败: {e}")
        return None

def main():
    model_dir = Path("speakplain/models/qwen3-asr")
    
    # 检查模型文件
    model_path = model_dir / "model.onnx"
    decoder_path = model_dir / "decoder.onnx"
    
    print("Qwen3-ASR ONNX 模型分析")
    print(f"模型目录: {model_dir.absolute()}")
    
    # 检查主模型
    if model_path.exists():
        session = inspect_model(str(model_path), "model.onnx (主模型)")
        
        # 尝试创建测试输入
        print("\n【创建测试输入】")
        
        # 根据输入形状创建随机测试数据
        test_inputs = {}
        for inp in session.get_inputs():
            name = inp.name
            shape = inp.shape
            
            # 替换动态维度为固定值
            shape_fixed = []
            for dim in shape:
                if isinstance(dim, str) or dim is None:
                    shape_fixed.append(1)  # 使用 1 作为默认批次大小
                else:
                    shape_fixed.append(dim)
            
            print(f"  {name}: 原始形状 {shape} -> 固定形状 {shape_fixed}")
            
            # 根据类型创建数据
            if 'int' in inp.type.lower():
                test_inputs[name] = np.zeros(shape_fixed, dtype=np.int64)
            elif 'float' in inp.type.lower():
                test_inputs[name] = np.zeros(shape_fixed, dtype=np.float32)
            elif 'bool' in inp.type.lower():
                test_inputs[name] = np.ones(shape_fixed, dtype=np.bool_)
            else:
                test_inputs[name] = np.zeros(shape_fixed, dtype=np.float32)
        
        # 测试推理
        test_model_inference(session, test_inputs, "全零输入测试")
        
        # 尝试不同的输入值
        print("\n【尝试不同的输入值】")
        for inp in session.get_inputs():
            name = inp.name
            if name in test_inputs:
                shape = test_inputs[name].shape
                if 'int' in inp.type.lower():
                    # 尝试不同的整数值
                    for val in [0, 1, 10]:
                        test_inputs[name] = np.full(shape, val, dtype=np.int64)
                        print(f"\n测试 {name} = {val}")
                        result = test_model_inference(session, test_inputs, f"{name}={val}")
                        if result:
                            print("成功找到有效输入!")
                            break
    
    # 检查 decoder 模型
    if decoder_path.exists():
        inspect_model(str(decoder_path), "decoder.onnx (解码器)")

if __name__ == "__main__":
    main()
