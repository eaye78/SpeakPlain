// 配置数据模型

use serde::{Serialize, Deserialize};
use crate::hotkey::key_codes;
use crate::sdr::InputSource;

/// SDR 频道预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdrChannel {
    pub id: String,           // 唯一ID
    pub name: String,         // 频道名称
    pub frequency_mhz: f64,   // 频率(MHz)，保留3位小数
    pub ctcss_tone: f32,      // CTCSS亚音(Hz)，0表示不使用
}

/// 修饰键类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModifierKey {
    None,
    Ctrl,
    Alt,
    Shift,
}

impl Default for ModifierKey {
    fn default() -> Self {
        ModifierKey::None
    }
}

/// 指令映射项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMapping {
    pub command_text: String,       // 指令文字，如"发送"
    pub key_code: i32,              // 模拟按键的虚拟键码
    pub key_name: String,           // 按键名称，如"Enter"
    pub modifier: ModifierKey,      // 修饰键
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    // 热键设置
    pub hotkey_vk: i32,
    pub hotkey_name: String,
    
    // 音频设置
    pub audio_device: Option<String>,
    pub sample_rate: u32,
    
    // ASR 模型设置
    pub asr_model: String,  // "sensevoice" 或 "qwen3-asr"
    
    // 识别设置
    pub use_gpu: bool,
    pub language: String,  // "zh", "en", "auto"
    pub use_itn: bool,     // 反文本规范化
    
    // 后处理设置
    pub remove_fillers: bool,
    pub capitalize_sentences: bool,
    pub optimize_spacing: bool,
    
    // 输入设置
    pub restore_clipboard: bool,
    pub paste_delay_ms: u64,
    
    // 指示器设置
    pub indicator_x: i32,
    pub indicator_y: i32,
    pub auto_hide_indicator: bool,
    
    // 音效设置
    pub sound_feedback: bool,
    
    // 自动启动
    pub auto_start: bool,
    
    // 静音检测
    pub silence_timeout_ms: u64,
    pub vad_threshold: f32,
    
    // 主题皮肤
    pub skin_id: String,

    // 说人话功能配置
    pub llm_enabled:     bool,   // 是否启用，默认 false
    pub persona_id:      String, // 当前人设 ID，默认 "formal"
    pub llm_provider_id: String, // 当前 LLM Provider ID

    // 指令模式配置
    pub command_mode_enabled: bool,   // 指令模式开关
    pub command_mappings: Vec<CommandMapping>, // 指令映射列表

    // SDR设备配置
    pub sdr_enabled: bool,            // SDR功能开关
    pub sdr_device_index: Option<u32>, // SDR设备索引
    pub sdr_frequency_mhz: f64,       // SDR接收频率(MHz)
    pub sdr_gain_db: f32,             // SDR增益(dB)
    pub sdr_auto_gain: bool,          // SDR自动增益
    pub sdr_output_device: String,    // SDR输出设备
    pub sdr_input_source: InputSource, // 输入源选择
    pub sdr_demod_mode: crate::sdr::DemodMode, // 解调模式
    pub sdr_ppm_correction: i32,      // PPM频率校正
    pub sdr_vad_threshold: f32,       // VAD阈值
    pub sdr_ctcss_tone: f32,          // CTCSS亚音频频率(Hz)，0表示禁用
    pub sdr_ctcss_threshold: f32,     // CTCSS检测门限
    pub sdr_bandwidth: u32,            // SDR带宽(Hz)，默认150000匹配SDR++
    pub sdr_channels: Vec<SdrChannel>, // 频道预设列表
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey_vk: key_codes::VK_F2,
            hotkey_name: "F2".to_string(),
            audio_device: None,
            sample_rate: 16000,
            asr_model: "qwen3-asr".to_string(),  // 默认使用 Qwen3-ASR
            use_gpu: true,
            language: "auto".to_string(),
            use_itn: true,
            remove_fillers: true,
            capitalize_sentences: true,
            optimize_spacing: true,
            restore_clipboard: true,
            paste_delay_ms: 100,
            indicator_x: 0,
            indicator_y: 0,
            auto_hide_indicator: true,
            sound_feedback: true,
            auto_start: false,
            silence_timeout_ms: 3000,
            vad_threshold: 0.001,
            skin_id: "classic".to_string(),
            llm_enabled:     false,
            persona_id:      "formal".to_string(),
            llm_provider_id: String::new(),
            command_mode_enabled: false,
            command_mappings: vec![
                CommandMapping {
                    command_text: "发送".to_string(),
                    key_code: 0x0D,  // VK_RETURN
                    key_name: "Enter".to_string(),
                    modifier: ModifierKey::None,
                },
                CommandMapping {
                    command_text: "回车".to_string(),
                    key_code: 0x0D,
                    key_name: "Enter".to_string(),
                    modifier: ModifierKey::None,
                },
            ],
            sdr_enabled: false,
            sdr_device_index: None,
            sdr_frequency_mhz: 438.625,  // 匹配SDR++截图频率
            sdr_gain_db: 19.7,           // FC0013最大有效增益19.7dB
            sdr_auto_gain: true,         // 启用自动增益
            sdr_output_device: String::new(),
            sdr_input_source: InputSource::Microphone,
            sdr_demod_mode: crate::sdr::DemodMode::Wbfm,  // 匹配SDR++截图WFM模式
            sdr_ppm_correction: 0,
            sdr_vad_threshold: 0.01,
            sdr_ctcss_tone: 85.4,       // 默认CTCSS频率
            sdr_ctcss_threshold: 0.005,  // 降低门限到0.5%
            sdr_bandwidth: 150_000,      // 匹配SDR++带宽150kHz
            sdr_channels: vec![          // 内置常见频道
                SdrChannel { id: "ch1".to_string(), name: "业余144".to_string(), frequency_mhz: 144.500, ctcss_tone: 0.0 },
                SdrChannel { id: "ch2".to_string(), name: "业余430".to_string(), frequency_mhz: 430.000, ctcss_tone: 0.0 },
                SdrChannel { id: "ch3".to_string(), name: "业余438".to_string(), frequency_mhz: 438.625, ctcss_tone: 85.4 },
            ],
        }
    }
}
