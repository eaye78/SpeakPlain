// 应用配置模块
use serde::{Serialize, Deserialize};
use crate::storage::Storage;
use crate::hotkey::key_codes;

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

        Ok(())
    }
}
