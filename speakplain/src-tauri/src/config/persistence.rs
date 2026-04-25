// 配置持久化（加载/保存）

use crate::storage::Storage;
use super::{AppConfig, CommandMapping, SdrChannel};
use crate::sdr::InputSource;

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
            if let Ok(gain) = value.parse::<f32>() {
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
        if let Ok(Some(value)) = storage.get_setting("sdr_channels") {
            if let Ok(channels) = serde_json::from_str::<Vec<SdrChannel>>(&value) {
                config.sdr_channels = channels;
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
        storage.set_setting("sdr_channels", &serde_json::to_string(&self.sdr_channels)?)?;

        Ok(())
    }
}
