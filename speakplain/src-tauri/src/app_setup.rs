// 应用启动初始化
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use log::info;

use crate::app_state::AppState;
use crate::asr::{ASREngine, ASRModelType};

pub fn setup_app(
    app: &mut tauri::App,
    hk_is_active: Arc<AtomicBool>,
    hk_is_freetalk: Arc<AtomicBool>,
    hk_last_stop: Arc<AtomicU64>,
    hk_is_rec_hk: Arc<AtomicBool>,
    hk_press_time: Arc<Mutex<Option<Instant>>>,
) -> anyhow::Result<()> {
    info!("应用启动中...");
    let handle = app.handle().clone();

    // 拦截主窗口关闭事件：关闭 = 隐藏到托盘
    if let Some(window) = app.get_webview_window("main") {
        let win = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                win.hide().ok();
            }
        });
    }

    // 初始化应用状态
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
    crate::tray::create_tray(&handle)?;

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

    // 如果配置保存的是 SDR 模式，启动时补注册回调
    {
        let state: State<AppState> = handle.state();
        let is_sdr = state.sdr_manager.lock().get_input_source() == crate::sdr::InputSource::Sdr;
        if is_sdr {
            register_sdr_callbacks(&handle, &state);
            info!("应用启动：SDR模式已恢复，回调已注册");
        }
    }

    // 注册音量回调
    {
        let state: State<AppState> = handle.state();
        let handle_vol = handle.clone();
        let cb: crate::audio::VolumeCallback = Arc::new(move |vol: f32| {
            let h = handle_vol.clone();
            {
                let s: State<AppState> = h.state();
                s.indicator.lock().emit_volume(vol);
            }
            tauri::async_runtime::spawn(async move {
                let s: State<AppState> = h.state();
                let _ = crate::commands::recording::on_volume(s, h.clone(), vol).await;
            });
        });
        state.recorder.lock().set_volume_callback(cb);
    }

    // 初始化热键监听
    {
        let state: State<AppState> = handle.state();
        state.hotkey_manager.lock().init(handle.clone())?;
    }

    // 监听热键事件
    register_hotkey_events(&handle);

    // 异步初始化ASR引擎
    init_asr_async(handle.clone());

    info!("应用启动完成");
    Ok(())
}

fn register_sdr_callbacks(handle: &AppHandle, state: &State<AppState>) {
    {
        let h = handle.clone();
        let signal_cb: Box<dyn Fn(f32) + Send + 'static> = Box::new(move |signal: f32| {
            let s: State<AppState> = h.state();
            s.indicator.lock().emit_volume(signal);
        });
        *state.sdr_manager.lock().on_signal.lock() = Some(signal_cb);
    }
    {
        let h = handle.clone();
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
    }
    {
        let h = handle.clone();
        let cb: Box<dyn Fn(Vec<f32>) + Send + 'static> = Box::new(move |audio_data: Vec<f32>| {
            let handle = h.clone();
            tauri::async_runtime::spawn(async move {
                info!("SDR VAD触发语音段结束，送入ASR，样本数={}", audio_data.len());
                crate::pipeline::run_sdr(handle, audio_data);
            });
        });
        *state.sdr_manager.lock().on_speech_end.lock() = Some(cb);
    }
}

fn register_hotkey_events(handle: &AppHandle) {
    let handle_hk = handle.clone();
    handle.listen("hotkey:start_recording", move |_| {
        let h = handle_hk.clone();
        tauri::async_runtime::spawn(async move {
            let s: State<AppState> = h.state();
            if s.sdr_manager.lock().is_sdr_input() { return; }
            if let Err(e) = crate::commands::recording::start_recording(s).await {
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
            if let Err(e) = crate::commands::recording::stop_recording(s, h.clone()).await {
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
            if let Err(e) = crate::commands::recording::toggle_freetalk(s, h.clone()).await {
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
            if let Err(e) = crate::commands::recording::stop_freetalk(s, h.clone()).await {
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
            if let Err(e) = crate::commands::recording::cancel_recording(s).await {
                log::error!("取消录音失败: {}", e);
            }
        });
    });
}

fn init_asr_async(handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state: State<AppState> = handle.state();
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
                handle.emit("asr:ready", &hw_info).ok();
            }
            Ok(Err(e)) => {
                log::error!("ASR引擎初始化失败: {}", e);
                handle.emit("asr:error", e.to_string()).ok();
            }
            Err(e) => {
                log::error!("ASR引擎加载线程 panic: {}", e);
                handle.emit("asr:error", format!("加载线程崩溃: {}", e)).ok();
            }
        }
    });
}
