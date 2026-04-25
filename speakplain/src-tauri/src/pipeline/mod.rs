// 语音识别处理管道：声音源 → ASR → 指令模式 → LLM润色 → 文字输出
pub mod context;
pub mod stages;

use tauri::AppHandle;
use log::info;
pub use context::{AudioSource, PipelineContext};

/// 静音自动停止阈值 (RMS)
pub const SILENCE_THRESHOLD: f32 = 0.05;
/// 连续静音多少秒后自动停止自由说话
pub const SILENCE_TIMEOUT_SECS: u64 = 3;
/// 自由说话开始后请勿检测静音的 grace period
pub const SILENCE_GRACE_SECS: u64 = 2;

/// 麦克风路径入口
pub fn run_microphone(app_handle: AppHandle, audio_data: Vec<f32>) {
    run_pipeline(app_handle, audio_data, AudioSource::Microphone);
}

/// SDR 路径入口：先降采样到 16kHz 再送入 Pipeline
pub fn run_sdr(app_handle: AppHandle, audio_data: Vec<f32>) {
    let resampled = sdr_resample_to_16k(&audio_data);
    info!("SDR音频降采样: 48kHz → 16kHz, {}样本 → {}样本", audio_data.len(), resampled.len());
    run_pipeline(app_handle, resampled, AudioSource::Sdr);
}

fn run_pipeline(app_handle: AppHandle, audio_data: Vec<f32>, source: AudioSource) {
    tauri::async_runtime::spawn(async move {
        let mut ctx = PipelineContext::new(app_handle, audio_data, source);
        if let Err(e) = stages::run_asr(&mut ctx).await {
            log::error!("[Pipeline] ASR 阶段失败: {}", e);
            return;
        }
        if stages::run_command(&mut ctx).await {
            return;
        }
        stages::run_refine(&mut ctx).await;
        if let Err(e) = stages::run_output(&mut ctx).await {
            log::error!("[Pipeline] 输出阶段失败: {}", e);
        }
    });
}

/// SDR 音频降采样：48000Hz → 16000Hz（3:1 平均降采样）
fn sdr_resample_to_16k(input: &[f32]) -> Vec<f32> {
    const RATIO: usize = 3;
    let out_len = input.len() / RATIO;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let sum = input[i * RATIO] + input[i * RATIO + 1] + input[i * RATIO + 2];
        out.push(sum / RATIO as f32);
    }
    out
}
