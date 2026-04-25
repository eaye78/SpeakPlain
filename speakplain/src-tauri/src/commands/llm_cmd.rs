// LLM 润色命令
use tauri::State;

use crate::app_state::AppState;

#[tauri::command]
pub async fn get_personas(state: State<'_, AppState>) -> Result<Vec<crate::llm::Persona>, String> {
    let builtins = crate::llm::builtin_personas();
    let custom = state.storage.lock().get_custom_personas().map_err(|e| e.to_string())?;
    let custom_ids: std::collections::HashSet<String> = custom.iter().map(|p| p.id.clone()).collect();
    let mut all: Vec<crate::llm::Persona> = builtins.into_iter()
        .filter(|p| !custom_ids.contains(&p.id))
        .collect();
    all.extend(custom);
    Ok(all)
}

#[tauri::command]
pub async fn save_persona(state: State<'_, AppState>, persona: crate::llm::Persona) -> Result<(), String> {
    if persona.is_builtin {
        return Err("内置人设不可直接保存".to_string());
    }
    state.storage.lock().save_persona(&persona).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_persona(state: State<'_, AppState>, persona_id: String) -> Result<(), String> {
    state.storage.lock().delete_persona(&persona_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_persona(state: State<'_, AppState>, persona_id: String) -> Result<(), String> {
    state.config.lock().persona_id = persona_id.clone();
    state.storage.lock().set_setting("persona_id", &persona_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_llm_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.config.lock().llm_enabled = enabled;
    state.storage.lock().set_setting("llm_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_llm_providers(state: State<'_, AppState>) -> Result<Vec<crate::llm::LlmProviderConfig>, String> {
    state.storage.lock().get_llm_providers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_llm_provider(state: State<'_, AppState>, provider: crate::llm::LlmProviderConfig) -> Result<(), String> {
    state.storage.lock().save_llm_provider(&provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_llm_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    let mut cfg = state.config.lock();
    let should_clear = cfg.llm_provider_id == provider_id;
    if should_clear {
        cfg.llm_provider_id = String::new();
    }
    drop(cfg);
    if should_clear {
        state.storage.lock().set_setting("llm_provider_id", "").map_err(|e| e.to_string())?;
    }
    state.storage.lock().delete_llm_provider(&provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_llm_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    state.config.lock().llm_provider_id = provider_id.clone();
    state.storage.lock().set_setting("llm_provider_id", &provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_llm_provider(provider: crate::llm::LlmProviderConfig) -> Result<String, String> {
    crate::llm::test_provider(provider).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = state.config.lock();
    Ok(serde_json::json!({
        "llm_enabled":     cfg.llm_enabled,
        "persona_id":      cfg.persona_id,
        "llm_provider_id": cfg.llm_provider_id,
    }))
}

#[tauri::command]
pub async fn get_llm_provider_defaults(provider_type: String) -> Result<crate::llm::LlmProviderConfig, String> {
    let pt: crate::llm::LlmProviderType = provider_type.parse().map_err(|e: anyhow::Error| e.to_string())?;
    Ok(crate::llm::LlmProviderConfig::default_for(pt))
}
