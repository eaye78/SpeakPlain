@echo off
chcp 65001 >nul
echo ============================================
echo Qwen3-ASR-0.6B ONNX 模型下载工具
echo ============================================
echo.

REM 检查 Python 是否安装
python --version >nul 2>&1
if errorlevel 1 (
    echo 错误: 未找到 Python，请先安装 Python 3.8 或更高版本
    echo 下载地址: https://www.python.org/downloads/
    pause
    exit /b 1
)

REM 运行下载脚本
echo 正在启动下载脚本...
python "%~dp0download_qwen3_asr.py" %*

if errorlevel 1 (
    echo.
    echo 下载失败，请检查网络连接
    pause
    exit /b 1
)

echo.
pause
