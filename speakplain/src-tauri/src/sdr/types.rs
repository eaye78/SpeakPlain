//! SDR 公共数据类型

use serde::{Deserialize, Serialize};

/// SDR设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdrDeviceInfo {
    pub index: u32,
    pub name: String,
    pub tuner: String,
    /// 设备序列号（SN）
    pub serial: String,
    pub is_connected: bool,
}

/// SDR设备状态（含实时DSP指标）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdrStatus {
    pub connected: bool,
    pub frequency_mhz: f64,
    pub gain_db: f32,
    /// 当前信号强度 0.0~1.0（由DSP管线实时计算）
    pub signal_strength: f32,
    pub streaming: bool,
    pub output_device: String,
    pub demod_mode: DemodMode,
    pub ppm_correction: i32,
    pub vad_active: bool,
    /// CTCSS 设置
    pub ctcss_tone: f32,
    pub ctcss_threshold: f32,
    /// CTCSS 检测状态
    pub ctcss_detected: bool,
    pub ctcss_strength: f32,
    /// 调试信息
    pub debug_sample_rate: u32,
    pub debug_out_sample_rate: u32,
    pub debug_audio_queue_len: usize,
    pub debug_call_test_mode: bool,
    /// 诊断：解调后音频 RMS
    pub diag_audio_rms: f32,
    /// 诊断：IQ 幅度范围（信号强度指标，无信号小、有信号大）
    pub diag_iq_range: f32,
    /// 诊断：IQ 直流偏置 I（正常应接近0）
    pub diag_iq_dc_i: f32,
    /// 接收带宽（Hz）
    pub bandwidth: u32,
    /// 是否自动增益
    pub auto_gain: bool,
}

/// 解调模式（参考ShinySDR支持的解调器类型）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DemodMode {
    /// 窄带调频（对讲机语音，推荐）
    #[default]
    Nbfm,
    /// 宽带调频（FM广播）
    Wbfm,
    /// 调幅
    Am,
    /// 上边带（USB）
    Usb,
    /// 下边带（LSB）
    Lsb,
}

/// CTCSS 亚音频频率（Hz）
/// 常用频率列表：67.0, 71.9, 74.4, 77.0, 79.7, 82.5, 85.4, 88.5, 91.5, 94.8, 97.4, 100.0, 103.5, 107.2, 110.9, 114.8, 118.8, 123.0, 127.3, 131.8, 136.5, 141.3, 146.2, 151.4, 156.7, 162.2, 167.9, 173.8, 179.9, 186.2, 192.8, 203.5, 210.7, 218.1, 225.7, 233.6, 241.8, 250.3
#[allow(dead_code)]
pub const CTCSS_TONES: &[f32] = &[
    67.0, 71.9, 74.4, 77.0, 79.7, 82.5, 85.4, 88.5, 91.5, 94.8,
    97.4, 100.0, 103.5, 107.2, 110.9, 114.8, 118.8, 123.0, 127.3, 131.8,
    136.5, 141.3, 146.2, 151.4, 156.7, 162.2, 167.9, 173.8, 179.9, 186.2,
    192.8, 203.5, 210.7, 218.1, 225.7, 233.6, 241.8, 250.3,
];

/// SDR配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdrConfig {
    pub enabled: bool,
    pub device_index: Option<u32>,
    pub frequency_mhz: f64,
    pub gain_db: f32,
    pub auto_gain: bool,
    pub output_device: String,
    pub input_source: InputSource,
    pub demod_mode: DemodMode,
    /// 频率校正（PPM，硬件晶振误差补偿）
    pub ppm_correction: i32,
    /// VAD静音门控阈值（0.0~1.0，低于此值视为静音）
    pub vad_threshold: f32,
    /// 采样率（Hz，默认2.4MHz）
    pub sample_rate: u32,
    /// CTCSS 亚音频频率（Hz），0 表示不使用
    pub ctcss_tone: f32,
    /// CTCSS 检测门限（0.0~1.0）
    pub ctcss_threshold: f32,
    /// 接收带宽（Hz，默认150000匹配SDR++）
    pub bandwidth: u32,
}

impl Default for SdrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_index: None,
            frequency_mhz: 438.625,  // 常用对讲机频率
            gain_db: 6.0,  // 默认增益6dB（Fitipower FC0013推荐值）
            auto_gain: true,  // 默认启用自动增益，避免手动设置不当导致饱和
            output_device: String::new(),
            input_source: InputSource::Microphone,
            demod_mode: DemodMode::Wbfm,  // 对齐 SDR++ 截图配置（WFM + 150kHz）
            ppm_correction: 0,
            vad_threshold: 0.20,  // 高于典型噪底(0.13~0.15)，避免无信号时误触发VAD
            sample_rate: 2_400_000,
            ctcss_tone: 0.0,  // 默认禁用CTCSS，用户按需开启
            ctcss_threshold: 0.05,
            bandwidth: 150_000,  // 对齐 SDR++ 截图配置（150kHz WFM）
        }
    }
}

/// 输入源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Microphone,
    Sdr,
}

/// 测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub signal_strength: f32,
    pub sample_rate: u32,
    pub demod_mode: DemodMode,
}

// ──────────────────────────────────────────────────────────────────────────────
// cpal::Stream 包装（绕过 Send+Sync 限制）
// ──────────────────────────────────────────────────────────────────────────────
#[allow(dead_code)]
pub struct StreamHandle(pub cpal::Stream);
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}
