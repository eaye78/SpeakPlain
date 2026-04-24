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
- **识别等待反馈**：松手后波形区冻结末帧 + 呼吸动画 + 底部扫光进度条，左侧实时显示等待秒数

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

### 🎯 指令模式
- **语音指令**：说出指令文字直接触发键盘按键，无需手动操作
- **自定义映射**：支持自定义指令文字与按键的映射关系
- **组合键支持**：支持 Ctrl、Alt、Shift 等修饰键组合
- **智能匹配**：自动忽略标点符号，支持"发送"、"发送。"等多种变体
- **与润色互斥**：指令模式下跳过 LLM 润色，直接执行按键操作

### 🤖 多模型 ASR 引擎
- **SenseVoice**：阿里达摩院轻量多语言识别模型，启动快、延迟低
- **Qwen3-ASR-0.6B**：阿里通义千问 3 语音识别模型，0.6B 参数，精度更高
- **一键切换**：设置页面运行时热切换模型，配置自动持久化
- **安装检测**：自动识别模型是否已安装，适配开发与生产环境路径

### 💻 系统支持
- **GPU 加速**：支持 DirectML GPU 加速（自动回退到 CPU）
- **多音频设备**：可选择不同的麦克风设备
- **系统托盘**：最小化到托盘，通过托盘菜单快速操作
- **悬浮窗**：可拖拽的悬浮指示器，显示录音状态

### 🗣️ 说人话（LLM 润色）
- **LLM 接入**：支持本地 Ollama、vLLM 及 OpenAI 兼容云端 API
- **人设驱动润色**：语音识别原文经 LLM 按选定人设风格重写后再输出
- **7 套内置人设**：正式书面、简洁精炼、口语自然、逻辑严谨、创意文案、中译英、英译中
- **自定义人设**：支持自定义 System Prompt，ID 自动生成
- **润色状态反馈**：悬浮框实时显示"润色中 Xs"计秒，失败自动降级输出原文
- **思考过程过滤**：自动去除 `<think>` 标签内容，只输出最终结果

### 🔊 无线电语音输入（RTL-SDR）
- **RTL-SDR 接入**：通过低成本 RTL-SDR 硬件接收无线电信号，实现无线对讲机语音转文字
- **FM 解调**：支持 WFM（宽带 FM）/ NFM（窄带 FM）/ AM 解调模式
- **亚音 CTCSS 过滤**：支持亚音静噪（CTCSS），只有携带指定亚音的信号才触发识别，屏蔽无关噪音
- **智能 VAD**：基于 IQ 信号功率的语音活动检测，无人发射时完全静音，有人发射时自动启动识别
- **频道预设**：支持保存多个常用频道（频率 + 亚音），一键切换
- **实时音波反馈**：悬浮框实时显示与扬声器一致的音频波形
- **输入源切换**：可在麦克风输入与 SDR 无线电输入之间随时切换
- **指令模式兼容**：SDR 识别结果同样支持指令模式，说出指令触发按键操作

## 🚀 快速开始

### 🖥️ 直接使用（Release 版）

1. 从 [Releases](https://github.com/yourusername/SpeakPlain/releases) 下载最新的 `SpeakPlain-vX.X.X-Windows-x64.zip`
2. 解压到任意目录
3. **下载 ASR 模型**（见下方说明），放入 `models/` 目录
4. 双击 `speakplain.exe` 启动

#### ASR 模型下载

| 模型 | 精度 | 大小 | 目录结构 | 下载地址 |
|------|------|------|----------|----------|
| SenseVoice（默认） | 标准 | ~400MB | `models/sensevoice/model.onnx` | [HuggingFace](https://huggingface.co/FunAudioLLM/SenseVoiceSmall) |
| Qwen3-ASR-0.6B（推荐） | 更高 | ~1.5GB | `models/Qwen3-ASR-0.6B-ONNX-CPU/onnx_models/...` | [HuggingFace](https://huggingface.co/Qwen/Qwen3-ASR) |

**方式一：使用下载脚本（需要 Python）**

```bash
# 下载 SenseVoice 模型
python scripts/download_model.py

# 下载 Qwen3-ASR-0.6B 模型（推荐，精度更高）
python scripts/download_qwen3_full.py
# 或使用一键脚本（Windows）
scripts/download_qwen3_full.bat
```

**方式二：手动下载**

SenseVoice：
1. 前往 [HuggingFace SenseVoiceSmall](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)
2. 下载 `model.onnx`，放到 `models/sensevoice/model.onnx`

Qwen3-ASR-0.6B（需要全套 ONNX 文件）：
1. 前往 [HuggingFace Qwen3-ASR](https://huggingface.co/Qwen/Qwen3-ASR)（或使用 ModelScope 镜像）
2. 将模型文件放到以下结构：
```
models/
└── Qwen3-ASR-0.6B-ONNX-CPU/
    ├── onnx_models/
    │   ├── encoder_conv.onnx
    │   ├── encoder_transformer.onnx
    │   ├── decoder_init.int8.onnx
    │   ├── decoder_step.int8.onnx
    │   └── embed_tokens.bin
    └── tokenizer.json
```

> 国内用户推荐使用 [ModelScope 镜像](https://modelscope.cn) 下载，速度更快。

#### RTL-SDR 驱动安装（使用无线电输入功能时需要）

如需使用 SDR 无线电语音输入功能，请按以下步骤安装驱动：

1. **下载驱动包**：[RTL-SDR Blog 驱动（Release.zip）](http://github.com/rtlsdrblog/rtl-sdr-blog/releases/latest/download/Release.zip)
2. **复制驱动文件**：解压后将 `x64/` 目录下的所有文件复制到应用目录的 `sdr/x64/` 下
3. **替换 USB 驱动**：将 RTL-SDR 设备接入电脑，在应用设置 → 无线电输入页面点击 **安装/更新驱动（Zadig）** 按钮，在弹出的 Zadig 窗口中选择 RTL-SDR 设备，将驱动替换为 **WinUSB**

> 注意：Zadig 需要管理员权限运行，应用会自动以管理员身份启动它。

---

### 👨‍💻 开发环境搭建

#### 环境要求
- Windows 10/11
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) 1.70+

#### 安装依赖

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

#### 下载模型

```bash
# 下载 SenseVoice 模型（默认）
python scripts/download_model.py

# 下载 Qwen3-ASR-0.6B 模型（可选，精度更高）
python scripts/download_qwen3_asr.py
# 或使用一键脚本
scripts/download_qwen3_asr.bat
```

> 模型文件放置在 `speakplain/models/` 目录下，应用启动时自动识别。

#### 开发运行

```bash
cd speakplain
npm run tauri dev
```

#### 构建发布

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
- **ASR 模型选择**：在 SenseVoice 和 Qwen3-ASR-0.6B 之间切换，运行时热切换无需重启
- **GPU 加速**：使用 DirectML 进行 GPU 加速推理
- **去除语气词**：自动过滤"嗯、啊、呃"等填充词
- **句首大写**：自动将句子首字母大写

### 说人话设置

#### 第一步：配置 LLM 提供方

在设置 → 说人话 → LLM 提供方中新建一个提供方：

| 类型 | 适用场景 | 示例地址 |
|------|----------|----------|
| Ollama（本地） | 本地运行大模型 | `http://localhost:11434` |
| vLLM（本地/服务器） | 高性能推理服务 | `http://localhost:8000` |
| OpenAI 兼容 | 云端 API（OpenAI、DeepSeek 等） | `https://api.openai.com` |

配置完成后点击**测试连接**确认可用，再点击该行**选中**为当前使用的提供方。

#### 第二步：选择人设

| 人设 | 适用场景 | 效果示例 |
|------|----------|----------|
| **正式书面** | 工作邮件、汇报、公文 | 口语 → 规范书面语 |
| **简洁精炼** | 备忘录、清单、摘要 | 冗余表达 → 精炼要点 |
| **口语自然** | 聊天、日常沟通 | 去除停顿词，语句更流畅 |
| **逻辑严谨** | 技术文档、分析报告 | 重新组织为条理清晰的结构 |
| **创意文案** | 营销、推广、故事 | 改写为生动有感染力的文字 |
| **中译英** | 中文输入 → 英文输出 | 语音说中文，输出英文翻译 |
| **英译中** | 英文输入 → 中文输出 | 语音说英文，输出中文翻译 |

也可在人设列表底部新建**自定义人设**，填写名称和 System Prompt 即可。

#### 第三步：开启功能

打开**说人话功能**开关，之后每次语音识别完成后都会自动调用 LLM 进行润色，悬浮框会显示"润色中 Xs"，完成后将润色结果粘贴到当前输入框。

> 润色失败时（如网络错误、模型不可用）自动降级，直接粘贴原始识别文字，不影响正常使用。

### 无线电输入（RTL-SDR）设置

在设置 → 无线电输入中配置：

| 参数 | 说明 | 默认值 |
|------|------|--------|
| **输入源** | 麦克风 / SDR 无线电，切换后立即生效 | 麦克风 |
| **接收频率** | 监听频率，单位 MHz，支持小数点后 3 位 | 438.625 MHz |
| **增益** | 接收增益 dB，或开启自动增益 | 自动增益 |
| **解调模式** | WFM（宽带 FM）/ NFM（窄带 FM）/ AM | WFM |
| **CTCSS 亚音** | 填入亚音频率（Hz）启用亚音静噪，0 = 禁用 | 85.4 Hz |
| **PPM 校正** | 频率偏移校正，用于补偿廉价 SDR 晶振误差 | 0 |
| **带宽** | 接收带宽 Hz，建议与软件接收机保持一致 | 150000 Hz |

#### 频道预设

在设置页面可保存多个常用频道（名称 + 频率 + 亚音），一键切换到对应配置，无需每次手动输入频率和亚音。

#### 使用流程

1. 完成驱动安装后，在设置 → 无线电输入点击 **连接设备**
2. 设置正确的接收频率和解调模式
3. 如需亚音静噪，填入对方发射使用的 CTCSS 频率
4. 将输入源切换为 **SDR 无线电**
5. 对方发射时，悬浮框自动显示并开始识别，发射结束后自动送入 ASR

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

### ✅ 第二点五阶段：多模型 & 交互优化（已完成）
- [x] Qwen3-ASR-0.6B ONNX 引擎集成
- [x] 运行时 ASR 模型热切换（无需重启）
- [x] 模型安装检测（自动适配开发/生产路径）
- [x] 识别等待 UI 优化：波形冻结 + 呼吸动画 + 底部扫光进度条
- [x] 识别等待实时计秒显示

### ✅ 第三阶段：大语言模型人设（已完成）
- [x] 本地/云端 LLM 集成（Ollama、vLLM、OpenAI 兼容接口）
- [x] 智能文本润色——识别结果经 LLM 按人设风格重写后再输出
- [x] 多场景人设切换：内置翻译、会议记录、播音员等人设
- [x] 自定义人设（自定义 System Prompt，ID 自动生成）
- [x] 人设提示词查看（内置只读，自定义可编辑）
- [x] 润色状态实时显示（悬浮框"润色中 Xs"计秒反馈）
- [x] `<think>` 思考过程自动过滤，只输出最终结果
- [x] 润色失败自动降级输出原始识别文字
- [ ] 上下文记忆功能
- [ ] 流式生成响应

### ✅ 第三点五阶段：指令模式（已完成）
- [x] 指令模式开关设置
- [x] 自定义指令文字与按键映射
- [x] 支持组合键（Ctrl、Alt、Shift）
- [x] 智能标点符号过滤匹配（含全角标点、Unicode 空白）
- [x] 指令模式与 LLM 润色互斥
- [x] 指令执行失败错误处理
- [x] SDR 无线电识别结果同样支持指令模式

### ✅ 第四阶段：无线电语音输入（已完成）
- [x] RTL-SDR 硬件接入，实时接收无线电信号
- [x] WFM / NFM / AM 多解调模式
- [x] CTCSS 亚音静噪过滤（只响应携带指定亚音的信号）
- [x] 基于 IQ 信号功率的 VAD，精准区分有无发射
- [x] 频道预设管理（名称 + 频率 + 亚音，一键切换）
- [x] 实时音波与扬声器声音同步显示
- [x] 麦克风 / SDR 输入源随时切换
- [x] Zadig 驱动安装集成（应用内一键启动）
- [x] SDR 音频 48kHz → 16kHz 自动降采样后送入 ASR

### 🔜 第五阶段（敬请期待）
- [ ] 上下文记忆功能
- [ ] 流式生成响应
- [ ] 多频点轮询监听

## 🙏 致谢

### 开源项目
- [Tauri](https://tauri.app/) - 跨平台应用框架
- [React](https://react.dev/) - 前端 UI 框架
- [Ant Design](https://ant.design/) - UI 组件库

### 语音识别
- [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) - 阿里达摩院开源语音识别模型
- [Qwen3-ASR](https://huggingface.co/Qwen/Qwen3-ASR) - 阿里通义千问 3 语音识别模型
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