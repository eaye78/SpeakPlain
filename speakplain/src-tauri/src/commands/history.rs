// 历史记录命令
use tauri::State;
use crate::app_state::AppState;

#[tauri::command]
pub async fn get_history(state: State<'_, AppState>) -> Result<Vec<crate::storage::HistoryItem>, String> {
    state.storage.lock().get_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_history_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.storage.lock().delete_history(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.storage.lock().clear_all_history().map_err(|e| e.to_string())
}
