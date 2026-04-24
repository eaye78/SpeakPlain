// 指令模式管理模块
use tauri::State;
use log::debug;
use crate::config::CommandMapping;
use crate::AppState;

/// 获取指令模式开关状态
#[tauri::command]
pub async fn get_command_mode_enabled(
    state: State<'_, AppState>
) -> Result<bool, String> {
    match state.storage.lock().get_setting("command_mode_enabled") {
        Ok(Some(value)) => Ok(value == "true"),
        Ok(None) => Ok(false),
        Err(e) => Err(format!("获取指令模式状态失败: {}", e)),
    }
}

/// 设置指令模式开关状态
#[tauri::command]
pub async fn set_command_mode_enabled(
    enabled: bool,
    state: State<'_, AppState>
) -> Result<(), String> {
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
    state.storage.lock().get_command_mappings()
        .map_err(|e| format!("获取指令映射失败: {}", e))
}

/// 保存指令映射（新增或更新）
#[tauri::command]
pub async fn save_command_mapping(
    mapping: CommandMapping,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 校验指令文字
    if mapping.command_text.is_empty() {
        return Err("指令文字不能为空".to_string());
    }
    if mapping.command_text.len() > 10 {
        return Err("指令文字不能超过10个字符".to_string());
    }
    
    // 加载现有映射
    let mut mappings = state.storage.lock().get_command_mappings()
        .map_err(|e| format!("加载指令映射失败: {}", e))?;
    
    // 检查指令文字是否已存在（更新或新增）
    if let Some(existing) = mappings.iter().position(|m| m.command_text == mapping.command_text) {
        mappings[existing] = mapping;
    } else {
        // 检查是否有其他映射使用相同的按键（可选：允许或禁止）
        mappings.push(mapping);
    }
    
    // 保存到数据库
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
    let mut mappings = state.storage.lock().get_command_mappings()
        .map_err(|e| format!("加载指令映射失败: {}", e))?;
    
    let original_len = mappings.len();
    mappings.retain(|m| m.command_text != command_text);
    
    if mappings.len() == original_len {
        return Err(format!("未找到指令文字 '{}' 的映射", command_text));
    }
    
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

/// 去除常见标点符号
fn remove_punctuation(text: &str) -> String {
    text.chars()
        .filter(|c| !is_punctuation(*c))
        .collect()
}

/// 判断字符是否为标点符号或空白
fn is_punctuation(c: char) -> bool {
    // 覆盖所有 Unicode 空白（含全角空格 \u{3000}）
    if c.is_whitespace() { return true; }
    matches!(c,
        // 中文标点
        '。' | '，' | '、' | '；' | '：' | '？' | '！' | '…' | '—' | '～' | '·' | '｜' |
        '「' | '」' | '『' | '』' | '【' | '】' | '（' | '）' | '《' | '》' | '〈' | '〉' |
        '\u{201c}' | '\u{201d}' | '\u{2018}' | '\u{2019}' | '＂' | '＇' |
        // 英文标点
        '.' | ',' | ';' | ':' | '?' | '!' | '-' | '~' | '|' |
        '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' |
        '"' | '\'' | '`' | '_' | '/' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '+' | '='
    )
}
