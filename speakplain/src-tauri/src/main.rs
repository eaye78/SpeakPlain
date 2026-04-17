// 说人话 - AI语音输入法主程序
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod hotkey;
mod input;
mod storage;
mod asr;
mod asr_sensevoice;
mod asr_qwen3asr;
mod indicator;
mod config;
mod tray;
mod llm;
mod command;
mod sdr;

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use log::info;
use asr::{ASREngine, ASRModelType};

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
    pub asr_engine: Arc<Mutex<Option<ASREngine>>>,
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
    /// SDR设备管理器
    pub sdr_manager: Arc<Mutex<sdr::SdrManager>>,
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

        // 初始化 SDR 管理器并应用已保存的配置
        let sdr_manager = sdr::SdrManager::new();
        sdr_manager.apply_saved_config(&config.lock());

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
            sdr_manager: Arc::new(Mutex::new(sdr_manager)),
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
        
        // 检查音频数据是否有效
        if audio_data.is_empty() {
            info!("无语音活动（样本数为0），跳过识别");
            let indicator = state.indicator.lock();
            indicator.set_no_voice();
            indicator.hide_delayed(1500);
            return;
        }
        
        // 检查是否有语音活动（RMS 超过阈值）
        if rms < vad_threshold {
            info!("无语音活动（RMS {:.6} < 阈值 {:.6}），跳过识别", rms, vad_threshold);
            let indicator = state.indicator.lock();
            indicator.set_no_voice();
            indicator.hide_delayed(1500);
            return;
        }
        
        info!("检测到语音活动，开始识别...");

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
                match engine.as_ref() {
                    Some(e) => {
                        info!("调用 recognize，样本数: {}", audio_clone.len());
                        let result = e.recognize(&audio_clone);
                        info!("recognize 返回: {:?}", result);
                        result
                    }
                    None => {
                        log::error!("ASR 引擎未初始化");
                        Err(anyhow::anyhow!("引擎未初始化"))
                    }
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

        // ── 指令模式处理 ────────────────────────────────────────────
        // 优先检查指令模式（在 LLM 润色之前）
        // 从 storage 实时读取指令模式配置（因为用户可能动态开关）
        let (command_mode_enabled, command_mappings) = {
            let storage = state.storage.lock();
            let enabled = storage.get_setting("command_mode_enabled")
                .ok()
                .flatten()
                .map(|v| v == "true")
                .unwrap_or(false);
            let mappings = storage.get_command_mappings().ok().unwrap_or_default();
            log::warn!("[指令模式] 状态: enabled={}, mappings_count={}, processed_text='{}'", 
                enabled, mappings.len(), processed);
            (enabled, mappings)
        };
        
        if command_mode_enabled {
            log::warn!("[指令模式] 已开启，检查是否匹配指令...");
            // 检查识别结果是否完全匹配某个指令
            if let Some(mapping) = command::find_command_mapping(&processed, &command_mappings) {
                info!("[指令模式] 匹配到指令: {} -> {:?} + {}", 
                    mapping.command_text, mapping.modifier, mapping.key_name);
                
                // 执行指令对应的按键操作
                if let Err(e) = input::execute_command_mapping(mapping) {
                    log::error!("[指令模式] 执行按键操作失败: {}", e);
                }
                
                // 指令模式下不输入任何文字，直接返回
                state.indicator.lock().set_done();
                let auto_hide = state.config.lock().auto_hide_indicator;
                if auto_hide {
                    state.indicator.lock().hide_delayed(1000);
                }
                app_handle.emit("recognition:complete", "").ok();
                return;
            } else {
                log::warn!("[指令模式] 未匹配到任何指令，继续正常流程");
            }
        } else {
            log::warn!("[指令模式] 未开启，跳过指令检查");
        }

        // ── 说人话 LLM 润色层 ────────────────────────────────────────
        info!("[LLM] 检查条件: llm_enabled={}, llm_provider_id='{}', persona_id='{}'",
            cfg.llm_enabled, cfg.llm_provider_id, cfg.persona_id);

        let (final_text, llm_text_opt, llm_success) = if cfg.llm_enabled && !cfg.llm_provider_id.is_empty() {
            // 读取 provider 配置
            let provider_cfg_opt = {
                let storage = state.storage.lock();
                let all_providers = storage.get_llm_providers().ok().unwrap_or_default();
                info!("[LLM] 数据库中 provider 数量: {}, 查找 id='{}'",
                    all_providers.len(), cfg.llm_provider_id);
                all_providers.into_iter().find(|p| p.id == cfg.llm_provider_id)
            };

            if let Some(provider_cfg) = provider_cfg_opt {
                info!("[LLM] 已找到 provider: name='{}', url='{}', model='{}'",
                    provider_cfg.name, provider_cfg.api_base_url, provider_cfg.model_name);

                // 获取人设
                let persona = {
                    let custom_personas = state.storage.lock().get_custom_personas().unwrap_or_default();
                    info!("[LLM] 自定义人设数量: {}, 查找 persona_id='{}'", custom_personas.len(), cfg.persona_id);
                    let custom_ids: std::collections::HashSet<String> =
                        custom_personas.iter().map(|p| p.id.clone()).collect();
                    let all_personas: Vec<llm::Persona> = llm::builtin_personas().into_iter()
                        .filter(|p| !custom_ids.contains(&p.id))
                        .chain(custom_personas.into_iter())
                        .collect();
                    let found = all_personas.into_iter()
                        .find(|p| p.id == cfg.persona_id)
                        .unwrap_or_else(|| llm::builtin_personas().into_iter().find(|p| p.id == "formal").unwrap());
                    info!("[LLM] 使用人设: id='{}', name='{}'", found.id, found.name);
                    found
                };

                // 显示润色中状态
                state.indicator.lock().set_refining();

                info!("[LLM] 开始调用 do_refine，原文: {:?}", processed);
                match llm::do_refine(&provider_cfg, &persona, &processed).await {
                    Ok(refined) => {
                        info!("[LLM] 润色成功，结果: {:?}", refined);
                        (refined.clone(), Some(refined), true)
                    }
                    Err(e) => {
                        log::warn!("[LLM] 润色失败，降级使用原始文字: {}", e);
                        state.indicator.lock().set_refine_failed("润色失败，已粘贴原文");
                        tokio::time::sleep(Duration::from_millis(800)).await;
                        (processed.clone(), None, false)
                    }
                }
            } else {
                log::warn!("[LLM] Provider 未找到 (id='{}')，降级", cfg.llm_provider_id);
                (processed.clone(), None, false)
            }
        } else {
            info!("[LLM] 未启用或 provider_id 为空，跳过润色");
            (processed.clone(), None, false)
        };

        // 保存历史记录（含 LLM 字段）
        {
            let storage = state.storage.lock();
            let persona_id = if cfg.llm_enabled { Some(cfg.persona_id.as_str()) } else { None };
            let provider_name: Option<String> = if cfg.llm_enabled && !cfg.llm_provider_id.is_empty() {
                storage.get_llm_providers().ok()
                    .and_then(|ps| ps.into_iter().find(|p| p.id == cfg.llm_provider_id))
                    .map(|p| p.name)
            } else {
                None
            };
            let _ = storage.add_history_with_llm(
                &final_text,
                audio_data.len() as u32 / 16000,
                Some(&processed),
                llm_text_opt.as_deref(),
                persona_id,
                provider_name.as_deref(),
                llm_success,
            );
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
            if let Err(e) = paster.paste(&final_text) {
                log::warn!("剩贴板粘贴失败，尝试直接输入: {}", e);
                let _ = paster.type_text(&final_text);
            }
        }

        // 完成状态
        state.indicator.lock().set_done();
        let auto_hide = state.config.lock().auto_hide_indicator;
        if auto_hide {
            state.indicator.lock().hide_delayed(2000);
        }

        // 通知前端
        app_handle.emit("recognition:complete", &final_text).ok();
    });
}

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    info!("开始录音 (hold-to-talk)");
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

/// 获取可用的 ASR 模型列表
#[tauri::command]
async fn get_available_asr_models() -> Result<Vec<(String, String, bool)>, String> {
    let models = asr::get_available_models();
    Ok(models.into_iter()
        .map(|(t, name, available)| (t.to_string(), name, available))
        .collect())
}

/// 切换 ASR 模型
#[tauri::command]
async fn switch_asr_model(state: State<'_, AppState>, model_type: String) -> Result<String, String> {
    info!("切换 ASR 模型到: {}", model_type);
    
    let model_type = model_type.parse::<ASRModelType>()
        .map_err(|e| format!("无效的模型类型: {}", e))?;
    
    // 先释放当前引擎
    {
        let mut engine = state.asr_engine.lock();
        *engine = None;
    }

    // 在独立线程中加载模型（避免阻塞异步运行时）
    let engine_arc = state.asr_engine.clone();
    let hw_info = tokio::task::spawn_blocking(move || {
        let new_engine = ASREngine::new(model_type)
            .map_err(|e| format!("切换模型失败: {}", e))?;
        let hw_info = new_engine.hardware_info();
        *engine_arc.lock() = Some(new_engine);
        Ok::<String, String>(hw_info)
    }).await.map_err(|e| format!("任务执行失败: {}", e))??;

    // 更新内存配置并持久化保存
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

/// 获取当前 ASR 模型信息
#[tauri::command]
async fn get_current_asr_model(state: State<'_, AppState>) -> Result<(String, String, bool), String> {
    let model_type = state.config.lock().asr_model.parse::<ASRModelType>()
        .unwrap_or(ASRModelType::Qwen3ASR);
    let (name, _desc, available) = asr::get_model_info(model_type);
    Ok((model_type.to_string(), name, available))
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

// ── 说人话相关 Tauri 命令 ────────────────────────────────────────────────

/// 获取所有可用人设（内置 + 自定义）
#[tauri::command]
async fn get_personas(state: State<'_, AppState>) -> Result<Vec<llm::Persona>, String> {
    let builtins = llm::builtin_personas();
    let custom = state.storage.lock().get_custom_personas().map_err(|e| e.to_string())?;
    let custom_ids: std::collections::HashSet<String> = custom.iter().map(|p| p.id.clone()).collect();
    let mut all: Vec<llm::Persona> = builtins.into_iter()
        .filter(|p| !custom_ids.contains(&p.id))
        .collect();
    all.extend(custom);
    Ok(all)
}

/// 保存自定义人设
#[tauri::command]
async fn save_persona(state: State<'_, AppState>, persona: llm::Persona) -> Result<(), String> {
    if persona.is_builtin {
        return Err("内置人设不可直接保存，请使用不同 ID的自定义人设覆盖".to_string());
    }
    state.storage.lock().save_persona(&persona).map_err(|e| e.to_string())
}

/// 删除自定义人设
#[tauri::command]
async fn delete_persona(state: State<'_, AppState>, persona_id: String) -> Result<(), String> {
    state.storage.lock().delete_persona(&persona_id).map_err(|e| e.to_string())
}

/// 切换当前人设
#[tauri::command]
async fn set_persona(state: State<'_, AppState>, persona_id: String) -> Result<(), String> {
    state.config.lock().persona_id = persona_id.clone();
    state.storage.lock().set_setting("persona_id", &persona_id).map_err(|e| e.to_string())
}

/// 切换说人话功能开关
#[tauri::command]
async fn set_llm_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.config.lock().llm_enabled = enabled;
    state.storage.lock().set_setting("llm_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

/// 获取所有 LLM Provider 配置
#[tauri::command]
async fn get_llm_providers(state: State<'_, AppState>) -> Result<Vec<llm::LlmProviderConfig>, String> {
    state.storage.lock().get_llm_providers().map_err(|e| e.to_string())
}

/// 保存 LLM Provider 配置
#[tauri::command]
async fn save_llm_provider(state: State<'_, AppState>, provider: llm::LlmProviderConfig) -> Result<(), String> {
    state.storage.lock().save_llm_provider(&provider).map_err(|e| e.to_string())
}

/// 删除 LLM Provider 配置
#[tauri::command]
async fn delete_llm_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    // 如果删除的是当前使用的 provider，清空配置
    let mut cfg = state.config.lock();
    if cfg.llm_provider_id == provider_id {
        cfg.llm_provider_id = String::new();
    }
    drop(cfg);
    state.storage.lock().delete_llm_provider(&provider_id).map_err(|e| e.to_string())
}

/// 切换当前使用的 LLM Provider
#[tauri::command]
async fn set_llm_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    state.config.lock().llm_provider_id = provider_id.clone();
    state.storage.lock().set_setting("llm_provider_id", &provider_id).map_err(|e| e.to_string())
}

/// 测试 LLM Provider 连通性
#[tauri::command]
async fn test_llm_provider(provider: llm::LlmProviderConfig) -> Result<String, String> {
    llm::test_provider(provider).await.map_err(|e| e.to_string())
}

/// 获取说人话功能当前配置
#[tauri::command]
async fn get_llm_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = state.config.lock();
    Ok(serde_json::json!({
        "llm_enabled":     cfg.llm_enabled,
        "persona_id":      cfg.persona_id,
        "llm_provider_id": cfg.llm_provider_id,
    }))
}

/// 获取 Provider 类型的预填默认值
#[tauri::command]
async fn get_llm_provider_defaults(provider_type: String) -> Result<llm::LlmProviderConfig, String> {
    let pt: llm::LlmProviderType = provider_type.parse().map_err(|e: anyhow::Error| e.to_string())?;
    Ok(llm::LlmProviderConfig::default_for(pt))
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

// ── SDR设备相关 Tauri 命令 ────────────────────────────────────────────────

/// 获取SDR设备列表
#[tauri::command]
async fn sdr_get_devices(state: State<'_, AppState>) -> Result<Vec<sdr::SdrDeviceInfo>, String> {
    state.sdr_manager.lock().list_devices().map_err(|e| e.to_string())
}

/// 连接SDR设备（仅连接，不自动启动IQ流）
#[tauri::command]
async fn sdr_connect(state: State<'_, AppState>, device_index: u32) -> Result<(), String> {
    state.sdr_manager.lock().connect(device_index).map_err(|e| e.to_string())?;
    // 注意：连接成功后不自动启动IQ流，需要用户手动点击"开始运行"
    let mut cfg = state.config.lock();
    cfg.sdr_device_index = Some(device_index);
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 断开SDR设备
#[tauri::command]
async fn sdr_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().disconnect().map_err(|e| e.to_string())?;
    // 断开设备后重置悬浮框状态
    let indicator = state.indicator.lock();
    indicator.stop_recording_timer();
    indicator.set_idle();
    indicator.hide();
    Ok(())
}

/// 设置SDR接收频率
#[tauri::command]
async fn sdr_set_frequency(state: State<'_, AppState>, freq_mhz: f64) -> Result<(), String> {
    state.sdr_manager.lock().set_frequency(freq_mhz).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_frequency_mhz = freq_mhz;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置SDR增益
#[tauri::command]
async fn sdr_set_gain(state: State<'_, AppState>, gain_db: i32) -> Result<(), String> {
    state.sdr_manager.lock().set_gain(gain_db).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_gain_db = gain_db;
    cfg.sdr_auto_gain = false;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置SDR自动增益
#[tauri::command]
async fn sdr_set_auto_gain(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.sdr_manager.lock().set_auto_gain(enabled).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_auto_gain = enabled;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置PPM频率校正
#[tauri::command]
async fn sdr_set_ppm(state: State<'_, AppState>, ppm: i32) -> Result<(), String> {
    state.sdr_manager.lock().set_ppm_correction(ppm).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_ppm_correction = ppm;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置解调模式
#[tauri::command]
async fn sdr_set_demod_mode(state: State<'_, AppState>, mode: sdr::DemodMode) -> Result<(), String> {
    state.sdr_manager.lock().set_demod_mode(mode.clone());
    let mut cfg = state.config.lock();
    cfg.sdr_demod_mode = mode;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置VAD邀值
#[tauri::command]
async fn sdr_set_vad_threshold(state: State<'_, AppState>, threshold: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_vad_threshold(threshold);
    let mut cfg = state.config.lock();
    cfg.sdr_vad_threshold = threshold;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置CTCSS亚音频频率
#[tauri::command]
async fn sdr_set_ctcss_tone(state: State<'_, AppState>, tone_hz: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_ctcss_tone(tone_hz);
    let mut cfg = state.config.lock();
    cfg.sdr_ctcss_tone = tone_hz;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 设置CTCSS检测门限
#[tauri::command]
async fn sdr_set_ctcss_threshold(state: State<'_, AppState>, threshold: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_ctcss_threshold(threshold);
    let mut cfg = state.config.lock();
    cfg.sdr_ctcss_threshold = threshold;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 获取SDR设备状态
#[tauri::command]
async fn sdr_get_status(state: State<'_, AppState>) -> Result<sdr::SdrStatus, String> {
    Ok(state.sdr_manager.lock().get_status())
}

/// 获取虚拟音频设备列表
#[tauri::command]
async fn sdr_get_virtual_devices() -> Result<Vec<String>, String> {
    sdr::SdrManager::list_virtual_devices().map_err(|e| e.to_string())
}

/// 获取所有音频输出设备（此接口替代旧的虚拟设备接口）
#[tauri::command]
async fn sdr_get_all_output_devices() -> Result<Vec<String>, String> {
    sdr::SdrManager::list_virtual_devices().map_err(|e| e.to_string())
}

/// 设置SDR输出设备
#[tauri::command]
async fn sdr_set_output_device(state: State<'_, AppState>, device_name: String) -> Result<(), String> {
    state.sdr_manager.lock().set_output_device(device_name.clone()).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_output_device = device_name;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

/// 启动SDR音频流
#[tauri::command]
async fn sdr_start_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().start_stream().map_err(|e| e.to_string())
}

/// 停止SDR音频流
#[tauri::command]
async fn sdr_stop_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().stop_stream().map_err(|e| e.to_string())
}

/// 测试SDR设备连接
#[tauri::command]
async fn sdr_test_connection(state: State<'_, AppState>) -> Result<sdr::TestResult, String> {
    state.sdr_manager.lock().test_connection().map_err(|e| e.to_string())
}

/// 通话测试：自动连接设备、启动音频流、仅播放音频，不触发 ASR
#[tauri::command]
async fn sdr_call_test_start(state: State<'_, AppState>, device_index: u32) -> Result<(), String> {
    // 标记进入通话测试模式
    state.sdr_manager.lock().call_test_mode.store(true, std::sync::atomic::Ordering::Relaxed);
    // 若未连接则先连接
    let connected = state.sdr_manager.lock().get_status().connected;
    if !connected {
        state.sdr_manager.lock().connect(device_index).map_err(|e| e.to_string())?;
    }
    // 若流未启动则启动
    let streaming = state.sdr_manager.lock().get_status().streaming;
    if !streaming {
        state.sdr_manager.lock().start_stream().map_err(|e| e.to_string())?;
    }
    log::info!("SDR通话测试已启动，设备索引={}", device_index);
    Ok(())
}

/// 停止通话测试
#[tauri::command]
async fn sdr_call_test_stop(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().call_test_mode.store(false, std::sync::atomic::Ordering::Relaxed);
    state.sdr_manager.lock().stop_stream().map_err(|e| e.to_string())?;
    log::info!("SDR通话测试已停止");
    Ok(())
}

/// 获取SDR音频缓冲并送入ASR识别
/// 只有在SDR模式下（InputSource::Sdr）且音频缓冲非空时才触发ASR
#[tauri::command]
async fn sdr_trigger_asr(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    if !state.sdr_manager.lock().is_sdr_input() {
        return Err("当前输入源不是SDR模式".to_string());
    }
    let audio_data = state.sdr_manager.lock().take_audio_buffer();
    if audio_data.is_empty() {
        return Err("SDR音频缓冲为空，请确保已开启音频流且有语音输入".to_string());
    }
    info!("SDR ASR触发：音频样本数={}", audio_data.len());
    recognize_and_paste(app_handle, audio_data);
    Ok(())
}

/// 读取 rtl_sdr 进程日志（排查硬件问题用）
#[tauri::command]
async fn sdr_get_rtlsdr_log() -> Result<String, String> {
    Ok(sdr::get_rtlsdr_log(60))
}

/// 获取 rtl_sdr 日志文件路径
#[tauri::command]
async fn sdr_get_rtlsdr_log_path() -> Result<String, String> {
    let p = std::env::temp_dir().join("speakplain_rtlsdr.log");
    Ok(p.to_string_lossy().to_string())
}

/// 实时信号强度查询（前端轮询使用）
#[tauri::command]
async fn sdr_get_signal_strength(state: State<'_, AppState>) -> Result<f32, String> {
    Ok(state.sdr_manager.lock().get_signal_strength())
}

/// 设置输入源：切到 SDR 时自动启动音频流并注册 VAD 回调；切回麦克风时停流
#[tauri::command]
async fn sdr_set_input_source(state: State<'_, AppState>, app_handle: AppHandle, source: sdr::InputSource) -> Result<(), String> {
    let prev_source = state.sdr_manager.lock().get_input_source();
    state.sdr_manager.lock().set_input_source(source.clone());
    {
        let mut cfg = state.config.lock();
        cfg.sdr_input_source = source.clone();
        let _ = cfg.save(&*state.storage.lock());
    }

    match source {
        sdr::InputSource::Sdr => {
            // SDR 模式：禁用热键，防止麦克风录音被意外触发
            state.hotkey_manager.lock().set_recording_hotkey(true);
            {
                let h = app_handle.clone();
                let signal_cb: Box<dyn Fn(f32) + Send + 'static> = Box::new(move |signal: f32| {
                    let s: State<AppState> = h.state();
                    s.indicator.lock().emit_volume(signal);
                });
                *state.sdr_manager.lock().on_signal.lock() = Some(signal_cb);
            }

            // 注册 VAD 状态变化回调 → 控制 indicator 录音/静音状态
            {
                let h = app_handle.clone();
                let vad_cb: Box<dyn Fn(bool) + Send + 'static> = Box::new(move |has_voice: bool| {
                    let s: State<AppState> = h.state();
                    if has_voice {
                        // 手台PTT按下（检测到语音）→ 显示接收中状态（不启动计时线程，避免重发事件覆盖processing）
                        let indicator = s.indicator.lock();
                        indicator.show();
                        indicator.set_sdr_receiving();
                    } else {
                        // 手台PTT松开（信号彻底消失）→ 切换到处理中状态
                        s.indicator.lock().set_processing();
                    }
                });
                *state.sdr_manager.lock().on_vad_change.lock() = Some(vad_cb);
            }

            // 当切到 SDR 模式：注册语音段结束回调，自动启动流
            let h = app_handle.clone();
            let cb: Box<dyn Fn(Vec<f32>) + Send + 'static> = Box::new(move |audio_data: Vec<f32>| {
                let handle = h.clone();
                tauri::async_runtime::spawn(async move {
                    info!("SDR VAD触发语音段结束，送入ASR，样本数={}", audio_data.len());
                    recognize_and_paste(handle, audio_data);
                });
            });
            *state.sdr_manager.lock().on_speech_end.lock() = Some(cb);

            // 如果设备已连接且流未开启，自动启动
            let (connected, streaming) = {
                let st = state.sdr_manager.lock().get_status();
                (st.connected, st.streaming)
            };
            if connected && !streaming {
                info!("SDR模式已切换，自动启动音频流");
                state.sdr_manager.lock().start_stream().map_err(|e| e.to_string())?;
            }
        }
        sdr::InputSource::Microphone => {
            // 切回麦克风：恢复热键，清除所有SDR回调并停流
            state.hotkey_manager.lock().set_recording_hotkey(false);
            *state.sdr_manager.lock().on_speech_end.lock() = None;
            *state.sdr_manager.lock().on_signal.lock() = None;
            *state.sdr_manager.lock().on_vad_change.lock() = None;
            if prev_source == sdr::InputSource::Sdr {
                let streaming = state.sdr_manager.lock().get_status().streaming;
                if streaming {
                    info!("已切换回麦克风，自动停止SDR音频流");
                    state.sdr_manager.lock().stop_stream().map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

/// 获取输入源
#[tauri::command]
async fn sdr_get_input_source(state: State<'_, AppState>) -> Result<sdr::InputSource, String> {
    Ok(state.sdr_manager.lock().get_input_source())
}

/// 启动 Zadig 驱动安装工具（以管理员权限运行，等待其退出后返回）
#[tauri::command]
async fn sdr_launch_zadig(app_handle: AppHandle) -> Result<(), String> {
    // 查找 Zadig 的位置：优先与 exe 同目录，其次尝试资源路径
    let zadig_path = {
        let exe_dir = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("无法获取exe目录".to_string())?
            .to_path_buf();
        let candidate = exe_dir.join("zadig.exe");
        if candidate.exists() {
            candidate
        } else {
            let res_path = app_handle.path().resource_dir()
                .map_err(|e| e.to_string())?
                .join("zadig.exe");
            if res_path.exists() {
                res_path
            } else {
                return Err("未找到 zadig.exe，请确认文件已存在于应用目录".to_string());
            }
        }
    };

    info!("启动 Zadig: {}", zadig_path.display());

    // 以管理员权限启动 Zadig（-Wait 使 PowerShell 等待 Zadig 退出后才返回）
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -Verb RunAs -Wait",
                zadig_path.to_string_lossy()
            ),
        ])
        .status()
        .map_err(|e| format!("启动失败: {}", e))?;

    if status.success() {
        info!("Zadig 已退出");
        Ok(())
    } else {
        Err("用户取消了操作或 Zadig 启动失败".to_string())
    }
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

            // 监听热键事件，调用对应命令（SDR模式下热键全部忧略）
            {
                let handle_hk = handle.clone();
                handle.listen("hotkey:start_recording", move |_| {
                    let h = handle_hk.clone();
                    tauri::async_runtime::spawn(async move {
                        let s: State<AppState> = h.state();
                        if s.sdr_manager.lock().is_sdr_input() { return; }
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
                        if s.sdr_manager.lock().is_sdr_input() { return; }
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
                        if s.sdr_manager.lock().is_sdr_input() { return; }
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
                        if s.sdr_manager.lock().is_sdr_input() { return; }
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
                        if s.sdr_manager.lock().is_sdr_input() { return; }
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
                
                // 获取配置的模型类型，默认使用 Qwen3-ASR
                let model_type = state.config.lock().asr_model.parse::<ASRModelType>()
                    .unwrap_or(ASRModelType::Qwen3ASR);
                
                info!("ASR引擎开始加载（在独立线程中）: {:?}...", model_type);
                state.indicator.lock().set_loading();
                
                match tokio::task::spawn_blocking(move || ASREngine::new(model_type)).await {
                    Ok(Ok(engine)) => {
                        let hw_info = engine.hardware_info();
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
            get_available_asr_models,
            switch_asr_model,
            get_current_asr_model,
            list_audio_devices,
            show_indicator,
            hide_indicator,
            move_indicator,
            drag_indicator,
            // 说人话功能
            get_personas,
            save_persona,
            delete_persona,
            set_persona,
            set_llm_enabled,
            get_llm_providers,
            save_llm_provider,
            delete_llm_provider,
            set_llm_provider,
            test_llm_provider,
            get_llm_config,
            get_llm_provider_defaults,
            // 指令模式功能
            command::get_command_mode_enabled,
            command::set_command_mode_enabled,
            command::get_command_mappings,
            command::save_command_mapping,
            command::delete_command_mapping,
            // SDR设备功能
            sdr_get_devices,
            sdr_connect,
            sdr_disconnect,
            sdr_set_frequency,
            sdr_set_gain,
            sdr_set_auto_gain,
            sdr_set_ppm,
            sdr_set_demod_mode,
            sdr_set_vad_threshold,
            sdr_set_ctcss_tone,
            sdr_set_ctcss_threshold,
            sdr_get_status,
            sdr_get_signal_strength,
            sdr_get_virtual_devices,
            sdr_get_all_output_devices,
            sdr_set_output_device,
            sdr_start_stream,
            sdr_stop_stream,
            sdr_test_connection,
            sdr_call_test_start,
            sdr_call_test_stop,
            sdr_trigger_asr,
            sdr_get_rtlsdr_log,
            sdr_get_rtlsdr_log_path,
            sdr_set_input_source,
            sdr_get_input_source,
            sdr_launch_zadig,
        ])
        .run(tauri::generate_context!())
        .expect("运行应用时出错");
}
