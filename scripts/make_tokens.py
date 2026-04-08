#!/usr/bin/env python3
"""将JSON tokens转换为txt格式"""
import json
import os

model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"
tokens_json = os.path.join(model_dir, "tokens.json")
tokens_txt = os.path.join(model_dir, "tokens.txt")

with open(tokens_json, "r", encoding="utf-8") as f:
    tokens = json.load(f)

with open(tokens_txt, "w", encoding="utf-8") as f:
    for token in tokens:
        f.write(f"{token}\n")

print(f"Tokens已保存到: {tokens_txt}")
print(f"共 {len(tokens)} 个token")
