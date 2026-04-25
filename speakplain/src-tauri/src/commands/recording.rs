// 录音控制命令
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use log::info;

use crate::app_state::AppState;
use crate::pipeline::run_microphone;

#[tauri::command]
pub async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    if state.config.lock().sound_feedback {
        crate::audio::play_sound_feedback();
    }
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
pub async fn stop_recording(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    info!("停止录音 (hold-to-talk)");
    let audio_data = state.recorder.lock().stop();
    state.indicator.lock().set_processing();
    run_microphone(app_handle, audio_data);
    Ok(())
}

#[tauri::command]
pub async fn toggle_freetalk(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    if state.config.lock().sound_feedback {
        crate::audio::play_sound_feedback();
    }
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
pub async fn stop_freetalk(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    if state.config.lock().sound_feedback {
        crate::audio::play_sound_feedback();
    }
    info!("停止自由说话");
    state.is_freetalk.store(false, Ordering::Relaxed);
    *state.freetalk_start.lock() = None;
    *state.silence_since.lock() = None;

    let audio_data = state.recorder.lock().stop();
    state.indicator.lock().set_processing();
    run_microphone(app_handle, audio_data);
    Ok(())
}

#[tauri::command]
pub async fn cancel_recording(state: State<'_, AppState>) -> Result<(), String> {
    if state.config.lock().sound_feedback {
        crate::audio::play_sound_feedback();
    }
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

#[tauri::command]
pub async fn on_volume(state: State<'_, AppState>, app_handle: AppHandle, vol: f32) -> Result<(), String> {
    use crate::pipeline::{SILENCE_THRESHOLD, SILENCE_TIMEOUT_SECS, SILENCE_GRACE_SECS};

    if !state.is_freetalk.load(Ordering::Relaxed) {
        *state.silence_since.lock() = None;
        return Ok(());
    }

    let in_grace = state.freetalk_start.lock()
        .map_or(true, |t| t.elapsed().as_secs() < SILENCE_GRACE_SECS);
    if in_grace { return Ok(()); }

    if vol > SILENCE_THRESHOLD {
        *state.silence_since.lock() = None;
        return Ok(());
    }

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
        run_microphone(app_handle, audio_data);
    }
    Ok(())
}
