// 指令模式管理模块
use tauri::State;
use log::debug;
use crate::config::CommandMapping;
use crate::app_state::AppState;

/// 获取指令模式开关状态
#[tauri::command]
pub async fn get_command_mode_enabled(
    state: State<'_, AppState>
) -> Result<bool, String> {
    Ok(state.config.lock().command_mode_enabled)
}

/// 设置指令模式开关状态
#[tauri::command]
pub async fn set_command_mode_enabled(
    enabled: bool,
    state: State<'_, AppState>
) -> Result<(), String> {
    state.config.lock().command_mode_enabled = enabled;
    state.storage.lock().set_setting("command_mode_enabled", &enabled.to_string())
        .map_err(|e| format!("保存指令模式状态失败: {}", e))?;
    debug!("指令模式已设置为: {}", enabled);
    Ok(())
}

/// 获取所有指令映射
#[tauri::command]
pub async fn get_command_mappings(
    state: State<'_, AppState>
) -> Result<Vec<CommandMapping>, String> {
    Ok(state.config.lock().command_mappings.clone())
}

/// 保存指令映射（新增或更新）
#[tauri::command]
pub async fn save_command_mapping(
    mapping: CommandMapping,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if mapping.command_text.is_empty() {
        return Err("指令文字不能为空".to_string());
    }
    if mapping.command_text.len() > 10 {
        return Err("指令文字不能超过10个字符".to_string());
    }

    let mut cfg = state.config.lock();
    if let Some(existing) = cfg.command_mappings.iter().position(|m| m.command_text == mapping.command_text) {
        cfg.command_mappings[existing] = mapping;
    } else {
        cfg.command_mappings.push(mapping);
    }
    let mappings = cfg.command_mappings.clone();
    drop(cfg);

    state.storage.lock().set_command_mappings(&mappings)
        .map_err(|e| format!("保存指令映射失败: {}", e))?;

    debug!("指令映射已保存，当前共 {} 条", mappings.len());
    Ok(())
}

/// 删除指令映射
#[tauri::command]
pub async fn delete_command_mapping(
    command_text: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cfg = state.config.lock();
    let original_len = cfg.command_mappings.len();
    cfg.command_mappings.retain(|m| m.command_text != command_text);
    if cfg.command_mappings.len() == original_len {
        return Err(format!("未找到指令文字 '{}' 的映射", command_text));
    }
    let mappings = cfg.command_mappings.clone();
    drop(cfg);

    state.storage.lock().set_command_mappings(&mappings)
        .map_err(|e| format!("保存指令映射失败: {}", e))?;

    debug!("指令映射 '{}' 已删除", command_text);
    Ok(())
}

/// 检查是否为指令词（内部使用）
pub fn find_command_mapping<'a>(
    text: &str,
    mappings: &'a [CommandMapping],
) -> Option<&'a CommandMapping> {
    // 去除首尾空格和常见标点符号后再匹配
    let trimmed = text.trim();
    let cleaned = remove_punctuation(trimmed);
    
    mappings.iter().find(|m| {
        let mapping_cleaned = remove_punctuation(&m.command_text);
        mapping_cleaned == cleaned
    })
}

/// 去除所有非字母数字字符（标点、空白、符号等）
/// 基于 Unicode 属性过滤，只保留字母（含中文 CJK）和数字
fn remove_punctuation(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
