use tauri::AppHandle;

#[derive(Debug, Clone, Copy)]
pub enum AudioSource {
    Microphone,
    Sdr,
}

/// Pipeline 上下文：贯穿声音源 → ASR → 指令 → 润色 → 输出的数据载体
pub struct PipelineContext {
    pub app_handle: AppHandle,
    pub audio_samples: Vec<f32>,
    #[allow(dead_code)]
    pub source: AudioSource,
    pub raw_text: String,
    pub processed_text: String,
    pub final_text: String,
    pub llm_refined_text: Option<String>,
    pub llm_success: bool,
    pub command_hit: bool,
    pub duration_sec: u32,
}

impl PipelineContext {
    pub fn new(app_handle: AppHandle, audio_samples: Vec<f32>, source: AudioSource) -> Self {
        Self {
            app_handle,
            audio_samples,
            source,
            raw_text: String::new(),
            processed_text: String::new(),
            final_text: String::new(),
            llm_refined_text: None,
            llm_success: false,
            command_hit: false,
            duration_sec: 0,
        }
    }
}
