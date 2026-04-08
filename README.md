# SpeakPlain 说人话

<div align="center">

![Logo](speakplain/src-tauri/icons/128x128.png)

**AI语音输入法 - 让语音输入更智能、更高效**

[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri)](https://tauri.app)
[![React](https://img.shields.io/badge/React-18-blue?logo=react)](https://react.dev)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0-blue?logo=typescript)](https://www.typescriptlang.org)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

</div>

## ✨ 功能特性

### 🎙️ 语音输入
- **按住说话 (Hold-to-Talk)**：长按热键录音，松手自动识别并输入
- **自由说话 (Free-Talk)**：短按热键切换模式，持续录音直到手动停止
- **智能静音检测**：自动检测语音结束，无需手动操作
- **音量实时反馈**：悬浮窗显示实时音量波形

### 🎨 主题皮肤系统
- **多主题支持**：经典、彩虹、沙漠、星空等内置主题
- **自定义皮肤**：支持通过 JSON + CSS 自定义皮肤样式
- **动态切换**：实时预览，一键切换
- **背景图片**：支持为皮肤添加背景图片

### ⌨️ 热键系统
- **可配置热键**：支持多种热键组合（F1-F12、Ctrl、Alt、Shift 等）
- **智能识别**：短按切换自由说话模式，长按触发按住说话
- **组合键取消**：录音过程中按组合键可取消识别

### 🛠️ 智能后处理
- **语气词过滤**：自动去除"嗯、啊、呃"等填充词
- **句首大写**：自动将句子首字母大写
- **智能空格**：优化中英文之间的空格处理

### 💻 系统支持
- **GPU 加速**：支持 DirectML GPU 加速（自动回退到 CPU）
- **多音频设备**：可选择不同的麦克风设备
- **系统托盘**：最小化到托盘，通过托盘菜单快速操作
- **悬浮窗**：可拖拽的悬浮指示器，显示录音状态

## 🚀 快速开始

### 环境要求
- Windows 10/11
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) 1.70+

### 安装依赖

```bash
# 克隆项目
git clone https://github.com/yourusername/SpeakPlain.git
cd SpeakPlain

# 安装前端依赖
cd speakplain
npm install

# 返回项目根目录
cd ..
```

### 下载模型

```bash
# 运行模型下载脚本
python scripts/download_model.py
```

### 开发运行

```bash
cd speakplain
npm run tauri dev
```

### 构建发布

```bash
cd speakplain
npm run tauri build
```

## 📁 项目结构

```
SpeakPlain/
├── speakplain/          # 前端应用 (React + TypeScript + Tauri)
│   ├── src/
│   │   ├── components/  # React 组件
│   │   ├── themes/      # 主题皮肤系统
│   │   ├── stores/      # 状态管理
│   │   └── ...
│   ├── src-tauri/       # Tauri 后端 (Rust)
│   │   ├── src/
│   │   │   ├── main.rs      # 主入口
│   │   │   ├── asr.rs       # 语音识别引擎
│   │   │   ├── audio.rs     # 音频录制
│   │   │   ├── hotkey.rs    # 热键管理
│   │   │   ├── indicator.rs # 悬浮窗
│   │   │   └── ...
│   │   └── icons/       # 应用图标
│   └── skins/           # 主题皮肤目录
├── scripts/             # 工具脚本
└── models/              # AI 模型文件 (运行时下载)
```

## 🎨 自定义皮肤

在 `speakplain/skins/` 目录下创建新文件夹，包含以下文件：

### skin.json
```json
{
  "id": "my-skin",
  "name": "我的皮肤",
  "description": "自定义皮肤描述",
  "version": "1.0.0",
  "author": "Your Name",
  "hasBackgroundImage": true,
  "colors": {
    "background": "#ffffff",
    "backgroundGradient": "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
    "textPrimary": "#333333",
    "textSecondary": "#666666",
    "textActive": "#1890ff",
    "waveformPrimary": "#1890ff",
    "waveformSecondary": "#52c41a",
    "dragDot": "#d9d9d9",
    "processingDot": "#1890ff",
    "shadowLight": "#ffffff",
    "shadowDark": "#d9d9d9"
  },
  "dimensions": {
    "borderRadius": 12,
    "paddingX": 16,
    "paddingY": 8,
    "gap": 10
  },
  "animations": {
    "transitionDuration": "0.3s"
  }
}
```

### styles.css（可选）
自定义 CSS 样式，覆盖默认样式。

### background.png（可选）
背景图片，需在 `skin.json` 中设置 `hasBackgroundImage: true`。

## ⚙️ 配置说明

### 热键设置
- 支持的热键：F1-F12、数字键、字母键，配合 Ctrl/Alt/Shift 组合
- 默认热键：F1

### 音频设置
- **静音超时**：检测到静音后自动停止录音的时间（毫秒）
- **语音检测阈值**：音量阈值，用于区分语音和静音

### 识别设置
- **GPU 加速**：使用 DirectML 进行 GPU 加速推理
- **去除语气词**：自动过滤"嗯、啊、呃"等填充词
- **句首大写**：自动将句子首字母大写

## 🛡️ 隐私说明

- 所有语音识别均在本地完成，不会上传音频到云端
- 识别历史仅保存在本地 SQLite 数据库中

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

本项目采用 [MIT](LICENSE) 许可证。

## 📅 产品规划路线

### ✅ 第一阶段：基础语音识别（已完成）
- [x] 本地语音识别引擎集成（SenseVoice）
- [x] 按住说话（Hold-to-Talk）模式
- [x] 自由说话（Free-Talk）模式
- [x] 智能静音检测（VAD）
- [x] 实时音量波形显示
- [x] 系统热键支持
- [x] 识别历史记录

### ✅ 第二阶段：自定义皮肤（已完成）
- [x] 多主题皮肤系统
- [x] 内置主题：经典、彩虹、沙漠、星空
- [x] 自定义皮肤开发支持（JSON + CSS）
- [x] 背景图片支持
- [x] 动态皮肤切换

### 🚧 第三阶段：大语言模型人设（进行中）
- [ ] 本地大语言模型集成
- [ ] 智能文本润色与纠错
- [ ] 多场景人设切换（正式/ casual /创意等）
- [ ] 自定义提示词模板
- [ ] 上下文记忆功能
- [ ] 流式生成响应

## 🙏 致谢

### 开源项目
- [Tauri](https://tauri.app/) - 跨平台应用框架
- [React](https://react.dev/) - 前端 UI 框架
- [Ant Design](https://ant.design/) - UI 组件库

### 语音识别
- [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) - 阿里达摩院开源语音识别模型
- [Sherpa ONNX](https://github.com/k2-fsa/sherpa-onnx) - 本地语音识别引擎
- [FunASR](https://github.com/alibaba-damo-academy/FunASR) - 阿里达摩院语音实验室开源工具包

### 参考项目
- [VoiceSnap](https://github.com/qiuqiu-77/VoiceSnap) - 语音输入工具参考
- [ququ](https://github.com/qiuqiu-77/ququ) - 语音交互方案参考
- [vocotype-cli](https://github.com/qiuqiu-77/vocotype-cli) - 语音输入 CLI 工具参考

---

<div align="center">

**Made with ❤️ by SpeakPlain Team**

</div>