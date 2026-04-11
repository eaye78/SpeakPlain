@echo off
chcp 65001 >nul
echo ==========================================
echo 下载 Qwen3-ASR 完整 ONNX 模型
echo ==========================================
echo.

set MODEL_DIR=..\speakplain\models\qwen3-asr-full

:: 创建目录
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"
cd /d "%MODEL_DIR%"

echo 模型将保存到: %CD%
echo.

:: 检查是否已安装 git-lfs
where git-lfs >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未安装 Git LFS
    echo 请先安装 Git LFS: https://git-lfs.github.com/
    pause
    exit /b 1
)

:: 克隆仓库（使用镜像源）
echo [1/3] 尝试从 HuggingFace 克隆...
git clone --depth 1 https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU temp_repo 2>nul

if %errorlevel% neq 0 (
    echo [1/3] HuggingFace 失败，尝试镜像源...
    git clone --depth 1 https://hf-mirror.com/Daumee/Qwen3-ASR-0.6B-ONNX-CPU temp_repo 2>nul
)

if %errorlevel% neq 0 (
    echo [1/3] 镜像源也失败，尝试 ModelScope...
    git clone --depth 1 https://modelscope.cn/models/Daumee/Qwen3-ASR-0.6B-ONNX-CPU.git temp_repo 2>nul
)

if %errorlevel% neq 0 (
    echo [错误] 所有下载源都失败了
    echo.
    echo 可能的解决方案:
    echo 1. 检查网络连接
    echo 2. 使用代理或 VPN
    echo 3. 手动下载: https://huggingface.co/Daumee/Qwen3-ASR-0.6B-ONNX-CPU
    echo 4. 使用浏览器访问 ModelScope: https://modelscope.cn/models/Daumee/Qwen3-ASR-0.6B-ONNX-CPU
    pause
    exit /b 1
)

:: 移动文件
echo [2/3] 移动文件...
cd temp_repo

move /Y encoder_conv.onnx ..\ >nul 2>nul
move /Y encoder_transformer.onnx ..\ >nul 2>nul
move /Y decoder_init.int8.onnx ..\ >nul 2>nul
move /Y decoder_step.int8.onnx ..\ >nul 2>nul
move /Y embed_tokens.bin ..\ >nul 2>nul
move /Y tokenizer.json ..\ >nul 2>nul
move /Y onnx_inference.py ..\ >nul 2>nul

cd ..

:: 清理临时目录
rmdir /s /q temp_repo 2>nul

echo [3/3] 下载完成!
echo.
echo 文件列表:
dir /b *.onnx *.bin *.json *.py 2>nul
echo.

pause
