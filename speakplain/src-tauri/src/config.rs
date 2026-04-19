// 应用配置模块
use serde::{Serialize, Deserialize};
use crate::storage::Storage;
use crate::hotkey::key_codes;
use crate::sdr::InputSource;

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
    pub sdr_gain_db: i32,             // SDR增益(dB)
    pub sdr_auto_gain: bool,          // SDR自动增益
    pub sdr_output_device: String,    // SDR输出设备
    pub sdr_input_source: InputSource, // 输入源选择
    pub sdr_demod_mode: crate::sdr::DemodMode, // 解调模式
    pub sdr_ppm_correction: i32,      // PPM频率校正
    pub sdr_vad_threshold: f32,       // VAD阈值
    pub sdr_ctcss_tone: f32,          // CTCSS亚音频频率(Hz)，0表示禁用
    pub sdr_ctcss_threshold: f32,     // CTCSS检测门限
    pub sdr_bandwidth: u32,            // SDR带宽(Hz)，默认150000匹配SDR++
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
            sdr_gain_db: 6,              // 默认增益6dB（接近Fitipower FC0013的6.1dB）
            sdr_auto_gain: true,         // 启用自动增益
            sdr_output_device: String::new(),
            sdr_input_source: InputSource::Microphone,
            sdr_demod_mode: crate::sdr::DemodMode::Wbfm,  // 匹配SDR++截图WFM模式
            sdr_ppm_correction: 0,
            sdr_vad_threshold: 0.01,
            sdr_ctcss_tone: 85.4,       // 默认CTCSS频率
            sdr_ctcss_threshold: 0.005,  // 降低门限到0.5%
            sdr_bandwidth: 150_000,      // 匹配SDR++带宽150kHz
        }
    }
}

impl AppConfig {
    pub fn load(storage: &Storage) -> anyhow::Result<Self> {
        let mut config = Self::default();
        
        // 从数据库加载配置
        if let Ok(Some(value)) = storage.get_setting("hotkey_vk") {
            if let Ok(vk) = value.parse::<i32>() {
                config.hotkey_vk = vk;
                config.hotkey_name = crate::hotkey::vk_to_name(vk);
            }
        }

        if let Ok(Some(value)) = storage.get_setting("hotkey_name") {
            if !value.is_empty() {
                config.hotkey_name = value;
            }
        }
        
        if let Ok(Some(value)) = storage.get_setting("audio_device") {
            config.audio_device = Some(value);
        }

        if let Ok(Some(value)) = storage.get_setting("sample_rate") {
            if let Ok(sr) = value.parse::<u32>() {
                config.sample_rate = sr;
            }
        }

        if let Ok(Some(value)) = storage.get_setting("asr_model") {
            config.asr_model = value;
        }
        
        if let Ok(Some(value)) = storage.get_setting("use_gpu") {
            config.use_gpu = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("language") {
            config.language = value;
        }

        if let Ok(Some(value)) = storage.get_setting("use_itn") {
            config.use_itn = value == "true";
        }

        if let Ok(Some(value)) = storage.get_setting("remove_fillers") {
            config.remove_fillers = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("capitalize_sentences") {
            config.capitalize_sentences = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("optimize_spacing") {
            config.optimize_spacing = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("restore_clipboard") {
            config.restore_clipboard = value == "true";
        }

        if let Ok(Some(value)) = storage.get_setting("paste_delay_ms") {
            if let Ok(ms) = value.parse::<u64>() {
                config.paste_delay_ms = ms;
            }
        }

        if let Ok(Some(value)) = storage.get_setting("indicator_x") {
            if let Ok(x) = value.parse::<i32>() {
                config.indicator_x = x;
            }
        }
        
        if let Ok(Some(value)) = storage.get_setting("indicator_y") {
            if let Ok(y) = value.parse::<i32>() {
                config.indicator_y = y;
            }
        }
        
        if let Ok(Some(value)) = storage.get_setting("auto_hide_indicator") {
            config.auto_hide_indicator = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("sound_feedback") {
            config.sound_feedback = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("auto_start") {
            config.auto_start = value == "true";
        }
        
        if let Ok(Some(value)) = storage.get_setting("silence_timeout_ms") {
            if let Ok(ms) = value.parse::<u64>() {
                config.silence_timeout_ms = ms;
            }
        }
        
        if let Ok(Some(value)) = storage.get_setting("vad_threshold") {
            if let Ok(th) = value.parse::<f32>() {
                config.vad_threshold = th;
            }
        }
        
        if let Ok(Some(value)) = storage.get_setting("skin_id") {
            config.skin_id = value;
        }

        if let Ok(Some(value)) = storage.get_setting("llm_enabled") {
            config.llm_enabled = value == "true";
        }

        if let Ok(Some(value)) = storage.get_setting("persona_id") {
            config.persona_id = value;
        }

        if let Ok(Some(value)) = storage.get_setting("llm_provider_id") {
            config.llm_provider_id = value;
        }

        // 加载指令模式配置
        if let Ok(Some(value)) = storage.get_setting("command_mode_enabled") {
            config.command_mode_enabled = value == "true";
        }

        if let Ok(Some(value)) = storage.get_setting("command_mappings") {
            if let Ok(mappings) = serde_json::from_str::<Vec<CommandMapping>>(&value) {
                config.command_mappings = mappings;
            }
        }

        // 加载SDR配置
        if let Ok(Some(value)) = storage.get_setting("sdr_enabled") {
            config.sdr_enabled = value == "true";
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_device_index") {
            if let Ok(idx) = value.parse::<u32>() {
                config.sdr_device_index = Some(idx);
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_frequency_mhz") {
            if let Ok(freq) = value.parse::<f64>() {
                config.sdr_frequency_mhz = freq;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_gain_db") {
            if let Ok(gain) = value.parse::<i32>() {
                config.sdr_gain_db = gain;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_auto_gain") {
            config.sdr_auto_gain = value == "true";
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_output_device") {
            config.sdr_output_device = value;
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_input_source") {
            if let Ok(source) = serde_json::from_str::<InputSource>(&value) {
                config.sdr_input_source = source;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_demod_mode") {
            if let Ok(mode) = serde_json::from_str::<crate::sdr::DemodMode>(&value) {
                config.sdr_demod_mode = mode;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_ppm_correction") {
            if let Ok(ppm) = value.parse::<i32>() {
                config.sdr_ppm_correction = ppm;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_vad_threshold") {
            if let Ok(th) = value.parse::<f32>() {
                config.sdr_vad_threshold = th;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_ctcss_tone") {
            if let Ok(tone) = value.parse::<f32>() {
                config.sdr_ctcss_tone = tone;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_ctcss_threshold") {
            if let Ok(th) = value.parse::<f32>() {
                config.sdr_ctcss_threshold = th;
            }
        }
        if let Ok(Some(value)) = storage.get_setting("sdr_bandwidth") {
            if let Ok(bw) = value.parse::<u32>() {
                config.sdr_bandwidth = bw;
            }
        }

        Ok(config)
    }
    
    pub fn save(&self, storage: &Storage) -> anyhow::Result<()> {
        storage.set_setting("hotkey_vk", &self.hotkey_vk.to_string())?;
        storage.set_setting("hotkey_name", &self.hotkey_name)?;
        storage.set_setting("audio_device", &self.audio_device.clone().unwrap_or_default())?;
        storage.set_setting("sample_rate", &self.sample_rate.to_string())?;
        storage.set_setting("asr_model", &self.asr_model)?;
        storage.set_setting("use_gpu", &self.use_gpu.to_string())?;
        storage.set_setting("language", &self.language)?;
        storage.set_setting("use_itn", &self.use_itn.to_string())?;
        storage.set_setting("remove_fillers", &self.remove_fillers.to_string())?;
        storage.set_setting("capitalize_sentences", &self.capitalize_sentences.to_string())?;
        storage.set_setting("optimize_spacing", &self.optimize_spacing.to_string())?;
        storage.set_setting("restore_clipboard", &self.restore_clipboard.to_string())?;
        storage.set_setting("paste_delay_ms", &self.paste_delay_ms.to_string())?;
        storage.set_setting("indicator_x", &self.indicator_x.to_string())?;
        storage.set_setting("indicator_y", &self.indicator_y.to_string())?;
        storage.set_setting("auto_hide_indicator", &self.auto_hide_indicator.to_string())?;
        storage.set_setting("sound_feedback", &self.sound_feedback.to_string())?;
        storage.set_setting("auto_start", &self.auto_start.to_string())?;
        storage.set_setting("silence_timeout_ms", &self.silence_timeout_ms.to_string())?;
        storage.set_setting("vad_threshold", &self.vad_threshold.to_string())?;
        storage.set_setting("skin_id", &self.skin_id)?;
        storage.set_setting("llm_enabled", &self.llm_enabled.to_string())?;
        storage.set_setting("persona_id", &self.persona_id)?;
        storage.set_setting("llm_provider_id", &self.llm_provider_id)?;
        storage.set_setting("command_mode_enabled", &self.command_mode_enabled.to_string())?;
        storage.set_setting("command_mappings", &serde_json::to_string(&self.command_mappings)?)?;

        // 保存SDR配置
        storage.set_setting("sdr_enabled", &self.sdr_enabled.to_string())?;
        storage.set_setting("sdr_device_index", &self.sdr_device_index.map(|i| i.to_string()).unwrap_or_default())?;
        storage.set_setting("sdr_frequency_mhz", &self.sdr_frequency_mhz.to_string())?;
        storage.set_setting("sdr_gain_db", &self.sdr_gain_db.to_string())?;
        storage.set_setting("sdr_auto_gain", &self.sdr_auto_gain.to_string())?;
        storage.set_setting("sdr_output_device", &self.sdr_output_device)?;
        storage.set_setting("sdr_input_source", &serde_json::to_string(&self.sdr_input_source)?)?;
        storage.set_setting("sdr_demod_mode", &serde_json::to_string(&self.sdr_demod_mode)?)?;
        storage.set_setting("sdr_ppm_correction", &self.sdr_ppm_correction.to_string())?;
        storage.set_setting("sdr_vad_threshold", &self.sdr_vad_threshold.to_string())?;
        storage.set_setting("sdr_ctcss_tone", &self.sdr_ctcss_tone.to_string())?;
        storage.set_setting("sdr_ctcss_threshold", &self.sdr_ctcss_threshold.to_string())?;
        storage.set_setting("sdr_bandwidth", &self.sdr_bandwidth.to_string())?;

        Ok(())
    }
}
