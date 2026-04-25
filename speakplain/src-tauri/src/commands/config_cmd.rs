// 配置与皮肤命令
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<crate::config::AppConfig, String> {
    Ok(state.config.lock().clone())
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, mut new_config: crate::config::AppConfig) -> Result<(), String> {
    new_config.hotkey_name = crate::hotkey::vk_to_name(new_config.hotkey_vk);
    {
        let cfg = state.config.lock();
        if cfg.hotkey_vk != new_config.hotkey_vk {
            state.hotkey_manager.lock().set_hotkey(new_config.hotkey_vk);
        }
        if cfg.audio_device != new_config.audio_device {
            let mut recorder = state.recorder.lock();
            let device_name = new_config.audio_device.as_deref();
            if let Err(e) = recorder.set_device(device_name) {
                log::warn!("切换音频设备失败: {}", e);
            }
        }
    }
    {
        let mut paster = state.input_paster.lock();
        paster.restore_clipboard = new_config.restore_clipboard;
        paster.paste_delay_ms = new_config.paste_delay_ms;
    }
    *state.config.lock() = new_config.clone();
    new_config.save(&*state.storage.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_skin_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.config.lock().skin_id.clone())
}

#[tauri::command]
pub async fn save_skin_id(state: State<'_, AppState>, skin_id: String) -> Result<(), String> {
    state.config.lock().skin_id = skin_id.clone();
    state.storage.lock().set_setting("skin_id", &skin_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_skin_folders(app_handle: AppHandle) -> Result<Vec<String>, String> {
    use std::fs;

    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| e.to_string())?;

    let possible_paths: Vec<PathBuf> = vec![
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();

    let skins_dir = possible_paths.iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| resource_dir.join("skins"));

    if !skins_dir.exists() {
        fs::create_dir_all(&skins_dir).map_err(|e| e.to_string())?;
    }

    if let Ok(entries) = fs::read_dir(&skins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "zip" {
                        let file_name = path.file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("skin");
                        let target_dir = skins_dir.join(file_name);
                        if target_dir.exists() {
                            let _ = fs::remove_dir_all(&target_dir);
                        }
                        match unzip_skin_package(&path, &target_dir) {
                            Ok(_) => { let _ = fs::remove_file(&path); }
                            Err(e) => log::error!("解压皮肤包失败 {:?}: {}", path, e),
                        }
                    }
                }
            }
        }
    }

    let mut skin_ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&skins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("skin.json").exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    skin_ids.push(name.to_string());
                }
            }
        }
    }

    Ok(skin_ids)
}

fn unzip_skin_package(zip_path: &std::path::Path, target_dir: &std::path::Path) -> Result<(), String> {
    use std::io::Read;
    use std::fs::File;
    use std::fs;

    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = target_dir.join(file.name());
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut outfile, &buffer).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn read_skin_file(app_handle: AppHandle, skin_id: String, filename: String) -> Result<String, String> {
    use std::fs;

    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;

    let possible_paths: Vec<PathBuf> = vec![
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();

    let mut file_path: Option<PathBuf> = None;
    for path in &possible_paths {
        let test_path = path.join(&skin_id).join(&filename);
        if test_path.exists() {
            file_path = Some(test_path);
            break;
        }
    }

    let file_path = file_path.ok_or_else(|| format!("文件不存在: {}/{}", skin_id, filename))?;
    fs::read_to_string(&file_path).map_err(|e| format!("读取文件失败: {}", e))
}

#[tauri::command]
pub async fn read_skin_background_base64(app_handle: AppHandle, skin_id: String) -> Result<String, String> {
    use std::fs;
    use base64::{Engine as _, engine::general_purpose};

    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;

    let possible_paths: Vec<PathBuf> = vec![
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();

    let mut file_path: Option<PathBuf> = None;
    for path in &possible_paths {
        let test_path = path.join(&skin_id).join("background.png");
        if test_path.exists() {
            file_path = Some(test_path);
            break;
        }
    }

    let file_path = file_path.ok_or_else(|| format!("背景文件不存在: {}/background.png", skin_id))?;
    let bytes = fs::read(&file_path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(general_purpose::STANDARD.encode(&bytes))
}
