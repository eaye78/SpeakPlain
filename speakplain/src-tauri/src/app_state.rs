// 应用全局状态
use std::sync::Arc;
use std::time::Instant;
use std::sync::atomic::AtomicBool;
use parking_lot::Mutex;
use tauri::AppHandle;
use log::info;

use crate::asr::ASREngine;

pub struct AppState {
    pub recorder: Arc<Mutex<crate::audio::AudioRecorder>>,
    pub hotkey_manager: Arc<Mutex<crate::hotkey::HotkeyManager>>,
    pub input_paster: Arc<Mutex<crate::input::TextPaster>>,
    pub storage: Arc<Mutex<crate::storage::Storage>>,
    pub asr_engine: Arc<Mutex<Option<ASREngine>>>,
    pub indicator: Arc<Mutex<crate::indicator::IndicatorWindow>>,
    pub config: Arc<Mutex<crate::config::AppConfig>>,
    /// 当前是否处于自由说话模式
    pub is_freetalk: Arc<AtomicBool>,
    /// 自由说话开始时间（用于 grace period）
    pub freetalk_start: Arc<Mutex<Option<Instant>>>,
    /// 最近一次检测到静音的时间
    pub silence_since: Arc<Mutex<Option<Instant>>>,
    /// 录音开始前的目标窗口 HWND（粘贴时恢复焦点用）
    pub target_hwnd: Arc<Mutex<isize>>,
    /// SDR设备管理器
    pub sdr_manager: Arc<Mutex<crate::sdr::SdrManager>>,
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
        let storage = Arc::new(Mutex::new(crate::storage::Storage::new()?));
        let config  = Arc::new(Mutex::new(crate::config::AppConfig::load(&storage.lock())?));

        let mut hk = crate::hotkey::HotkeyManager::new();
        hk.is_active          = hk_is_active;
        hk.is_freetalk        = hk_is_freetalk;
        hk.last_stop_ms       = hk_last_stop;
        hk.is_recording_hotkey = hk_is_rec_hk;
        hk.press_time         = hk_press_time;

        let sdr_manager = crate::sdr::SdrManager::new();
        sdr_manager.apply_saved_config(&config.lock());

        let mut recorder = crate::audio::AudioRecorder::new()?;
        if let Some(ref device) = config.lock().audio_device {
            if let Err(e) = recorder.set_device(Some(device)) {
                log::warn!("初始化时切换音频设备失败: {}，使用默认设备", e);
            }
        }

        Ok(Self {
            recorder: Arc::new(Mutex::new(recorder)),
            hotkey_manager: Arc::new(Mutex::new(hk)),
            input_paster: Arc::new(Mutex::new(crate::input::TextPaster::new())),
            storage,
            asr_engine: Arc::new(Mutex::new(None)),
            indicator: Arc::new(Mutex::new(crate::indicator::IndicatorWindow::new(app_handle)?)),
            config,
            is_freetalk: Arc::new(AtomicBool::new(false)),
            freetalk_start: Arc::new(Mutex::new(None)),
            silence_since: Arc::new(Mutex::new(None)),
            target_hwnd: Arc::new(Mutex::new(0)),
            sdr_manager: Arc::new(Mutex::new(sdr_manager)),
        })
    }

    /// 清理资源，在应用退出前调用
    pub fn cleanup(&self) {
        info!("开始清理应用资源...");

        {
            let mut recorder = self.recorder.lock();
            if recorder.is_recording() {
                info!("停止音频录制");
                let _ = recorder.stop();
            }
        }

        {
            let mut engine = self.asr_engine.lock();
            if engine.is_some() {
                info!("释放 ASR 引擎");
                *engine = None;
            }
        }

        {
            let indicator = self.indicator.lock();
            indicator.hide();
        }

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
