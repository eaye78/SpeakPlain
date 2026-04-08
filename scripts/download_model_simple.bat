@echo off
chcp 65001 >nul
echo ============================================
echo 下载 SenseVoice ONNX 模型
echo ============================================
echo.
echo 方法1: 使用浏览器手动下载
echo --------------------------------------------
echo 1. 访问: https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models
echo 2. 下载: sense-voice-small.tar.bz2
echo 3. 解压到: d:\projects\SpeakPlain\speakplain\models\sensevoice\
echo.
echo 方法2: 使用 GitHub 命令行工具
echo --------------------------------------------
echo 如果已安装 gh 工具，运行:
echo   gh release download asr-models -R k2-fsa/sherpa-onnx -p "sense-voice-small.tar.bz2"
echo.
echo 方法3: 使用 PowerShell 下载
echo --------------------------------------------
echo 正在尝试下载...
echo.

set "URL=https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sense-voice-small.tar.bz2"
set "OUTPUT=d:\projects\SpeakPlain\speakplain\models\sensevoice\sense-voice-small.tar.bz2"

powershell -Command "& {try { Invoke-WebRequest -Uri '%URL%' -OutFile '%OUTPUT%' -TimeoutSec 300; Write-Host '下载成功' } catch { Write-Host '下载失败，请手动下载' }}"

if exist "%OUTPUT%" (
    echo.
    echo 下载成功，正在解压...
    tar -xjf "%OUTPUT%" -C "d:\projects\SpeakPlain\speakplain\models\sensevoice\"
    echo 解压完成
) else (
    echo.
    echo 自动下载失败，请手动下载
    start https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models
)

echo.
echo ============================================
pause
