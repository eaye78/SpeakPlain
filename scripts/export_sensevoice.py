#!/usr/bin/env python3
"""
导出 SenseVoiceSmall 模型为 ONNX 格式
参考 funasr/models/sense_voice/export_meta.py 的导出方法
"""
import os
import types
import torch
import torch.onnx

model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
onnx_output = os.path.join(model_dir, "model.onnx")

print("=" * 60)
print("SenseVoice ONNX 导出")
print("=" * 60)

if os.path.exists(onnx_output):
    size_mb = os.path.getsize(onnx_output) / (1024 * 1024)
    print(f"\n模型已存在: {onnx_output} ({size_mb:.1f} MB)")
    exit(0)

try:
    from funasr import AutoModel
    from funasr.utils.torch_function import sequence_mask
    from funasr.models.sense_voice.export_meta import export_rebuild_model

    print("\n[1/3] 加载模型...")
    model = AutoModel(model=model_dir, device="cpu")
    # 取出内部的 PyTorch 模型
    pt_model = model.model
    pt_model.eval()
    print(f"  模型类型: {type(pt_model).__name__}")

    print("\n[2/3] 准备导出元数据...")
    pt_model = export_rebuild_model(pt_model, device="cpu", max_seq_len=512)

    # 获取虚拟输入（来自 export_meta.py）
    dummy = pt_model.export_dummy_inputs()
    input_names  = pt_model.export_input_names()
    output_names = pt_model.export_output_names()
    dynamic_axes = pt_model.export_dynamic_axes()

    print(f"  输入: {input_names}")
    print(f"  输出: {output_names}")
    print(f"  虚拟输入形状: {[t.shape for t in dummy]}")

    print(f"\n[3/3] 导出 ONNX -> {onnx_output} ...")
    with torch.no_grad():
        torch.onnx.export(
            pt_model,
            dummy,
            onnx_output,
            export_params=True,
            opset_version=14,
            do_constant_folding=True,
            input_names=input_names,
            output_names=output_names,
            dynamic_axes=dynamic_axes,
            verbose=False,
            dynamo=False,  # 使用旧版导出器，避免类型推断问题
        )

    size_mb = os.path.getsize(onnx_output) / (1024 * 1024)
    print(f"\n✓ 导出成功: {onnx_output}")
    print(f"  大小: {size_mb:.1f} MB")

    # 验证
    print("\n验证 ONNX 模型...")
    import onnx
    m = onnx.load(onnx_output)
    onnx.checker.check_model(m)
    print("✓ 模型验证通过")

except Exception as e:
    print(f"\n✗ 失败: {e}")
    import traceback
    traceback.print_exc()

print("\n" + "=" * 60)
