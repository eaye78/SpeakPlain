// ASR 引擎命令
use tauri::State;
use log::info;

use crate::app_state::AppState;
use crate::asr::{ASREngine, ASRModelType};

#[tauri::command]
pub async fn init_asr_engine(state: State<'_, AppState>) -> Result<String, String> {
    info!("手动重新初始化ASR引擎");
    let model_type = state.config.lock().asr_model.parse::<ASRModelType>()
        .map_err(|e| format!("无效的模型类型: {}", e))?;
    let engine_arc = state.asr_engine.clone();
    let hw_info = tokio::task::spawn_blocking(move || {
        let engine = ASREngine::new(model_type)
            .map_err(|e| format!("初始化失败: {}", e))?;
        let hw_info = engine.hardware_info();
        *engine_arc.lock() = Some(engine);
        Ok::<String, String>(hw_info)
    }).await.map_err(|e| format!("任务执行失败: {}", e))??;
    Ok(hw_info)
}

#[tauri::command]
pub async fn get_available_asr_models() -> Result<Vec<(String, String, bool)>, String> {
    let models = crate::asr::get_available_models();
    Ok(models.into_iter()
        .map(|(t, name, available)| (t.to_string(), name, available))
        .collect())
}

#[tauri::command]
pub async fn switch_asr_model(state: State<'_, AppState>, model_type: String) -> Result<String, String> {
    info!("切换 ASR 模型到: {}", model_type);
    let model_type = model_type.parse::<ASRModelType>()
        .map_err(|e| format!("无效的模型类型: {}", e))?;

    {
        let mut engine = state.asr_engine.lock();
        *engine = None;
    }

    let engine_arc = state.asr_engine.clone();
    let hw_info = tokio::task::spawn_blocking(move || {
        let new_engine = ASREngine::new(model_type)
            .map_err(|e| format!("切换模型失败: {}", e))?;
        let hw_info = new_engine.hardware_info();
        *engine_arc.lock() = Some(new_engine);
        Ok::<String, String>(hw_info)
    }).await.map_err(|e| format!("任务执行失败: {}", e))??;

    {
        let mut cfg = state.config.lock();
        cfg.asr_model = model_type.to_string();
    }
    if let Err(e) = state.config.lock().save(&*state.storage.lock()) {
        log::warn!("保存模型配置失败: {}", e);
    }

    info!("ASR 模型切换成功: {}", hw_info);
    Ok(hw_info)
}

#[tauri::command]
pub async fn get_current_asr_model(state: State<'_, AppState>) -> Result<(String, String, bool), String> {
    let model_type = state.config.lock().asr_model.parse::<ASRModelType>()
        .unwrap_or(ASRModelType::Qwen3ASR);
    let (name, _desc, available) = crate::asr::get_model_info(model_type);
    Ok((model_type.to_string(), name, available))
}

#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<String>, String> {
    crate::audio::AudioRecorder::list_devices().map_err(|e| e.to_string())
}
