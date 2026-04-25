//! SDR设备管理器

pub(crate) mod stream;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sdr::types::{SdrConfig, SdrDeviceInfo, SdrStatus, DemodMode, InputSource, TestResult, StreamHandle};
use crate::sdr::hw::{RtlSdrDeviceHandle, find_nearest_gain, list_devices_hw, connect_hw, DeviceCommand};
use crate::sdr::ctcss::CtcssDetector;

/// SDR设备管理器
pub struct SdrManager {
    pub(crate) config: Arc<Mutex<SdrConfig>>,
    /// RTL-SDR 设备句柄（连接后持有，断开后为 None）
    pub(crate) device: Arc<Mutex<Option<RtlSdrDeviceHandle>>>,
    /// 运行时命令发送端（用于读取线程实时改参）
    pub(crate) cmd_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<DeviceCommand>>>>,
    pub(crate) streaming: Arc<AtomicBool>,
    pub(crate) audio_stream: Arc<Mutex<Option<StreamHandle>>>,
    /// 实时信号强度（原子存储，供状态查询）
    pub(crate) signal_strength_raw: Arc<AtomicU32>,
    /// VAD状态（是否检测到语音）
    pub(crate) vad_active: Arc<AtomicBool>,
    /// SDR采集的音频缓冲（ASR消费）
    pub audio_buffer: Arc<Mutex<Vec<f32>>>,
    /// 语音段结束回调：VAD从有→无时触发，参数为当前积累的音频
    pub on_speech_end: Arc<Mutex<Option<Box<dyn Fn(Vec<f32>) + Send + 'static>>>>,
    /// 信号强度实时回调：每批IQ处理后调用，参数为 0.0~1.0 信号强度
    pub on_signal: Arc<Mutex<Option<Box<dyn Fn(f32) + Send + 'static>>>>,
    /// VAD 状态变化回调：参数为 (has_voice: bool)
    pub on_vad_change: Arc<Mutex<Option<Box<dyn Fn(bool) + Send + 'static>>>>,
    /// 通话测试模式：只播放音频，不触发 ASR
    pub call_test_mode: Arc<AtomicBool>,
    /// 调试：当前音频队列长度
    pub(crate) audio_queue_len: Arc<AtomicU32>,
    /// 调试：音频输出采样率
    pub(crate) out_sample_rate: Arc<AtomicU32>,
    /// 诊断：解调后音频 RMS（实时）
    pub diag_audio_rms: Arc<AtomicU32>,
    /// 诊断：IQ 幅度范围（实时）
    pub diag_iq_range: Arc<AtomicU32>,
    /// 诊断：IQ DC 偏置 I（实时）
    pub diag_iq_dc_i: Arc<AtomicU32>,
    /// CTCSS 检测器（可选）
    pub ctcss_detector: Arc<Mutex<Option<CtcssDetector>>>,
    /// CTCSS 检测状态（实时）
    pub ctcss_detected: Arc<AtomicBool>,
    /// CTCSS 强度（实时）
    pub ctcss_strength: Arc<AtomicU32>,
}

impl SdrManager {
    /// 创建新的SDR管理器
    pub fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(SdrConfig::default())),
            device: Arc::new(Mutex::new(None)),
            cmd_tx: Arc::new(Mutex::new(None)),
            streaming: Arc::new(AtomicBool::new(false)),
            audio_stream: Arc::new(Mutex::new(None)),
            signal_strength_raw: Arc::new(AtomicU32::new(0)),
            vad_active: Arc::new(AtomicBool::new(false)),
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            on_speech_end: Arc::new(Mutex::new(None)),
            on_signal: Arc::new(Mutex::new(None)),
            on_vad_change: Arc::new(Mutex::new(None)),
            call_test_mode: Arc::new(AtomicBool::new(false)),
            audio_queue_len: Arc::new(AtomicU32::new(0)),
            out_sample_rate: Arc::new(AtomicU32::new(0)),
            diag_audio_rms: Arc::new(AtomicU32::new(0)),
            diag_iq_range: Arc::new(AtomicU32::new(0)),
            diag_iq_dc_i: Arc::new(AtomicU32::new(0)),
            ctcss_detector: Arc::new(Mutex::new(None)),
            ctcss_detected: Arc::new(AtomicBool::new(false)),
            ctcss_strength: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn is_device_connected(&self) -> bool {
        self.device.lock().is_some()
    }

    /// 获取当前信号强度（0.0~1.0）
    /// 使用与实时回调相同的转换公式：(rms * 6.0).powf(0.65).min(1.0)
    pub fn get_signal_strength(&self) -> f32 {
        let rms = f32::from_bits(self.signal_strength_raw.load(Ordering::Relaxed));
        // 与实时回调相同的转换公式
        (rms * 6.0).powf(0.65).min(1.0)
    }

    /// 获取SDR设备列表（标记已连接设备）
    pub fn list_devices(&self) -> Result<Vec<SdrDeviceInfo>> {
        let mut devices = list_devices_hw()?;
        let connected_index = self.config.lock().device_index;
        for dev in &mut devices {
            dev.is_connected = Some(dev.index) == connected_index && self.is_device_connected();
        }
        Ok(devices)
    }

    /// 连接SDR设备（直接调用 librtlsdr 库）
    pub fn connect(&self, device_index: u32) -> Result<()> {
        // 先确保之前的资源已清理
        self.stop_stream().ok();
        *self.device.lock() = None;
        *self.cmd_tx.lock() = None;

        let cfg = self.config.lock().clone();
        log::info!("[SDR连接] 使用配置: 频率={}MHz, 增益={}dB, 自动增益={}, PPM={}, CTCSS={}Hz",
            cfg.frequency_mhz, cfg.gain_db, cfg.auto_gain, cfg.ppm_correction, cfg.ctcss_tone);
        let handle = connect_hw(device_index, &cfg)?;
        *self.device.lock() = Some(handle);
        self.config.lock().device_index = Some(device_index);
        log::info!("SDR设备连接成功，设备索引: {}", device_index);
        Ok(())
    }

    /// 断开SDR设备
    pub fn disconnect(&self) -> Result<()> {
        self.stop_stream()?;
        *self.device.lock() = None;
        *self.cmd_tx.lock() = None;
        self.config.lock().device_index = None;
        self.signal_strength_raw.store(0, Ordering::Relaxed);
        self.vad_active.store(false, Ordering::Relaxed);
        log::info!("SDR设备已断开");
        Ok(())
    }

    /// 设置接收频率（支持22~1100MHz）
    /// 使用 librtlsdr 实时改频，无需断开重连
    pub fn set_frequency(&self, freq_mhz: f64) -> Result<()> {
        if freq_mhz < 22.0 || freq_mhz > 1100.0 {
            anyhow::bail!("频率必须在22MHz~1100MHz范围内");
        }
        self.config.lock().frequency_mhz = freq_mhz;
        if let Some(ref tx) = *self.cmd_tx.lock() {
            let freq_hz = (freq_mhz * 1e6) as u32;
            let _ = tx.send(DeviceCommand::SetFrequency(freq_hz));
            log::info!("频率实时设置为 {} MHz", freq_mhz);
        } else {
            log::info!("频率设置为 {} MHz (下次启动流时生效)", freq_mhz);
        }
        Ok(())
    }

    /// 设置增益（-1~50 dB）
    /// 使用 librtlsdr 实时改增益，无需断开重连
    pub fn set_gain(&self, gain_db: f32) -> Result<()> {
        if gain_db < -1.0 || gain_db > 50.0 {
            anyhow::bail!("增益必须在-1.0~50.0 dB范围内");
        }
        let mut cfg = self.config.lock();
        cfg.gain_db = gain_db;
        cfg.auto_gain = false;
        drop(cfg);
        if let Some(ref tx) = *self.cmd_tx.lock() {
            if let Some(ref handle) = *self.device.lock() {
                let nearest = find_nearest_gain(gain_db, &handle.gain_list);
                let _ = tx.send(DeviceCommand::SetGain(nearest));
                log::info!("增益实时设置为 {} dB (硬件值 {} tenths-dB)", gain_db, nearest);
            }
        } else {
            log::info!("增益设置为 {} dB (下次启动流时生效)", gain_db);
        }
        Ok(())
    }

    /// 设置自动增益控制（AGC）
    /// 使用 librtlsdr 实时改AGC，无需断开重连
    pub fn set_auto_gain(&self, enabled: bool) -> Result<()> {
        self.config.lock().auto_gain = enabled;
        if let Some(ref tx) = *self.cmd_tx.lock() {
            let _ = tx.send(DeviceCommand::SetAutoGain(enabled));
            log::info!("自动增益(AGC) {}", if enabled { "开启" } else { "关闭" });
        } else {
            log::info!("自动增益(AGC) {} (下次启动流时生效)", if enabled { "开启" } else { "关闭" });
        }
        Ok(())
    }

    /// 设置PPM频率校正（补偿晶振误差，典型值-50~+50 ppm，范围限制±1000）
    /// 使用 librtlsdr 实时改PPM，无需断开重连
    /// 注意：某些 tuner（如 FC0013）可能不支持 PPM 校正，失败时只记录警告
    pub fn set_ppm_correction(&self, ppm: i32) -> Result<()> {
        if ppm < -1000 || ppm > 1000 {
            anyhow::bail!("PPM校正值必须在 -1000 ~ +1000 范围内");
        }
        self.config.lock().ppm_correction = ppm;
        if let Some(ref tx) = *self.cmd_tx.lock() {
            let _ = tx.send(DeviceCommand::SetPpm(ppm));
            log::info!("PPM校正实时设置为 {} ppm", ppm);
        } else {
            log::info!("PPM校正设置为 {} ppm (下次启动流时生效)", ppm);
        }
        Ok(())
    }

    /// 设置解调模式
    pub fn set_demod_mode(&self, mode: DemodMode) {
        log::info!("解调模式切换为: {:?}", mode);
        self.config.lock().demod_mode = mode;
    }

    /// 设置VAD阈值
    pub fn set_vad_threshold(&self, threshold: f32) {
        self.config.lock().vad_threshold = threshold.clamp(0.0, 1.0);
    }

    /// 设置CTCSS亚音频频率（0表示禁用）
    /// 注意：CTCSS 检测器在流启动时自动初始化，此处仅更新配置
    pub fn set_ctcss_tone(&self, tone_hz: f32) {
        self.config.lock().ctcss_tone = tone_hz;
        if tone_hz > 0.0 {
            log::info!("CTCSS 设置为 {} Hz (下次启动流时生效)", tone_hz);
        } else {
            log::info!("CTCSS 已禁用");
        }
    }

    /// 设置CTCSS检测门限
    pub fn set_ctcss_threshold(&self, threshold: f32) {
        self.config.lock().ctcss_threshold = threshold.clamp(0.0, 1.0);
        // 如果检测器存在，更新门限
        if let Some(ref mut detector) = *self.ctcss_detector.lock() {
            detector.threshold = threshold;
        }
    }

    /// 设置接收带宽（Hz）
    /// 注意：rtl_sdr 不支持运行时改带宽，需要断开重连
    pub fn set_bandwidth(&self, bandwidth: u32) -> Result<()> {
        if bandwidth < 10000 || bandwidth > 300000 {
            anyhow::bail!("带宽必须在10kHz~300kHz范围内");
        }
        self.config.lock().bandwidth = bandwidth;
        log::info!("带宽设置为 {} Hz (下次连接时生效)", bandwidth);
        Ok(())
    }

    /// 获取虚拟音频设备列表（含所有输出设备，标记VB-Cable等虚拟设备）
    pub fn list_virtual_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices = host.output_devices().context("无法枚举音频设备")?;
        let mut all_devices = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                all_devices.push(name);
            }
        }
        Ok(all_devices)
    }

    /// 设置输出设备
    pub fn set_output_device(&self, device_name: String) -> Result<()> {
        self.config.lock().output_device = device_name.clone();
        log::info!("SDR输出设备: {}", device_name);
        Ok(())
    }

    /// 获取设备状态
    pub fn get_status(&self) -> SdrStatus {
        let cfg = self.config.lock();
        SdrStatus {
            connected: self.is_device_connected(),
            frequency_mhz: cfg.frequency_mhz,
            gain_db: cfg.gain_db,
            signal_strength: self.get_signal_strength(),
            streaming: self.streaming.load(Ordering::Relaxed),
            output_device: cfg.output_device.clone(),
            demod_mode: cfg.demod_mode.clone(),
            ppm_correction: cfg.ppm_correction,
            vad_active: self.vad_active.load(Ordering::Relaxed),
            ctcss_tone: cfg.ctcss_tone,
            ctcss_threshold: cfg.ctcss_threshold,
            ctcss_detected: self.ctcss_detected.load(Ordering::Relaxed),
            ctcss_strength: f32::from_bits(self.ctcss_strength.load(Ordering::Relaxed)),
            debug_sample_rate: cfg.sample_rate,
            debug_out_sample_rate: self.out_sample_rate.load(Ordering::Relaxed),
            debug_audio_queue_len: self.audio_queue_len.load(Ordering::Relaxed) as usize,
            debug_call_test_mode: self.call_test_mode.load(Ordering::Relaxed),
            diag_audio_rms: f32::from_bits(self.diag_audio_rms.load(Ordering::Relaxed)),
            diag_iq_range: f32::from_bits(self.diag_iq_range.load(Ordering::Relaxed)),
            diag_iq_dc_i: f32::from_bits(self.diag_iq_dc_i.load(Ordering::Relaxed)),
            bandwidth: cfg.bandwidth,
            auto_gain: cfg.auto_gain,
        }
    }

    /// 获取配置
    #[allow(dead_code)]
    pub fn get_config(&self) -> SdrConfig {
        self.config.lock().clone()
    }

    /// 批量更新配置（频率/增益变化时同步到硬件）
    #[allow(dead_code)]
    pub fn set_config(&self, config: SdrConfig) -> Result<()> {
        let old_cfg = self.config.lock().clone();
        if (old_cfg.frequency_mhz - config.frequency_mhz).abs() > 0.001 {
            self.set_frequency(config.frequency_mhz)?;
        }
        if old_cfg.gain_db != config.gain_db && !config.auto_gain {
            self.set_gain(config.gain_db)?;
        }
        if old_cfg.ppm_correction != config.ppm_correction {
            self.set_ppm_correction(config.ppm_correction)?;
        }
        // CTCSS 设置可以实时更新
        if (old_cfg.ctcss_tone - config.ctcss_tone).abs() > 0.1 {
            self.set_ctcss_tone(config.ctcss_tone);
        }
        if (old_cfg.ctcss_threshold - config.ctcss_threshold).abs() > 0.01 {
            self.set_ctcss_threshold(config.ctcss_threshold);
        }
        *self.config.lock() = config;
        Ok(())
    }

    /// 测试设备连接（返回当前信号强度和DSP配置）
    pub fn test_connection(&self) -> Result<TestResult> {
        if !self.is_device_connected() {
            return Ok(TestResult {
                success: false,
                message: "SDR设备未连接".to_string(),
                signal_strength: 0.0,
                sample_rate: 0,
                demod_mode: DemodMode::Nbfm,
            });
        }
        let cfg = self.config.lock();
        Ok(TestResult {
            success: true,
            message: format!("设备连接正常 | {}MHz | {}dB | {:?}",
                cfg.frequency_mhz, cfg.gain_db, cfg.demod_mode),
            signal_strength: self.get_signal_strength(),
            sample_rate: cfg.sample_rate,
            demod_mode: cfg.demod_mode.clone(),
        })
    }

    /// 设置输入源
    pub fn set_input_source(&self, source: InputSource) {
        log::info!("输入源切换为: {:?}", source);
        self.config.lock().input_source = source;
    }

    /// 获取输入源
    pub fn get_input_source(&self) -> InputSource {
        self.config.lock().input_source.clone()
    }

    /// 是否使用SDR输入
    pub fn is_sdr_input(&self) -> bool {
        self.config.lock().input_source == InputSource::Sdr
    }

    /// 从 AppConfig 批量应用配置（启动时初始化用，不需要设备已连接）
    pub fn apply_saved_config(&self, app_cfg: &crate::config::AppConfig) {
        let mut cfg = self.config.lock();
        cfg.frequency_mhz  = app_cfg.sdr_frequency_mhz;
        cfg.gain_db        = app_cfg.sdr_gain_db;
        cfg.auto_gain      = app_cfg.sdr_auto_gain;
        cfg.output_device  = app_cfg.sdr_output_device.clone();
        cfg.input_source   = app_cfg.sdr_input_source.clone();
        cfg.demod_mode     = app_cfg.sdr_demod_mode.clone();
        cfg.ppm_correction = app_cfg.sdr_ppm_correction;
        cfg.vad_threshold  = app_cfg.sdr_vad_threshold;
        cfg.ctcss_tone     = app_cfg.sdr_ctcss_tone;
        cfg.ctcss_threshold = app_cfg.sdr_ctcss_threshold;
        if let Some(idx) = app_cfg.sdr_device_index {
            cfg.device_index = Some(idx);
        }
        log::info!("SDR配置已从数据库加载: 频率={} MHz, 增益={} dB, 解调={:?}, CTCSS={}Hz",
            cfg.frequency_mhz, cfg.gain_db, cfg.demod_mode, cfg.ctcss_tone);
    }
}

impl Default for SdrManager {
    fn default() -> Self {
        Self::new()
    }
}
