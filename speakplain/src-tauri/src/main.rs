// 说人话 - AI语音输入法主程序
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod hotkey;
mod input;
mod storage;
mod asr;
mod indicator;
mod config;
mod tray;

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use log::info;

/// 静音自动停止阈值 (RMS)
const SILENCE_THRESHOLD: f32 = 0.05;
/// 连续静音多少秒后自动停止自由说话
const SILENCE_TIMEOUT_SECS: u64 = 3;
/// 自由说话开始后请勿检测静音的 grace period
const SILENCE_GRACE_SECS: u64 = 2;

// 应用状态
pub struct AppState {
    pub recorder: Arc<Mutex<audio::AudioRecorder>>,
    pub hotkey_manager: Arc<Mutex<hotkey::HotkeyManager>>,
    pub input_paster: Arc<Mutex<input::TextPaster>>,
    pub storage: Arc<Mutex<storage::Storage>>,
    pub asr_engine: Arc<Mutex<Option<asr::SenseVoiceEngine>>>,
    pub indicator: Arc<Mutex<indicator::IndicatorWindow>>,
    pub config: Arc<Mutex<config::AppConfig>>,
    /// 当前是否处于自由说话模式
    pub is_freetalk: Arc<AtomicBool>,
    /// 自由说话开始时间（用于 grace period）
    pub freetalk_start: Arc<Mutex<Option<Instant>>>,
    /// 最近一次检测到静音的时间
    pub silence_since: Arc<Mutex<Option<Instant>>>,
    /// 录音开始前的目标窗口 HWND（粘贴时恢复焦点用）
    pub target_hwnd: Arc<Mutex<isize>>,
}

impl AppState {
    pub fn new(
        app_handle: AppHandle,
        hk_is_active:   Arc<std::sync::atomic::AtomicBool>,
        hk_is_freetalk: Arc<std::sync::atomic::AtomicBool>,
        hk_last_stop:   Arc<std::sync::atomic::AtomicU64>,
        hk_is_rec_hk:   Arc<std::sync::atomic::AtomicBool>,
        hk_press_time:  Arc<Mutex<Option<Instant>>>,
    ) -> anyhow::Result<Self> {
        let storage = Arc::new(Mutex::new(storage::Storage::new()?));
        let config  = Arc::new(Mutex::new(config::AppConfig::load(&storage.lock())?));

        let mut hk = hotkey::HotkeyManager::new();
        // 将外部传入的共享 Arc 注入到 HotkeyManager
        hk.is_active          = hk_is_active;
        hk.is_freetalk        = hk_is_freetalk;
        hk.last_stop_ms       = hk_last_stop;
        hk.is_recording_hotkey = hk_is_rec_hk;
        hk.press_time         = hk_press_time;

        Ok(Self {
            recorder: Arc::new(Mutex::new(audio::AudioRecorder::new()?)),
            hotkey_manager: Arc::new(Mutex::new(hk)),
            input_paster: Arc::new(Mutex::new(input::TextPaster::new())),
            storage,
            asr_engine: Arc::new(Mutex::new(None)),
            indicator: Arc::new(Mutex::new(indicator::IndicatorWindow::new(app_handle)?)),
            config,
            is_freetalk: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            freetalk_start: Arc::new(Mutex::new(None)),
            silence_since: Arc::new(Mutex::new(None)),
            target_hwnd: Arc::new(Mutex::new(0)),
        })
    }

    /// 清理资源，在应用退出前调用
    pub fn cleanup(&self) {
        info!("开始清理应用资源...");
        
        // 1. 停止音频录制
        {
            let mut recorder = self.recorder.lock();
            if recorder.is_recording() {
                info!("停止音频录制");
                let _ = recorder.stop();
            }
        }
        
        // 2. 释放 ASR 引擎
        {
            let mut engine = self.asr_engine.lock();
            if engine.is_some() {
                info!("释放 ASR 引擎");
                *engine = None;
            }
        }
        
        // 3. 隐藏指示器窗口
        {
            let indicator = self.indicator.lock();
            indicator.hide();
        }
        
        // 4. 保存配置
        {
            let config = self.config.lock();
            let storage = self.storage.lock();
            if let Err(e) = config.save(&storage) {
                log::warn!("保存配置失败: {}", e);
            }
        }
        
        info!("资源清理完成");
    }
}

// ── 内部共用函数 ──

/// 异步识别并粘贴文本
fn recognize_and_paste(app_handle: AppHandle, audio_data: Vec<f32>) {
    tauri::async_runtime::spawn(async move {
        let state: State<AppState> = app_handle.state();

        // 检测是否有语音内容
        let vad_threshold = state.config.lock().vad_threshold;
        let rms = audio::AudioRecorder::calculate_rms(&audio_data);
        info!("VAD 检测: 样本数={}, RMS={:.6}, 阈值={:.6}", audio_data.len(), rms, vad_threshold);
        // 如果数据根本为空（样本数=0）则跳过
        if audio_data.is_empty() {
            info!("无鼿音活动（样本数为0），跳过识别");
            let indicator = state.indicator.lock();
            indicator.set_no_voice();
            indicator.hide_delayed(1500);
            return;
        }

        // 设置识别中状态
        state.indicator.lock().set_processing();

        // 语音识别（ONNX 推理是阻塞操作，必须放到独立线程避免阻塞异步运行时）
        let text = {
            let engine_arc = state.asr_engine.clone();
            let audio_clone = audio_data.clone();

            // 如果引擎还未初始化，等待最多 300 秒（模型加载可能较慢）
            let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
            loop {
                if engine_arc.lock().is_some() { break; }
                if tokio::time::Instant::now() >= deadline {
                    log::error!("等待 ASR 引擎超时（300s），放弃识别");
                    state.indicator.lock().set_error("引擎超时");
                    state.indicator.lock().hide_delayed(2000);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            match tokio::task::spawn_blocking(move || {
                let engine = engine_arc.lock();
                match *engine {
                    Some(ref e) => e.recognize(&audio_clone),
                    None => Err(anyhow::anyhow!("引擎未初始化")),
                }
            }).await {
                Ok(Ok(t)) => t,
                Ok(Err(err)) => {
                    log::error!("识别失败: {}", err);
                    state.indicator.lock().set_error("错误");
                    state.indicator.lock().hide_delayed(2000);
                    return;
                }
                Err(e) => {
                    log::error!("推理线程 panic: {}", e);
                    state.indicator.lock().set_error("错误");
                    state.indicator.lock().hide_delayed(2000);
                    return;
                }
            }
        };

        if text.is_empty() {
            info!("识别结果为空");
            state.indicator.lock().set_no_voice();
            state.indicator.lock().hide_delayed(1500);
            return;
        }

        // 后处理
        let cfg = state.config.lock().clone();
        let processed = input::TextProcessor::post_process(
            &text,
            cfg.remove_fillers,
            cfg.capitalize_sentences,
        );

        info!("识别结果: {}", processed);

        // 保存历史记录
        {
            let storage = state.storage.lock();
            let _ = storage.add_history(&processed, audio_data.len() as u32 / 16000);
        }

        // 等待热键真正松开再粘贴，防止粘贴到自身（最多等 500ms）
        {
            let deadline = std::time::Instant::now() + Duration::from_millis(500);
            loop {
                if !state.hotkey_manager.lock().is_key_pressed() { break; }
                if std::time::Instant::now() >= deadline { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        tokio::time::sleep(Duration::from_millis(30)).await; // 少量额外延迟确保系统处理完成

        // 粘贴前先把焦点还给录音开始前的窗口
        #[cfg(windows)]
        {
            let hwnd = *state.target_hwnd.lock();
            if hwnd != 0 {
                extern "system" {
                    fn SetForegroundWindow(hwnd: isize) -> i32;
                }
                unsafe { SetForegroundWindow(hwnd); }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        // 粘贴文本
        {
            let mut paster = state.input_paster.lock();
            if let Err(e) = paster.paste(&processed) {
                log::warn!("剩贴板粘贴失败，尝试直接输入: {}", e);
                let _ = paster.type_text(&processed);
            }
        }

        // 完成状态
        state.indicator.lock().set_done();
        let auto_hide = state.config.lock().auto_hide_indicator;
        if auto_hide {
            state.indicator.lock().hide_delayed(2000);
        }

        // 通知前端
        app_handle.emit("recognition:complete", &processed).ok();
    });
}

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    info!("开始录音 (hold-to-talk)");
    // 记录当前前台窗口，粘贴时还给它
    #[cfg(windows)]
    {
        extern "system" { fn GetForegroundWindow() -> isize; }
        *state.target_hwnd.lock() = unsafe { GetForegroundWindow() };
    }
    state.is_freetalk.store(false, Ordering::Relaxed);
    *state.silence_since.lock() = None;

    let indicator = state.indicator.lock();
    indicator.show();
    indicator.set_recording();
    drop(indicator);

    state.recorder.lock().start().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn stop_recording(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    info!("停止录音 (hold-to-talk)");
    let audio_data = state.recorder.lock().stop();
    state.indicator.lock().set_processing();
    recognize_and_paste(app_handle, audio_data);
    Ok(())
}

#[tauri::command]
async fn toggle_freetalk(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    info!("切换自由说话模式");
    // 记录当前前台窗口，粘贴时还给它
    #[cfg(windows)]
    {
        extern "system" { fn GetForegroundWindow() -> isize; }
        *state.target_hwnd.lock() = unsafe { GetForegroundWindow() };
    }
    state.is_freetalk.store(true, Ordering::Relaxed);
    *state.freetalk_start.lock() = Some(Instant::now());
    *state.silence_since.lock() = None;

    let indicator = state.indicator.lock();
    indicator.show();
    indicator.set_freetalk();
    drop(indicator);

    state.recorder.lock().start().map_err(|e| e.to_string())?;

    // 启动录音计时器
    let handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let start = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let s: State<AppState> = handle.state();
            if !s.is_freetalk.load(Ordering::Relaxed) { break; }
            let secs = start.elapsed().as_secs();
            s.indicator.lock().update_timer(secs, true);
        }
    });

    Ok(())
}

#[tauri::command]
async fn stop_freetalk(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    info!("停止自由说话");
    state.is_freetalk.store(false, Ordering::Relaxed);
    *state.freetalk_start.lock() = None;
    *state.silence_since.lock() = None;

    let audio_data = state.recorder.lock().stop();
    state.indicator.lock().set_processing();
    recognize_and_paste(app_handle, audio_data);
    Ok(())
}

#[tauri::command]
async fn cancel_recording(state: State<'_, AppState>) -> Result<(), String> {
    info!("取消录音");
    state.is_freetalk.store(false, Ordering::Relaxed);
    *state.freetalk_start.lock() = None;
    *state.silence_since.lock() = None;

    state.recorder.lock().stop();
    let indicator = state.indicator.lock();
    indicator.set_cancelled();
    indicator.hide_delayed(1000);
    Ok(())
}

/// 当音量回调返回时调用，计算静音超时
#[tauri::command]
async fn on_volume(state: State<'_, AppState>, app_handle: AppHandle, vol: f32) -> Result<(), String> {
    if !state.is_freetalk.load(Ordering::Relaxed) {
        *state.silence_since.lock() = None;
        return Ok(());
    }

    // Grace period内不检测静音
    let in_grace = state.freetalk_start.lock()
        .map_or(true, |t| t.elapsed().as_secs() < SILENCE_GRACE_SECS);
    if in_grace { return Ok(()); }

    if vol > SILENCE_THRESHOLD {
        *state.silence_since.lock() = None;
        return Ok(());
    }

    // 静音计时
    let mut ss = state.silence_since.lock();
    if ss.is_none() {
        *ss = Some(Instant::now());
        return Ok(());
    }
    if ss.unwrap().elapsed().as_secs() >= SILENCE_TIMEOUT_SECS {
        *ss = None;
        drop(ss);
        info!("静音超时，自动停止自由说话");
        state.is_freetalk.store(false, Ordering::Relaxed);
        *state.freetalk_start.lock() = None;
        let audio_data = state.recorder.lock().stop();
        state.indicator.lock().set_processing();
        recognize_and_paste(app_handle, audio_data);
    }
    Ok(())
}

#[tauri::command]
async fn get_history(state: State<'_, AppState>) -> Result<Vec<storage::HistoryItem>, String> {
    state.storage.lock().get_history().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_history_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.storage.lock().delete_history(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.storage.lock().clear_all_history().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<config::AppConfig, String> {
    Ok(state.config.lock().clone())
}

#[tauri::command]
async fn get_skin_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.config.lock().skin_id.clone())
}

#[tauri::command]
async fn save_skin_id(state: State<'_, AppState>, skin_id: String) -> Result<(), String> {
    state.config.lock().skin_id = skin_id.clone();
    state.storage.lock().set_setting("skin_id", &skin_id)
        .map_err(|e| e.to_string())
}

/// 扫描 skins 目录，自动解压 zip 皮肤包，返回所有可用皮肤文件夹名称
#[tauri::command]
async fn scan_skin_folders(app_handle: AppHandle) -> Result<Vec<String>, String> {
    use std::fs;
    use std::path::Path;
    
    // 获取资源目录
    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| e.to_string())?;
    
    // 尝试多个可能的路径（开发模式和发布模式）
    let possible_paths: Vec<std::path::PathBuf> = vec![
        // 开发模式：从 target/debug 往上找到项目根目录
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        // 发布模式：直接在资源目录下
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();
    
    // 使用第一个存在的路径作为 skins 目录
    let skins_dir = possible_paths.iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| resource_dir.join("skins"));
    
    log::info!("扫描皮肤目录: {:?}", skins_dir);
    
    // 确保 skins 目录存在
    if !skins_dir.exists() {
        fs::create_dir_all(&skins_dir).map_err(|e| e.to_string())?;
    }
    
    // 1. 自动解压 skins 目录下的 zip 文件
    if let Ok(entries) = fs::read_dir(&skins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "zip" {
                        // 解压 zip 文件
                        let file_name = path.file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("skin");
                        let target_dir = skins_dir.join(file_name);
                        
                        // 如果目标目录已存在，先删除
                        if target_dir.exists() {
                            let _ = fs::remove_dir_all(&target_dir);
                        }
                        
                        // 解压
                        match unzip_skin_package(&path, &target_dir) {
                            Ok(_) => {
                                log::info!("解压皮肤包成功: {:?}", path);
                                // 解压成功后删除 zip 文件
                                let _ = fs::remove_file(&path);
                            }
                            Err(e) => {
                                log::error!("解压皮肤包失败 {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    // 2. 收集所有包含 skin.json 的文件夹
    let mut skin_ids = Vec::new();
    
    if let Ok(entries) = fs::read_dir(&skins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 检查是否有 skin.json 文件
                let skin_json = path.join("skin.json");
                if skin_json.exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        skin_ids.push(name.to_string());
                    }
                }
            }
        }
    }
    
    Ok(skin_ids)
}

/// 解压皮肤压缩包
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
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
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
async fn save_config(state: State<'_, AppState>, mut new_config: config::AppConfig) -> Result<(), String> {
    // hotkey_name 由后端根据 hotkey_vk 自动补全，前端无需传递
    new_config.hotkey_name = hotkey::vk_to_name(new_config.hotkey_vk);

    // 如果热键改变，更新热键管理器
    {
        let cfg = state.config.lock();
        if cfg.hotkey_vk != new_config.hotkey_vk {
            state.hotkey_manager.lock().set_hotkey(new_config.hotkey_vk);
        }
    }
    *state.config.lock() = new_config.clone();
    new_config.save(&*state.storage.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn init_asr_engine(state: State<'_, AppState>) -> Result<String, String> {
    info!("手动重新初始化ASR引擎");
    let engine = asr::SenseVoiceEngine::new()
        .map_err(|e| format!("初始化失败: {}", e))?;
    let hw_info = engine.hardware_info().to_string();
    *state.asr_engine.lock() = Some(engine);
    Ok(hw_info)
}

#[tauri::command]
async fn list_audio_devices() -> Result<Vec<String>, String> {
    audio::AudioRecorder::list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
async fn show_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().show();
    Ok(())
}

#[tauri::command]
async fn hide_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().hide();
    Ok(())
}

#[tauri::command]
async fn move_indicator(state: State<'_, AppState>, dx: i32, dy: i32) -> Result<(), String> {
    state.indicator.lock().move_by(dx, dy);
    Ok(())
}

#[tauri::command]
async fn drag_indicator(state: State<'_, AppState>) -> Result<(), String> {
    state.indicator.lock().start_drag();
    Ok(())
}

/// 读取皮肤文件内容
#[tauri::command]
async fn read_skin_file(app_handle: AppHandle, skin_id: String, filename: String) -> Result<String, String> {
    use std::fs;
    use std::path::PathBuf;
    
    // 获取资源目录
    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;
    
    log::info!("read_skin_file: skin_id={}, filename={}, resource_dir={:?}", skin_id, filename, resource_dir);
    
    // 尝试多个可能的路径（开发模式和发布模式）
    let possible_paths: Vec<PathBuf> = vec![
        // 开发模式：从 target/debug 往上找到项目根目录
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        // 发布模式：直接在资源目录下
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();
    
    log::info!("可能的皮肤路径: {:?}", possible_paths);
    
    let mut file_path: Option<PathBuf> = None;
    for path in &possible_paths {
        let test_path = path.join(&skin_id).join(&filename);
        log::info!("尝试路径: {:?}, exists={}", test_path, test_path.exists());
        if test_path.exists() {
            file_path = Some(test_path);
            break;
        }
    }
    
    let file_path = file_path.ok_or_else(|| format!("文件不存在: {}/{}", skin_id, filename))?;
    
    fs::read_to_string(&file_path)
        .map_err(|e| format!("读取文件失败: {}", e))
}

/// 读取皮肤背景图片为 base64
#[tauri::command]
async fn read_skin_background_base64(app_handle: AppHandle, skin_id: String) -> Result<String, String> {
    use std::fs;
    use std::path::PathBuf;
    use base64::{Engine as _, engine::general_purpose};
    
    // 获取资源目录
    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;
    
    // 尝试多个可能的路径
    let possible_paths: Vec<PathBuf> = vec![
        resource_dir.parent().and_then(|p| p.parent()).map(|p| p.join("speakplain").join("skins")),
        Some(resource_dir.join("skins")),
    ].into_iter().flatten().collect();
    
    let mut file_path: Option<PathBuf> = None;
    for path in &possible_paths {
        let test_path = path.join(&skin_id).join("background.png");
        log::info!("尝试背景路径: {:?}", test_path);
        if test_path.exists() {
            file_path = Some(test_path);
            break;
        }
    }
    
    let file_path = file_path.ok_or_else(|| format!("背景文件不存在: {}/background.png", skin_id))?;
    
    let bytes = fs::read(&file_path)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    
    Ok(general_purpose::STANDARD.encode(&bytes))
}

fn main() {
    env_logger::init();

    // 提前创建共享状态，供 Builder 阶段的 with_handler 闭包引用
    let hk_is_active    = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hk_is_freetalk  = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hk_last_stop    = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let hk_is_rec_hk    = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hk_press_time: std::sync::Arc<parking_lot::Mutex<Option<std::time::Instant>>>
        = std::sync::Arc::new(parking_lot::Mutex::new(None));

    let shortcut_handler = hotkey::HotkeyManager::make_shortcut_handler(
        hk_is_active.clone(),
        hk_is_freetalk.clone(),
        hk_last_stop.clone(),
        hk_is_rec_hk.clone(),
        hk_press_time.clone(),
    );

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shortcut_handler)
                .build()
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.get_webview_window("main").map(|w| {
                w.set_focus().ok();
                w.show().ok();
            });
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            info!("应用启动中...");

            let handle = app.handle().clone();

            // 拦截主窗口关闭事件：关闭 = 隐藏到托盘，而非退出
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        win.hide().ok();
                    }
                });
            }

            // 初始化应用状态（将共享 Arc 传入，与 Builder 阶段的 handler 共享同一组状态）
            let state = AppState::new(
                handle.clone(),
                hk_is_active,
                hk_is_freetalk,
                hk_last_stop,
                hk_is_rec_hk,
                hk_press_time,
            )?;
            app.manage(state);

            // 创建系统托盘
            tray::create_tray(&handle)?;

            // 启动并显示 indicator 悬浮窗口
            {
                let state: State<AppState> = handle.state();
                state.indicator.lock().startup_show();
            }

            // 监听前端 ready 事件，补发当前状态
            {
                let handle_ready = handle.clone();
                handle.listen("indicator:ready", move |_| {
                    let s: State<AppState> = handle_ready.state();
                    s.indicator.lock().resend_status();
                });
            }

            // 注册音量回调 → 转发到 on_volume
            {
                let state: State<AppState> = handle.state();
                let handle_vol = handle.clone();
                let cb: audio::VolumeCallback = Arc::new(move |vol: f32| {
                    let h = handle_vol.clone();
                    // 实时推送音量到 indicator 窗口（直接调用，无需异步）
                    {
                        let s: State<AppState> = h.state();
                        s.indicator.lock().emit_volume(vol);
                    }
                    // 异步处理静音超时逻辑
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        let _ = on_volume(s, h.clone(), vol).await;
                    });
                });
                state.recorder.lock().set_volume_callback(cb);
            }

            // 初始化热键监听
            {
                let state: State<AppState> = handle.state();
                state.hotkey_manager.lock().init(handle.clone())?;
            }

            // 监听热键事件，调用对应命令
            {
                let handle_hk = handle.clone();
                handle.listen("hotkey:start_recording", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if let Err(e) = start_recording(s).await {
                            log::error!("开始录音失败: {}", e);
                        }
                    });
                });

                let handle_hk = handle.clone();
                handle.listen("hotkey:stop_recording", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if let Err(e) = stop_recording(s, h.clone()).await {
                            log::error!("停止录音失败: {}", e);
                        }
                    });
                });

                let handle_hk = handle.clone();
                handle.listen("hotkey:toggle_freetalk", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if let Err(e) = toggle_freetalk(s, h.clone()).await {
                            log::error!("切换自由说话失败: {}", e);
                        }
                    });
                });

                let handle_hk = handle.clone();
                handle.listen("hotkey:stop_freetalk", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if let Err(e) = stop_freetalk(s, h.clone()).await {
                            log::error!("停止自由说话失败: {}", e);
                        }
                    });
                });

                let handle_hk = handle.clone();
                handle.listen("hotkey:cancel_recording", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if let Err(e) = cancel_recording(s).await {
                            log::error!("取消录音失败: {}", e);
                        }
                    });
                });
            }

            // 异步初始化ASR引擎（使用 spawn_blocking 避免阻塞 tokio 运行时）
            let handle_asr = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<AppState> = handle_asr.state();
                info!("ASR引擎开始加载（在独立线程中）...");
                state.indicator.lock().set_loading();
                match tokio::task::spawn_blocking(|| asr::SenseVoiceEngine::new()).await {
                    Ok(Ok(engine)) => {
                        let hw_info = engine.hardware_info().to_string();
                        *state.asr_engine.lock() = Some(engine);
                        info!("ASR引擎初始化成功: {}", hw_info);
                        state.indicator.lock().set_idle();
                        handle_asr.emit("asr:ready", &hw_info).ok();
                    }
                    Ok(Err(e)) => {
                        log::error!("ASR引擎初始化失败: {}", e);
                        handle_asr.emit("asr:error", e.to_string()).ok();
                    }
                    Err(e) => {
                        log::error!("ASR引擎加载线程 panic: {}", e);
                        handle_asr.emit("asr:error", format!("加载线程崩溃: {}", e)).ok();
                    }
                }
            });

            info!("应用启动完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            toggle_freetalk,
            stop_freetalk,
            cancel_recording,
            on_volume,
            get_history,
            delete_history_item,
            clear_history,
            get_config,
            save_config,
            get_skin_id,
            save_skin_id,
            scan_skin_folders,
            read_skin_file,
            read_skin_background_base64,
            init_asr_engine,
            list_audio_devices,
            show_indicator,
            hide_indicator,
            move_indicator,
            drag_indicator,
        ])
        .run(tauri::generate_context!())
        .expect("运行应用时出错");
}
