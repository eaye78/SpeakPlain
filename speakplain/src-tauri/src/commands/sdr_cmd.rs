// SDR 设备命令
use tauri::{AppHandle, Manager, State};
use log::info;

use crate::app_state::AppState;
use crate::pipeline::run_microphone;

#[tauri::command]
pub async fn sdr_get_devices(state: State<'_, AppState>) -> Result<Vec<crate::sdr::SdrDeviceInfo>, String> {
    state.sdr_manager.lock().list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_connect(state: State<'_, AppState>, device_index: u32) -> Result<(), String> {
    state.sdr_manager.lock().connect(device_index).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_device_index = Some(device_index);
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().disconnect().map_err(|e| e.to_string())?;
    let indicator = state.indicator.lock();
    indicator.stop_recording_timer();
    indicator.set_idle();
    indicator.hide();
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_frequency(state: State<'_, AppState>, freq_mhz: f64) -> Result<(), String> {
    state.sdr_manager.lock().set_frequency(freq_mhz).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_frequency_mhz = freq_mhz;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_gain(state: State<'_, AppState>, gain_db: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_gain(gain_db).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_gain_db = gain_db;
    cfg.sdr_auto_gain = false;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_auto_gain(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.sdr_manager.lock().set_auto_gain(enabled).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_auto_gain = enabled;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_ppm(state: State<'_, AppState>, ppm: i32) -> Result<(), String> {
    state.sdr_manager.lock().set_ppm_correction(ppm).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_ppm_correction = ppm;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_demod_mode(state: State<'_, AppState>, mode: crate::sdr::DemodMode) -> Result<(), String> {
    state.sdr_manager.lock().set_demod_mode(mode.clone());
    let mut cfg = state.config.lock();
    cfg.sdr_demod_mode = mode;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_vad_threshold(state: State<'_, AppState>, threshold: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_vad_threshold(threshold);
    let mut cfg = state.config.lock();
    cfg.sdr_vad_threshold = threshold;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_ctcss_tone(state: State<'_, AppState>, tone_hz: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_ctcss_tone(tone_hz);
    let mut cfg = state.config.lock();
    cfg.sdr_ctcss_tone = tone_hz;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_ctcss_threshold(state: State<'_, AppState>, threshold: f32) -> Result<(), String> {
    state.sdr_manager.lock().set_ctcss_threshold(threshold);
    let mut cfg = state.config.lock();
    cfg.sdr_ctcss_threshold = threshold;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_set_bandwidth(state: State<'_, AppState>, bandwidth: u32) -> Result<(), String> {
    state.sdr_manager.lock().set_bandwidth(bandwidth).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_bandwidth = bandwidth;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_get_status(state: State<'_, AppState>) -> Result<crate::sdr::SdrStatus, String> {
    Ok(state.sdr_manager.lock().get_status())
}

#[tauri::command]
pub async fn sdr_get_virtual_devices() -> Result<Vec<String>, String> {
    crate::sdr::SdrManager::list_virtual_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_get_all_output_devices() -> Result<Vec<String>, String> {
    crate::sdr::SdrManager::list_virtual_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_set_output_device(state: State<'_, AppState>, device_name: String) -> Result<(), String> {
    state.sdr_manager.lock().set_output_device(device_name.clone()).map_err(|e| e.to_string())?;
    let mut cfg = state.config.lock();
    cfg.sdr_output_device = device_name;
    let _ = cfg.save(&*state.storage.lock());
    Ok(())
}

#[tauri::command]
pub async fn sdr_start_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().start_stream().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_stop_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.sdr_manager.lock().stop_stream().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_test_connection(state: State<'_, AppState>) -> Result<crate::sdr::TestResult, String> {
    state.sdr_manager.lock().test_connection().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_test_broadcast(state: State<'_, AppState>, freq_mhz: f64) -> Result<(), String> {
    crate::sdr::broadcast::run_broadcast_test(&state.sdr_manager.lock(), freq_mhz).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_trigger_asr(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    if !state.sdr_manager.lock().is_sdr_input() {
        return Err("当前输入源不是SDR模式".to_string());
    }
    let audio_data = state.sdr_manager.lock().take_audio_buffer();
    if audio_data.is_empty() {
        return Err("SDR音频缓冲为空".to_string());
    }
    info!("SDR ASR触发：音频样本数={}", audio_data.len());
    run_microphone(app_handle, audio_data);
    Ok(())
}

#[tauri::command]
pub async fn sdr_get_signal_strength(state: State<'_, AppState>) -> Result<f32, String> {
    Ok(state.sdr_manager.lock().get_signal_strength())
}

#[tauri::command]
pub async fn sdr_set_input_source(state: State<'_, AppState>, app_handle: AppHandle, source: crate::sdr::InputSource) -> Result<(), String> {
    let prev_source = state.sdr_manager.lock().get_input_source();
    state.sdr_manager.lock().set_input_source(source.clone());
    {
        let mut cfg = state.config.lock();
        cfg.sdr_input_source = source.clone();
        let _ = cfg.save(&*state.storage.lock());
    }

    match source {
        crate::sdr::InputSource::Sdr => {
            state.hotkey_manager.lock().set_recording_hotkey(true);
            {
                let h = app_handle.clone();
                let signal_cb: Box<dyn Fn(f32) + Send + 'static> = Box::new(move |signal: f32| {
                    let s: State<AppState> = h.state();
                    s.indicator.lock().emit_volume(signal);
                });
                *state.sdr_manager.lock().on_signal.lock() = Some(signal_cb);
            }
            {
                let h = app_handle.clone();
                let vad_cb: Box<dyn Fn(bool) + Send + 'static> = Box::new(move |has_voice: bool| {
                    let s: State<AppState> = h.state();
                    if has_voice {
                        let indicator = s.indicator.lock();
                        indicator.show();
                        indicator.set_sdr_receiving();
                    } else {
                        s.indicator.lock().set_processing();
                    }
                });
                *state.sdr_manager.lock().on_vad_change.lock() = Some(vad_cb);
                let already_active = state.sdr_manager.lock().get_status().vad_active;
                if already_active {
                    let indicator = state.indicator.lock();
                    indicator.show();
                    indicator.set_sdr_receiving();
                }
            }
            let h = app_handle.clone();
            let cb: Box<dyn Fn(Vec<f32>) + Send + 'static> = Box::new(move |audio_data: Vec<f32>| {
                let handle = h.clone();
                tauri::async_runtime::spawn(async move {
                    info!("SDR VAD触发语音段结束，送入ASR，样本数={}", audio_data.len());
                    crate::pipeline::run_sdr(handle, audio_data);
                });
            });
            *state.sdr_manager.lock().on_speech_end.lock() = Some(cb);

            let (connected, streaming) = {
                let st = state.sdr_manager.lock().get_status();
                (st.connected, st.streaming)
            };
            if connected && !streaming {
                info!("SDR模式已切换，自动启动音频流");
                state.sdr_manager.lock().start_stream().map_err(|e| e.to_string())?;
            }
        }
        crate::sdr::InputSource::Microphone => {
            state.hotkey_manager.lock().set_recording_hotkey(false);
            *state.sdr_manager.lock().on_speech_end.lock() = None;
            *state.sdr_manager.lock().on_signal.lock() = None;
            *state.sdr_manager.lock().on_vad_change.lock() = None;
            if prev_source == crate::sdr::InputSource::Sdr {
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

#[tauri::command]
pub async fn sdr_get_input_source(state: State<'_, AppState>) -> Result<crate::sdr::InputSource, String> {
    Ok(state.sdr_manager.lock().get_input_source())
}

#[tauri::command]
pub async fn sdr_launch_zadig(app_handle: AppHandle) -> Result<(), String> {
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
            if res_path.exists() { res_path } else {
                return Err("未找到 zadig.exe".to_string());
            }
        }
    };

    info!("启动 Zadig: {}", zadig_path.display());

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-WindowStyle", "Hidden",
        "-Command",
        &format!("Start-Process -FilePath '{}' -Verb RunAs -Wait -WindowStyle Hidden", zadig_path.to_string_lossy()),
    ]);

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let status = cmd.status().map_err(|e| format!("启动失败: {}", e))?;
    if status.success() {
        info!("Zadig 已退出");
        Ok(())
    } else {
        Err("用户取消了操作或 Zadig 启动失败".to_string())
    }
}

#[tauri::command]
pub async fn sdr_get_channels(state: State<'_, AppState>) -> Result<Vec<crate::config::SdrChannel>, String> {
    Ok(state.config.lock().sdr_channels.clone())
}

#[tauri::command]
pub async fn sdr_save_channel(state: State<'_, AppState>, channel: crate::config::SdrChannel) -> Result<(), String> {
    let mut cfg = state.config.lock();
    let mut ch = channel;
    ch.frequency_mhz = (ch.frequency_mhz * 1000.0).round() / 1000.0;
    if let Some(existing) = cfg.sdr_channels.iter_mut().find(|c| c.id == ch.id) {
        *existing = ch;
    } else {
        if ch.id.is_empty() {
            ch.id = format!("ch_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
        }
        cfg.sdr_channels.push(ch);
    }
    cfg.save(&*state.storage.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_delete_channel(state: State<'_, AppState>, channel_id: String) -> Result<(), String> {
    let mut cfg = state.config.lock();
    cfg.sdr_channels.retain(|c| c.id != channel_id);
    cfg.save(&*state.storage.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sdr_apply_channel(state: State<'_, AppState>, channel_id: String) -> Result<(), String> {
    let channel = {
        let cfg = state.config.lock();
        cfg.sdr_channels.iter().find(|c| c.id == channel_id).cloned()
    };
    let ch = channel.ok_or_else(|| "未找到频道".to_string())?;
    state.sdr_manager.lock().set_frequency(ch.frequency_mhz).map_err(|e| e.to_string())?;
    state.sdr_manager.lock().set_ctcss_tone(ch.ctcss_tone);
    let mut cfg = state.config.lock();
    cfg.sdr_frequency_mhz = ch.frequency_mhz;
    cfg.sdr_ctcss_tone = ch.ctcss_tone;
    cfg.save(&*state.storage.lock()).map_err(|e| e.to_string())
}
