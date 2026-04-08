#!/usr/bin/env python3
"""验证导出的 ONNX 模型可以正常推理"""
import os
os.chdir(r"d:\projects\SpeakPlain\speakplain\models\sensevoice")

import onnxruntime as ort
import numpy as np

print("加载 ONNX 模型...")
sess = ort.InferenceSession("model.onnx", providers=["CPUExecutionProvider"])

print("输入节点:")
for i in sess.get_inputs():
    print(f"  {i.name}: {i.shape} ({i.type})")

print("输出节点:")
for o in sess.get_outputs():
    print(f"  {o.name}: {o.shape} ({o.type})")

# 虚拟推理（batch=1，30帧，560维特征）
speech = np.random.randn(1, 30, 560).astype(np.float32)
speech_lengths = np.array([30], dtype=np.int32)
language = np.array([0], dtype=np.int32)   # 0=zh
textnorm = np.array([15], dtype=np.int32)  # 15=withitn

print("\n运行推理...")
out = sess.run(None, {
    "speech": speech,
    "speech_lengths": speech_lengths,
    "language": language,
    "textnorm": textnorm,
})

print(f"ctc_logits shape: {out[0].shape}")
print(f"encoder_out_lens: {out[1]}")
print("\n✓ 推理成功！ONNX 模型可正常使用")
