#!/usr/bin/env python3
"""
SenseVoice 模型设置脚本
帮助用户获取 ONNX 格式的模型
"""
import os
import sys
import webbrowser

model_dir = r"d:\projects\SpeakPlain\speakplain\models\sensevoice"

def print_header(text):
    print("\n" + "=" * 60)
    print(text)
    print("=" * 60)

def check_current_models():
    """检查当前模型文件"""
    print_header("当前模型文件")
    
    files = {
        "model.pt": "PyTorch模型 (~893MB)",
        "tokens.txt": "词表文件",
        "config.yaml": "配置文件",
        "am.mvn": "均值方差归一化",
        "model.onnx": "ONNX模型 (待获取)",
    }
    
    for filename, desc in files.items():
        filepath = os.path.join(model_dir, filename)
        exists = "✓" if os.path.exists(filepath) else "✗"
        print(f"  {exists} {filename:<20} {desc}")

def download_option_1():
    """选项1: 打开浏览器下载 Sherpa-ONNX 模型"""
    print_header("选项1: 下载 Sherpa-ONNX 预转换模型")
    
    url = "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models"
    print(f"打开浏览器: {url}")
    print("\n请按以下步骤操作:")
    print("1. 在页面中找到 'sense-voice-small.tar.bz2'")
    print("2. 点击下载")
    print("3. 解压到:")
    print(f"   {model_dir}")
    print("\n正在打开浏览器...")
    
    webbrowser.open(url)

def download_option_2():
    """选项2: 使用 funasr 导出"""
    print_header("选项2: 使用 funasr 导出 ONNX")
    
    print("安装依赖:")
    print("  pip install funasr onnxscript onnx")
    print("\n导出命令:")
    print("  python export_sensevoice.py")
    print("\n注意: 导出过程可能需要较长时间")

def download_option_3():
    """选项3: 使用 ModelScope 下载"""
    print_header("选项3: 从 ModelScope 下载")
    
    print("模型页面:")
    print("  https://modelscope.cn/models/iic/SenseVoiceSmall")
    print("\n已下载的文件:")
    print(f"  {model_dir}")
    print("\n需要转换为ONNX格式才能使用")

def create_placeholder():
    """创建占位模型文件用于开发测试"""
    print_header("创建开发测试占位")
    
    placeholder_path = os.path.join(model_dir, "model.onnx.placeholder")
    with open(placeholder_path, "w") as f:
        f.write("# Placeholder for model.onnx\n")
        f.write("# Replace this with the actual ONNX model file\n")
    
    print(f"✓ 创建占位文件: {placeholder_path}")
    print("\n注意: 这是占位文件，实际运行需要真实的ONNX模型")

def main():
    print_header("SenseVoice 模型设置")
    
    check_current_models()
    
    print_header("获取 ONNX 模型的方法")
    
    print("""
请选择:
1. 下载 Sherpa-ONNX 预转换模型 (推荐，最简单)
2. 使用 funasr 导出 ONNX (需要安装依赖)
3. 查看 ModelScope 模型页面
4. 创建开发测试占位
5. 退出
""")
    
    choice = input("请输入选项 (1-5): ").strip()
    
    if choice == "1":
        download_option_1()
    elif choice == "2":
        download_option_2()
    elif choice == "3":
        download_option_3()
    elif choice == "4":
        create_placeholder()
    else:
        print("退出")
        return
    
    print("\n" + "=" * 60)
    print("模型目录:")
    print(f"  {model_dir}")
    print("=" * 60)

if __name__ == "__main__":
    main()
