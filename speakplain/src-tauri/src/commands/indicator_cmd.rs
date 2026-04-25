// 指示器窗口命令
use tauri::State;
use crate::app_state::AppState;

#[tauri::command]
pub async fn show_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().show();
    Ok(())
}

#[tauri::command]
pub async fn hide_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().hide();
    Ok(())
}

#[tauri::command]
pub async fn move_indicator(state: State<'_, AppState>, dx: i32, dy: i32) -> Result<(), String> {
    state.indicator.lock().move_by(dx, dy);
    Ok(())
}

#[tauri::command]
pub async fn drag_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().start_drag();
    Ok(())
}
