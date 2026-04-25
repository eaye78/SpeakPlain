// 说人话 - AI语音输入法主程序
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
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
mod pipeline;
mod commands;
mod app_setup;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use parking_lot::Mutex;

use tauri::Manager;

fn main() {
    // 设置 UTF-8 编码
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::*;
        let _ = SetConsoleCP(65001);
        let _ = SetConsoleOutputCP(65001);
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if let Ok(handle) = handle {
            let mut mode = CONSOLE_MODE(0);
            if GetConsoleMode(handle, &mut mode).is_ok() {
                mode.0 |= ENABLE_VIRTUAL_TERMINAL_PROCESSING.0;
                let _ = SetConsoleMode(handle, mode);
            }
        }
        let stderr_handle = GetStdHandle(STD_ERROR_HANDLE);
        if let Ok(handle) = stderr_handle {
            let mut mode = CONSOLE_MODE(0);
            if GetConsoleMode(handle, &mut mode).is_ok() {
                mode.0 |= ENABLE_VIRTUAL_TERMINAL_PROCESSING.0;
                let _ = SetConsoleMode(handle, mode);
            }
        }
    }

    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
            writeln!(buf, "[{} {} {}] {}", timestamp, record.level(), record.target(), record.args())
        })
        .target(env_logger::Target::Stdout)
        .init();

    let hk_is_active    = Arc::new(AtomicBool::new(false));
    let hk_is_freetalk  = Arc::new(AtomicBool::new(false));
    let hk_last_stop    = Arc::new(AtomicU64::new(0));
    let hk_is_rec_hk    = Arc::new(AtomicBool::new(false));
    let hk_press_time: Arc<Mutex<Option<std::time::Instant>>> = Arc::new(Mutex::new(None));

    let shortcut_handler = hotkey::HotkeyManager::make_shortcut_handler(
        hk_is_active.clone(), hk_is_freetalk.clone(), hk_last_stop.clone(),
        hk_is_rec_hk.clone(), hk_press_time.clone(),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new()
            .with_handler(shortcut_handler).build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.get_webview_window("main").map(|w| { w.set_focus().ok(); w.show().ok(); });
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent, None,
        ))
        .setup(move |app| {
            app_setup::setup_app(
                app,
                hk_is_active, hk_is_freetalk, hk_last_stop, hk_is_rec_hk, hk_press_time,
            ).map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e) as Box<dyn std::error::Error>)
        })
        .invoke_handler(tauri::generate_handler![
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::toggle_freetalk,
            commands::recording::stop_freetalk,
            commands::recording::cancel_recording,
            commands::recording::on_volume,
            commands::history::get_history,
            commands::history::delete_history_item,
            commands::history::clear_history,
            commands::config_cmd::get_config,
            commands::config_cmd::save_config,
            commands::config_cmd::get_skin_id,
            commands::config_cmd::save_skin_id,
            commands::config_cmd::scan_skin_folders,
            commands::config_cmd::read_skin_file,
            commands::config_cmd::read_skin_background_base64,
            commands::asr_cmd::init_asr_engine,
            commands::asr_cmd::get_available_asr_models,
            commands::asr_cmd::switch_asr_model,
            commands::asr_cmd::get_current_asr_model,
            commands::asr_cmd::list_audio_devices,
            commands::indicator_cmd::show_indicator,
            commands::indicator_cmd::hide_indicator,
            commands::indicator_cmd::move_indicator,
            commands::indicator_cmd::drag_indicator,
            commands::llm_cmd::get_personas,
            commands::llm_cmd::save_persona,
            commands::llm_cmd::delete_persona,
            commands::llm_cmd::set_persona,
            commands::llm_cmd::set_llm_enabled,
            commands::llm_cmd::get_llm_providers,
            commands::llm_cmd::save_llm_provider,
            commands::llm_cmd::delete_llm_provider,
            commands::llm_cmd::set_llm_provider,
            commands::llm_cmd::test_llm_provider,
            commands::llm_cmd::get_llm_config,
            commands::llm_cmd::get_llm_provider_defaults,
            command::get_command_mode_enabled,
            command::set_command_mode_enabled,
            command::get_command_mappings,
            command::save_command_mapping,
            command::delete_command_mapping,
            commands::sdr_cmd::sdr_get_devices,
            commands::sdr_cmd::sdr_connect,
            commands::sdr_cmd::sdr_disconnect,
            commands::sdr_cmd::sdr_set_frequency,
            commands::sdr_cmd::sdr_set_gain,
            commands::sdr_cmd::sdr_set_auto_gain,
            commands::sdr_cmd::sdr_set_ppm,
            commands::sdr_cmd::sdr_set_demod_mode,
            commands::sdr_cmd::sdr_set_vad_threshold,
            commands::sdr_cmd::sdr_set_ctcss_tone,
            commands::sdr_cmd::sdr_set_ctcss_threshold,
            commands::sdr_cmd::sdr_set_bandwidth,
            commands::sdr_cmd::sdr_get_status,
            commands::sdr_cmd::sdr_get_signal_strength,
            commands::sdr_cmd::sdr_get_virtual_devices,
            commands::sdr_cmd::sdr_get_all_output_devices,
            commands::sdr_cmd::sdr_set_output_device,
            commands::sdr_cmd::sdr_start_stream,
            commands::sdr_cmd::sdr_stop_stream,
            commands::sdr_cmd::sdr_test_connection,
            commands::sdr_cmd::sdr_test_broadcast,
            commands::sdr_cmd::sdr_trigger_asr,
            commands::sdr_cmd::sdr_set_input_source,
            commands::sdr_cmd::sdr_get_input_source,
            commands::sdr_cmd::sdr_launch_zadig,
            commands::sdr_cmd::sdr_get_channels,
            commands::sdr_cmd::sdr_save_channel,
            commands::sdr_cmd::sdr_delete_channel,
            commands::sdr_cmd::sdr_apply_channel,
        ])
        .run(tauri::generate_context!())
        .expect("运行应用时出错");
}
