//! SDR设备管理模块
//! 支持RTL2832U设备，通过 rtl_sdr 进程读取IQ数据，参考ShinySDR架构实现DSP管线
//!
//! DSP管线：IQ原始数据 → NBFM解调 → FIR低通滤波 → 降采样(2.4MHz→16kHz) → VAD检测 → 输出
//!
//! 使用 rtl_sdr.exe（项目sdr/目录内置）直接读取USB设备数据，不使用网络TCP模式。

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ──────────────────────────────────────────────────────────────────────────────
// 公共数据结构
// ──────────────────────────────────────────────────────────────────────────────

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
    pub gain_db: i32,
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
    pub gain_db: i32,
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
            frequency_mhz: 461.025,
            gain_db: 6,  // 默认增益6dB（Fitipower FC0013推荐值）
            auto_gain: true,  // 默认启用自动增益，避免手动设置不当导致饱和
            output_device: String::new(),
            input_source: InputSource::Microphone,
            demod_mode: DemodMode::Nbfm,
            ppm_correction: 0,
            vad_threshold: 0.01,
            sample_rate: 2_400_000,
            ctcss_tone: 0.0,
            ctcss_threshold: 0.05,  // 降低阈值，更容易检测到亚音
            bandwidth: 150_000,
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
struct StreamHandle(cpal::Stream);
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}

// ──────────────────────────────────────────────────────────────────────────────
// RTL-SDR 硬件连接封装
// 通过启动 rtl_sdr.exe 子进程，从 stdout 读取 IQ 数据流
// 完全绕过 rtlsdr crate 的 DLL 版本兼容问题
// ──────────────────────────────────────────────────────────────────────────────
mod hw {
    use super::*;
    use std::path::PathBuf;
    use std::process::{Child, ChildStdout, Stdio};

    /// RTL-SDR 进程句柄（Drop 时自动 kill）
    pub struct RtlSdrProcess {
        pub child: Child,
        /// stdout 用 Option 包装，方便用 take() 取出给读取线程
        pub stdout: Option<ChildStdout>,
    }

    impl Drop for RtlSdrProcess {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            log::info!("rtl_sdr 进程已终止");
        }
    }

    /// 查找 rtl_sdr.exe 路径（优先 exe 同目录，其次 sdr/x64/）
    pub fn find_rtl_sdr() -> Option<PathBuf> {
        // exe 旁边
        if let Ok(exe) = std::env::current_exe() {
            let p = exe.parent().unwrap().join("rtl_sdr.exe");
            if p.exists() { return Some(p); }
        }
        // sdr/x64/ 目录（开发环境）
        let manifest = std::env::var("CARGO_MANIFEST_DIR").ok()?;
        let p = PathBuf::from(manifest).parent()?.join("sdr").join("x64").join("rtl_sdr.exe");
        if p.exists() { return Some(p); }
        None
    }

    /// 枚举 RTL-SDR 设备：运行 rtl_test.exe 解析其输出获取设备列表
    pub fn list_devices_hw() -> Result<Vec<SdrDeviceInfo>> {
        let rtl_sdr = find_rtl_sdr()
            .ok_or_else(|| anyhow::anyhow!("未找到 rtl_sdr.exe，请确认 sdr/ 目录完整"))?;

        // 用 rtl_test.exe 枚举（同目录下）
        let rtl_test = rtl_sdr.parent().unwrap().join("rtl_test.exe");
        if rtl_test.exists() {
            let out = std::process::Command::new(&rtl_test)
                .arg("-t")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .ok();

            if let Some(output) = out {
                let text = String::from_utf8_lossy(&output.stderr).to_string()
                    + &String::from_utf8_lossy(&output.stdout);
                log::info!("rtl_test 输出: {}", text.trim());
                return parse_rtl_test_output(&text);
            }
        }

        // 无法枚举设备，返回空列表
        log::warn!("无法枚举设备，请确认：1.已插USB  2.已用Zadig装WinUSB驱动  3.无其他SDR软件占用");
        Ok(vec![])
    }

    /// 解析 rtl_test 输出，提取设备列表
    fn parse_rtl_test_output(text: &str) -> Result<Vec<SdrDeviceInfo>> {
        let mut devices = Vec::new();
        // 查找 "Found N device(s):"
        let count = text.lines()
            .find_map(|l| {
                if l.contains("Found") && l.contains("device") {
                    l.split_whitespace().nth(1).and_then(|n| n.parse::<u32>().ok())
                } else { None }
            })
            .unwrap_or(0);

        log::info!("RTL-SDR: 检测到 {} 个设备", count);

        for i in 0..count {
            // 查找 "  N:  制造商, 产品, SN:"
            let mut name = format!("Generic RTL2832U #{}", i);
            let mut tuner = "RTL2832U".to_string();
            let mut serial = String::new();
            for line in text.lines() {
                let prefix = format!("  {}:", i);
                if line.starts_with(&prefix) {
                    // "  0:  Generic, RTL2832U, SN: 77771111153705700"
                    let parts: Vec<&str> = line[prefix.len()..].split(',').collect();
                    if parts.len() >= 2 {
                        let mfr = parts[0].trim();
                        let prod = parts[1].trim();
                        if !mfr.is_empty() { name = format!("{} {}", mfr, prod).trim().to_string(); }
                        tuner = prod.to_string();
                    }
                    // 提取 SN
                    if let Some(sn_pos) = line.find("SN:") {
                        serial = line[sn_pos + 3..].trim().to_string();
                    }
                    break;
                }
                // 也匹配 "Using device N: xxx" 中的调谐器名
                let using = format!("Using device {}:", i);
                if line.contains(&using) {
                    if let Some(t) = line.split(&using).nth(1) {
                        name = t.trim().to_string();
                    }
                }
                if line.starts_with("Found ") && line.contains("tuner") {
                    tuner = line.replace("Found ", "").replace(" tuner", "").trim().to_string();
                }
            }
            devices.push(SdrDeviceInfo { index: i, name, tuner, serial, is_connected: false });
        }

        if devices.is_empty() {
            log::warn!("RTL-SDR: 未发现设备。确认：1.已插USB  2.已用Zadig装WinUSB驱动  3.无其他SDR软件占用");
        }
        Ok(devices)
    }

    /// 启动 rtl_sdr 子进程并返回 stdout 句柄
    pub fn connect_hw(device_index: u32, cfg: &SdrConfig) -> Result<RtlSdrProcess> {
        let rtl_sdr = find_rtl_sdr()
            .ok_or_else(|| anyhow::anyhow!("未找到 rtl_sdr.exe"))?;

        let freq_hz = (cfg.frequency_mhz * 1e6) as u32;
        let sample_rate = cfg.sample_rate;

        // 将 rtl_sdr stderr 重定向到日志文件，方便排查硬件问题
        let log_path = std::env::temp_dir().join("speakplain_rtlsdr.log");
        let log_file = std::fs::File::create(&log_path).ok();
        let stderr_handle = if let Some(f) = log_file {
            log::info!("rtl_sdr 日志路径: {}", log_path.display());
            Stdio::from(f)
        } else {
            Stdio::null()
        };

        // 构建命令行参数
        let mut args = vec![
            "-d".to_string(), device_index.to_string(),
            "-f".to_string(), freq_hz.to_string(),
            "-s".to_string(), sample_rate.to_string(),
        ];

        // 增益设置（参考 SDR++：默认使用自动增益或较低增益防止饱和）
        // RTL-SDR 8-bit ADC 动态范围有限，高增益易导致饱和
        // Fitipower FC0013 调谐器在强信号环境下容易饱和，建议使用自动增益
        const MAX_GAIN_DB: i32 = 0;  // 最大手动增益限制（0 表示最小增益）
        if cfg.auto_gain {
            args.push("-g".to_string());
            args.push("0".to_string()); // 0 表示自动增益（强烈推荐）
            log::info!("使用自动增益模式（推荐）");
        } else {
            let limited_gain = cfg.gain_db.min(MAX_GAIN_DB);
            if limited_gain != cfg.gain_db {
                log::warn!("增益 {}dB 超过最大限制 {}dB，已自动限制为 {}dB。建议启用自动增益以避免饱和！", 
                    cfg.gain_db, MAX_GAIN_DB, limited_gain);
            }
            args.push("-g".to_string());
            args.push(limited_gain.to_string());
        }

        // PPM 校正（注意：rtl_sdr 使用小写 -p）
        if cfg.ppm_correction != 0 {
            args.push("-p".to_string());
            args.push(cfg.ppm_correction.to_string());
        }

        // 输出到 stdout（必须作为最后一个参数）
        args.push("-".to_string());

        log::info!("启动 rtl_sdr: {} {:?}", rtl_sdr.display(), args);

        let mut child = std::process::Command::new(&rtl_sdr)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(stderr_handle)
            .spawn()
            .map_err(|e| anyhow::anyhow!("启动 rtl_sdr 失败: {}", e))?;

        // 短暂等待设备就绪
        std::thread::sleep(std::time::Duration::from_millis(500));

        // 先取出 stdout，再构建 RtlSdrProcess
        let stdout = child.stdout.take();

        log::info!("rtl_sdr 进程已启动，频率={}MHz ({}Hz) 采样率={}Hz 增益={}dB PPM={}",
            cfg.frequency_mhz, freq_hz, sample_rate, cfg.gain_db, cfg.ppm_correction);

        Ok(RtlSdrProcess {
            child,
            stdout,
        })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// IQ Frontend 预处理（参考 SDRPlusPlus 架构）
// ──────────────────────────────────────────────────────────────────────────────

/// IQ Frontend：原始 IQ 数据预处理
/// 参考 SDR++ 的 iq_frontend.h 实现
/// - DC 去除（高通滤波器）
/// - IQ 平衡校正
/// - 自动增益控制（AGC）防止饱和
pub struct IqFrontend {
    /// DC 去除滤波器状态（I 通道）
    dc_i: f32,
    /// DC 去除滤波器状态（Q 通道）
    dc_q: f32,
    /// DC 去除系数（alpha = 1 - exp(-2π * fc / fs)）
    dc_alpha: f32,
    /// IQ 不平衡校正系数
    iq_balance: f32,
    /// 自动增益控制增益
    agc_gain: f32,
    /// AGC 目标电平（RMS）
    agc_target: f32,
    /// AGC 攻击系数（快速降低增益）
    agc_attack: f32,
    /// AGC 释放系数（慢速增加增益）
    agc_release: f32,
}

impl IqFrontend {
    /// 创建新的 IQ Frontend
    /// - sample_rate: 采样率（Hz）
    pub fn new(sample_rate: u32) -> Self {
        // DC 去除：截止频率约 10Hz（去除非常慢的漂移）
        let dc_cutoff = 10.0f32;
        let dc_alpha = 1.0 - (-2.0 * std::f32::consts::PI * dc_cutoff / sample_rate as f32).exp();
        
        // AGC 系数：攻击快（防止饱和），释放慢（保持稳定）
        let agc_attack = 0.9;   // 快速降低增益
        let agc_release = 0.995; // 慢速增加增益
        
        Self {
            dc_i: 0.0,
            dc_q: 0.0,
            dc_alpha,
            iq_balance: 1.0,
            agc_gain: 1.0,
            agc_target: 0.3, // 目标 RMS 电平（防止饱和）
            agc_attack,
            agc_release,
        }
    }

    /// 处理 IQ 样本对
    /// 返回 (i_corrected, q_corrected)
    pub fn process(&mut self, i_raw: f32, q_raw: f32) -> (f32, f32) {
        // 1. DC 去除（一阶高通滤波器）
        self.dc_i += self.dc_alpha * (i_raw - self.dc_i);
        self.dc_q += self.dc_alpha * (q_raw - self.dc_q);
        let i_dc_removed = i_raw - self.dc_i;
        let q_dc_removed = q_raw - self.dc_q;

        // 2. IQ 平衡校正（简单幅度平衡）
        let i_balanced = i_dc_removed;
        let q_balanced = q_dc_removed * self.iq_balance;

        // 3. IQ 限幅（防止饱和）
        // 将 IQ 限制在 [-1.0, 1.0] 范围内，防止后续处理溢出
        const IQ_LIMIT: f32 = 0.95;
        let i_limited = i_balanced.clamp(-IQ_LIMIT, IQ_LIMIT);
        let q_limited = q_balanced.clamp(-IQ_LIMIT, IQ_LIMIT);

        // 4. 自动增益控制（AGC）
        // 计算当前幅度
        let amp = (i_limited * i_limited + q_limited * q_limited).sqrt();
        
        // 更新 AGC 增益
        if amp > self.agc_target {
            // 信号太强，快速降低增益（攻击）
            self.agc_gain = self.agc_gain * self.agc_attack + (self.agc_target / amp) * (1.0 - self.agc_attack);
        } else {
            // 信号较弱，慢速增加增益（释放）
            self.agc_gain = self.agc_gain * self.agc_release + 1.0 * (1.0 - self.agc_release);
        }
        
        // 限制增益范围（防止过大或过小）
        self.agc_gain = self.agc_gain.clamp(0.01, 5.0);
        
        // 应用增益
        let i_agc = i_limited * self.agc_gain;
        let q_agc = q_limited * self.agc_gain;

        (i_agc, q_agc)
    }

    /// 重置状态
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.dc_i = 0.0;
        self.dc_q = 0.0;
        self.agc_gain = 1.0;
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// DSP管线（参考 SDRPlusPlus 架构）
// ──────────────────────────────────────────────────────────────────────────────

/// DSP管线：将RTL-SDR原始IQ字节流处理为音频
/// 
/// 架构参考 SDRPlusPlus：
/// - IQ Frontend：DC 去除、IQ 平衡
/// - 正交解调（Quadrature Demodulation）使用 atan2 计算相位差
/// - 两级抽取策略：
///   级1：粗抽取（步长 = input_rate / 240000，到约240kHz）
///   级2：精抽取（到 output_rate，通常48kHz）
/// - 多相 FIR 滤波器实现高效抽取
///
/// 与 SDR++ 的差异：
/// - SDR++ 使用 librtlsdr 异步回调，我们使用 rtl_sdr.exe 同步流
/// - SDR++ 使用多相 FIR，我们使用标准 FIR（简化实现）
/// DSP处理结果（音频 + CTCSS频率样本）
#[derive(Clone, Debug)]
pub struct DspOutput {
    /// 音频样本（单声道）
    pub audio: Vec<f32>,
    /// FM解调后的频率偏移样本（用于CTCSS检测）
    /// 这些样本在 stage1_rate 采样率下，包含频偏信息
    pub freq_samples: Vec<f32>,
}

pub struct DspPipeline {
    // 输入采样率（Hz，RTL-SDR原始IQ率）
    // pub input_rate: u32,
    // 输出音频采样率（Hz，匹配音频设备）
    // pub output_rate: u32,
    /// 解调模式
    pub mode: DemodMode,
    /// IQ Frontend 预处理模块
    iq_frontend: IqFrontend,
    /// NBFM/WBFM 前一个相位值（用于正交解调，参考 SDR++）
    prev_phase: f32,
    /// 级1抽取因子（大整数降采样，降到约240kHz）
    stage1_decim: usize,
    /// 级1抽取后采样率
    stage1_rate: u32,
    /// 级2抽取因子（降到output_rate）
    stage2_decim: usize,
    /// 级1 FIR低通（截止频率 = stage1_rate/2，消除级1混叠）
    fir1_state: Vec<f32>,
    fir1_coeffs: Vec<f32>,
    /// 级2 FIR低通（截止频率 = output_rate*0.4，消除级2混叠+音频带外噪声）
    fir2_state: Vec<f32>,
    fir2_coeffs: Vec<f32>,
    /// 级1计数器
    decim1_counter: usize,
    /// 级2计数器
    decim2_counter: usize,
    /// 信号强度（RMS，实时更新）
    pub signal_rms: f32,
    /// NBFM去加重滤波器状态（模拟收音机75μs去加重）
    deemph_state: f32,
    /// 诊断：IQ直流偏置 I 分量（正常应接近0）
    pub diag_iq_dc_i: f32,
    /// 诊断：IQ直流偏置 Q 分量（正常应接近0）
    pub diag_iq_dc_q: f32,
    /// 诊断：I 分量幅度范围（正常无信号约0.01，有强信号可达0.5+）
    pub diag_iq_range: f32,
    /// 诊断：解调后音频 RMS（应随语音变化）
    pub diag_audio_rms: f32,
    /// FM解调后的频率样本缓冲区（用于CTCSS检测）
    freq_samples: Vec<f32>,
    /// 音频输出缓冲区
    audio_out: Vec<f32>,
    /// CTCSS低通滤波器状态（截止频率300Hz，用于提取亚音）
    ctcss_fir_state: Vec<f32>,
    ctcss_fir_coeffs: Vec<f32>,
}

impl DspPipeline {
    /// 创建新的DSP管线
    /// bandwidth: 接收带宽（Hz），用于设置FIR滤波器截止频率
    pub fn new(input_rate: u32, output_rate: u32, mode: DemodMode, bandwidth: u32) -> Self {
        // 初始化 IQ Frontend
        let iq_frontend = IqFrontend::new(input_rate);

        // 级1：抽取到约240kHz（方便后续处理），如 2400000/10=240000
        let stage1_decim = ((input_rate / 240_000) as usize).max(1);
        let stage1_rate = input_rate / stage1_decim as u32;

        // 级2：从stage1_rate抽取到output_rate
        let stage2_decim = ((stage1_rate / output_rate) as usize).max(1);

        // 级1 FIR：根据带宽设置截止频率
        // 截止频率 = min(带宽/2, stage1_rate/2)，避免混叠
        let half_bandwidth = (bandwidth as f32 / 2.0).min(stage1_rate as f32 / 2.0);
        let cutoff1_norm = half_bandwidth / (input_rate as f32 / 2.0);
        let fir1_coeffs = design_fir_lowpass(63, cutoff1_norm.min(0.95));

        // 级2 FIR：截止频率 = min(带宽/2, output_rate*0.45)
        // 在stage1_rate频域归一化
        let audio_bandwidth = half_bandwidth.min(output_rate as f32 * 0.45);
        let cutoff2_norm = audio_bandwidth / (stage1_rate as f32 / 2.0);
        let fir2_coeffs = design_fir_lowpass(63, cutoff2_norm.min(0.95));

        // CTCSS 低通滤波器：截止频率 300Hz，用于从频偏中提取亚音
        // 在 stage1_rate 采样率下，归一化截止频率 = 300 / (stage1_rate/2)
        let ctcss_cutoff_norm = 300.0 / (stage1_rate as f32 / 2.0);
        let ctcss_fir_coeffs = design_fir_lowpass(63, ctcss_cutoff_norm.min(0.95));

        log::info!("DSP管线: input={}Hz bandwidth={}Hz stage1={}/{} stage2={}/{} output={}Hz CTCSS滤波器截止={}Hz",
            input_rate, bandwidth, stage1_rate, stage1_decim, output_rate, stage2_decim, output_rate, 300);

        let fir1_len = fir1_coeffs.len();
        let fir2_len = fir2_coeffs.len();
        let ctcss_fir_len = ctcss_fir_coeffs.len();
        Self {
            // input_rate,
            // output_rate,
            mode,
            iq_frontend,
            prev_phase: 0.0,
            stage1_decim,
            stage1_rate,
            stage2_decim,
            fir1_state: vec![0.0f32; fir1_len],
            fir1_coeffs,
            fir2_state: vec![0.0f32; fir2_len],
            fir2_coeffs,
            decim1_counter: 0,
            decim2_counter: 0,
            signal_rms: 0.0,
            deemph_state: 0.0,
            diag_iq_dc_i: 0.0,
            diag_iq_dc_q: 0.0,
            diag_iq_range: 0.0,
            diag_audio_rms: 0.0,
            freq_samples: Vec::new(),
            audio_out: Vec::new(),
            ctcss_fir_state: vec![0.0f32; ctcss_fir_len],
            ctcss_fir_coeffs,
        }
    }

    /// 处理一批IQ原始字节，返回解调后的音频和频率样本
    /// 音频样本用于播放，频率样本用于CTCSS检测
    pub fn process(&mut self, iq_bytes: &[u8]) -> DspOutput {
        let n_samples = iq_bytes.len() / 2;
        let expected_audio_out = n_samples / (self.stage1_decim * self.stage2_decim) + 4;
        let expected_freq_out = n_samples / self.stage1_decim + 4;
        self.audio_out.clear();
        self.audio_out.reserve(expected_audio_out);
        self.freq_samples.clear();
        self.freq_samples.reserve(expected_freq_out);

        // 1. 计算信号强度（RMS，基于IQ功率）+ 诊断统计
        let mut power_sum = 0.0f32;
        let mut i_sum = 0.0f32;
        let mut q_sum = 0.0f32;
        let mut i_min = f32::MAX;
        let mut i_max = f32::MIN;
        for chunk in iq_bytes.chunks_exact(2) {
            let i = (chunk[0] as f32 - 127.4) / 128.0;
            let q = (chunk[1] as f32 - 127.4) / 128.0;
            power_sum += i * i + q * q;
            i_sum += i;
            q_sum += q;
            if i < i_min { i_min = i; }
            if i > i_max { i_max = i; }
        }
        self.signal_rms = (power_sum / n_samples as f32).sqrt();
        // 存储诊断值供外部读取
        self.diag_iq_dc_i = i_sum / n_samples as f32;
        self.diag_iq_dc_q = q_sum / n_samples as f32;
        self.diag_iq_range = i_max - i_min;

        // 2. 解调 + 两级抽取
        // 检测 IQ 饱和：如果范围接近 2.0，说明信号过强
        let iq_saturated = self.diag_iq_range > 1.9;
        if iq_saturated && self.signal_rms > 1.0 {
            log::debug!("IQ 饱和 detected: 范围={:.3}, RMS={:.3}，跳过解调", 
                self.diag_iq_range, self.signal_rms);
        }
        
        for chunk in iq_bytes.chunks_exact(2) {
            // 原始 IQ 转换为浮点（-1.0 ~ 1.0）
            let i_raw = (chunk[0] as f32 - 127.4) / 128.0;
            let q_raw = (chunk[1] as f32 - 127.4) / 128.0;

            // IQ Frontend 预处理：DC 去除、IQ 平衡
            let (i, q) = self.iq_frontend.process(i_raw, q_raw);

            // 如果 IQ 严重饱和，跳过此样本的处理（防止解调错误）
            if iq_saturated && self.signal_rms > 1.0 {
                continue;
            }

            // 解调（参考 SDRPlusPlus 正交解调算法）
            let demod_sample = match self.mode {
                DemodMode::Nbfm | DemodMode::Wbfm => {
                    // FM 解调：使用 atan2 计算相位差（SDR++ 风格）
                    // 这种方法在低信噪比下更精确
                    
                    // IQ 限幅：防止饱和信号导致相位跳变
                    let max_amp = 0.95;
                    let amp = (i * i + q * q).sqrt();
                    let (i_clamped, q_clamped) = if amp > max_amp {
                        let scale = max_amp / amp;
                        (i * scale, q * scale)
                    } else {
                        (i, q)
                    };
                    
                    // 计算当前相位（使用限幅后的 IQ）
                    let current_phase = q_clamped.atan2(i_clamped);
                    
                    // 计算相位差
                    let mut phase_diff = current_phase - self.prev_phase;
                    self.prev_phase = current_phase;
                    
                    // 归一化相位差到 [-π, π]
                    if phase_diff > std::f32::consts::PI {
                        phase_diff -= 2.0 * std::f32::consts::PI;
                    } else if phase_diff < -std::f32::consts::PI {
                        phase_diff += 2.0 * std::f32::consts::PI;
                    }
                    
                    // 转换为频率偏移（Hz）
                    // 公式：Δf = Δφ * fs / (2π)
                    let freq_offset_hz = phase_diff * self.stage1_rate as f32 / (2.0 * std::f32::consts::PI);
                    
                    // 保存原始频率样本（用于CTCSS检测，在stage1_rate采样率下）
                    self.freq_samples.push(freq_offset_hz);
                    
                    // 根据模式归一化输出
                    // NBFM: 典型频偏 ±5kHz，输出归一化到 ±0.5
                    // WBFM: 典型频偏 ±75kHz，输出归一化到 ±0.5
                    let max_deviation = match self.mode {
                        DemodMode::Nbfm => 5000.0,
                        DemodMode::Wbfm => 75000.0,
                        _ => 5000.0,
                    };
                    
                    (freq_offset_hz / max_deviation * 0.5).clamp(-0.9, 0.9)
                }
                DemodMode::Am => {
                    (i * i + q * q).sqrt() - 0.5 // 去除直流
                }
                DemodMode::Usb => i,
                DemodMode::Lsb => q,
            };

            // 级1：FIR低通 + 抽取
            let fir1_out = Self::fir_filter_inplace(&mut self.fir1_state, &self.fir1_coeffs, demod_sample);
            self.decim1_counter += 1;
            if self.decim1_counter < self.stage1_decim {
                continue;
            }
            self.decim1_counter = 0;

            // 经过级1抽取后得到 stage1_rate Hz 的音频
            // NBFM去加重：75μs时间常数（模拟收音机去加重曲线，改善音质）
            let after_deemph = if matches!(self.mode, DemodMode::Nbfm) {
                let alpha = (-1.0 / (self.stage1_rate as f32 * 75e-6)).exp();
                self.deemph_state = alpha * self.deemph_state + (1.0 - alpha) * fir1_out;
                self.deemph_state
            } else {
                fir1_out
            };

            // 级2：FIR低通 + 抽取
            let fir2_out = Self::fir_filter_inplace(&mut self.fir2_state, &self.fir2_coeffs, after_deemph);
            self.decim2_counter += 1;
            if self.decim2_counter < self.stage2_decim {
                continue;
            }
            self.decim2_counter = 0;

            // 增益补偿（FM解调输出幅度取决于调制指数）
            // SDR++ 风格：NBFM 需要较高增益补偿
            let gain = match self.mode {
                DemodMode::Nbfm => 10.0,  // 提高增益以获得合适的音频电平
                DemodMode::Wbfm => 2.0,
                DemodMode::Am   => 3.0,
                _               => 2.0,
            };
            let audio_sample = (fir2_out * gain).clamp(-1.0, 1.0);
            
            // 输出音频样本
            self.audio_out.push(audio_sample);
        }

        // 计算解调后音频 RMS（诊断用）
        if !self.audio_out.is_empty() {
            let ar = (self.audio_out.iter().map(|&x| x * x).sum::<f32>() / self.audio_out.len() as f32).sqrt();
            self.diag_audio_rms = ar;
        }

        DspOutput {
            audio: self.audio_out.clone(),
            freq_samples: self.freq_samples.clone(),
        }
    }

    /// FIR卷积滤波（单样本处理，移位寄存器实现）
    fn fir_filter_inplace(state: &mut Vec<f32>, coeffs: &[f32], sample: f32) -> f32 {
        let len = state.len();
        for i in (1..len).rev() {
            state[i] = state[i - 1];
        }
        state[0] = sample;
        state.iter().zip(coeffs.iter()).map(|(s, c)| s * c).sum()
    }
}

/// CTCSS 检测器（严格复刻 SDR++ 实现）
/// SDR++ 在 FM 解调器之后，对频率偏移样本进行 CTCSS 检测
/// 处理流程：低通滤波 -> DDC（160.55Hz偏移）-> 抽取到 500Hz -> 运行均值/方差
pub struct CtcssDetector {
    /// 目标频率（Hz）
    target_freq: f32,
    /// 输入采样率（Hz）- FM解调后的采样率（stage1_rate，约240kHz）
    input_sample_rate: f32,
    /// CTCSS 处理采样率（Hz）- SDR++ 使用 500Hz
    ctcss_sample_rate: f32,
    /// 检测门限
    threshold: f32,
    /// DDC 相位累加器
    ddc_phase: f32,
    /// DDC 相位步进
    ddc_phase_step: f32,
    /// 抽取计数器
    decim_count: usize,
    /// 抽取比率
    decim_ratio: usize,
    /// 运行均值（频率平滑）
    mean: f32,
    /// 运行方差（频率稳定性）
    var: f32,
    /// 方差是否稳定
    var_ok: bool,
    /// 最小频率阈值（Hz）
    min_freq: f32,
    /// 最大频率阈值（Hz）
    max_freq: f32,
    /// 检测结果
    pub detected: bool,
    /// 检测到的频率强度（0.0~1.0）
    pub strength: f32,
    /// 检测到的实际频率（Hz）
    pub detected_freq: f32,
    /// 样本计数（用于决策）
    sample_count: usize,
    /// 静音状态
    mute: bool,
    /// 低通滤波器状态（用于提取亚音）
    lp_state: Vec<f32>,
    /// 低通滤波器系数
    lp_coeffs: Vec<f32>,
}

impl CtcssDetector {
    /// 创建新的 CTCSS 检测器（严格复刻 SDR++）
    /// - tone_freq: 目标亚音频频率（Hz），如 85.4
    /// - input_sample_rate: 输入采样率（Hz），FM解调后的采样率（约240kHz）
    /// - threshold: 检测门限（0.0~1.0）
    pub fn new(tone_freq: f32, input_sample_rate: f32, threshold: f32) -> Self {
        // SDR++ 使用 160.55Hz 作为 DDC 偏移
        // DDC 在输入采样率下工作
        const CTCSS_DECODE_OFFSET: f32 = 160.55;
        // SDR++ FrequencyXlator 使用负偏移：setOffset(-_offset, _inSamplerate)
        let ddc_phase_step = -2.0 * std::f32::consts::PI * CTCSS_DECODE_OFFSET / input_sample_rate;
        
        // SDR++ 使用 500Hz 采样率处理 CTCSS
        const CTCSS_SAMPLE_RATE: f32 = 500.0;
        let decim_ratio = (input_sample_rate / CTCSS_SAMPLE_RATE) as usize;
        
        // 设计低通滤波器：截止频率 300Hz，用于从频偏中提取亚音
        // 在 input_sample_rate 采样率下
        let lp_cutoff_norm = 300.0 / (input_sample_rate / 2.0);
        let lp_coeffs = design_fir_lowpass(127, lp_cutoff_norm.min(0.95));

        log::info!("CTCSS检测器(SDR++风格): 目标频率={}Hz 输入采样率={}Hz CTCSS采样率={}Hz 抽取比={} DDC偏移={}Hz 低通截止=300Hz",
            tone_freq, input_sample_rate, CTCSS_SAMPLE_RATE, decim_ratio, CTCSS_DECODE_OFFSET);

        Self {
            target_freq: tone_freq,
            input_sample_rate,
            ctcss_sample_rate: CTCSS_SAMPLE_RATE,
            threshold,
            ddc_phase: 0.0,
            ddc_phase_step,
            decim_count: 0,
            decim_ratio,
            mean: 0.0,
            var: 0.0,
            var_ok: false,
            min_freq: 0.0,
            max_freq: 0.0,
            detected: false,
            strength: 0.0,
            detected_freq: 0.0,
            sample_count: 0,
            mute: true,
            lp_state: vec![0.0f32; lp_coeffs.len()],
            lp_coeffs,
        }
    }

    /// 处理频率样本（FM解调后的频偏值），返回是否检测到 CTCSS
    /// SDR++ 风格：直接处理频率偏移样本
    /// 处理流程：低通滤波 -> DDC -> 抽取到 500Hz -> 运行均值/方差
    pub fn process_freq_sample(&mut self, freq_hz: f32) -> bool {
        // 1. 低通滤波：只保留亚音频率成分（<300Hz）
        let filtered = self.lp_filter(freq_hz);
        
        // 2. DDC（数字下变频）：将 160.55Hz 附近搬移到基带
        // 对于频率样本，DDC 就是简单的减法：freq - 160.55Hz
        const CTCSS_DECODE_OFFSET: f32 = 160.55;
        let ddc_out = filtered - CTCSS_DECODE_OFFSET;
        
        // 2. 抽取到 500Hz（SDR++ 采样率）
        self.decim_count += 1;
        if self.decim_count < self.decim_ratio {
            return self.detected;
        }
        self.decim_count = 0;

        // 3. 运行均值和方差计算（SDR++ 使用 0.95/0.05 EMA @ 500Hz）
        self.mean = 0.95 * self.mean + 0.05 * ddc_out;
        let err = ddc_out - self.mean;
        self.var = 0.95 * self.var + 0.05 * err * err;
        
        self.sample_count += 1;
        
        // 4. 每 50 个样本做一次检测决策（约 100ms @ 500Hz）
        if self.sample_count >= 50 {
            self.sample_count = 0;
            
            // 施密特触发器判断方差稳定性（SDR++ 使用 1000/1100）
            let var_threshold_low = 1000.0;
            let var_threshold_high = 1100.0;
            
            let new_var_ok = if self.var_ok {
                self.var < var_threshold_high
            } else {
                self.var < var_threshold_low
            };
            
            // SDR++: freq = mean + CTCSS_DECODE_OFFSET
            let freq = self.mean + CTCSS_DECODE_OFFSET;
            
            // 调试日志 - 增加详细度
            static mut DEBUG_COUNT: u32 = 0;
            unsafe {
                DEBUG_COUNT += 1;
                if DEBUG_COUNT % 5 == 0 {
                    let in_range = freq >= self.min_freq && freq <= self.max_freq;
                    log::info!("[CTCSS] 目标={}Hz ddc_out={:.2} mean={:.2} freq={:.2}Hz 方差={:.2} var_ok={} 范围=[{:.1},{:.1}] 在范围内={} 检测={}", 
                        self.target_freq, ddc_out, self.mean, freq, self.var, new_var_ok, 
                        self.min_freq, self.max_freq, in_range, self.detected);
                }
            }
            
            // 如果方差稳定，查找匹配的 CTCSS 频率
            if new_var_ok && (!self.var_ok || freq < self.min_freq || freq > self.max_freq) {
                let detected = self.find_nearest_tone(freq);
                
                if detected > 0.0 {
                    self.detected_freq = detected;
                    
                    // 计算容限范围（SDR++ 风格）
                    let idx = self.find_tone_index(detected);
                    let left_bound = if idx > 0 { 
                        (Self::CTCSS_TONES[idx - 1] + detected) / 2.0 
                    } else { 
                        detected - 2.5 
                    };
                    let right_bound = if idx < Self::CTCSS_TONES.len() - 1 { 
                        (Self::CTCSS_TONES[idx + 1] + detected) / 2.0 
                    } else { 
                        detected + 2.5 
                    };
                    
                    self.min_freq = left_bound;
                    self.max_freq = right_bound;
                    
                    // 检查是否匹配目标频率
                    if self.target_freq > 0.0 {
                        self.detected = (detected - self.target_freq).abs() < 2.5;
                    } else {
                        self.detected = true;
                    }
                    
                    // 计算强度（基于方差）
                    self.strength = 1.0 - (self.var / 100.0).min(1.0);
                    self.mute = !self.detected;
                    
                    log::info!("[CTCSS检测] 检测到频率={}Hz 目标={}Hz 匹配={} 强度={:.2}%", 
                        detected, self.target_freq, self.detected, self.strength * 100.0);
                } else {
                    self.detected = false;
                    self.mute = true;
                    self.strength = 0.0;
                }
            }
            
            // 方差上升沿（信号不稳定）
            if !new_var_ok && self.var_ok {
                self.mute = true;
                self.detected = false;
                self.detected_freq = 0.0;
                self.strength = 0.0;
            }
            
            self.var_ok = new_var_ok;
        }
        
        self.detected
    }
    
    /// 标准 CTCSS 频率表
    const CTCSS_TONES: [f32; 50] = [
        67.0, 69.3, 71.9, 74.4, 77.0, 79.7, 82.5, 85.4, 88.5, 91.5,
        94.8, 97.4, 100.0, 103.5, 107.2, 110.9, 114.8, 118.8, 123.0, 127.3,
        131.8, 136.5, 141.3, 146.2, 150.0, 151.4, 156.7, 159.8, 162.2, 165.5,
        167.9, 171.3, 173.8, 177.3, 179.9, 183.5, 186.2, 189.9, 192.8, 196.6,
        199.5, 203.5, 206.5, 210.7, 218.1, 225.7, 229.1, 233.6, 241.8, 250.3,
    ];
    
    /// 查找最近的 CTCSS 频率
    fn find_nearest_tone(&self, freq: f32) -> f32 {
        if freq < Self::CTCSS_TONES[0] - 2.5 || freq > Self::CTCSS_TONES[Self::CTCSS_TONES.len() - 1] + 2.5 {
            return 0.0;
        }
        
        let mut left = 0;
        let mut right = Self::CTCSS_TONES.len() - 1;
        
        while right - left > 1 {
            let mid = (left + right) / 2;
            if Self::CTCSS_TONES[mid] < freq {
                left = mid;
            } else {
                right = mid;
            }
        }
        
        if (freq - Self::CTCSS_TONES[left]).abs() < (freq - Self::CTCSS_TONES[right]).abs() {
            Self::CTCSS_TONES[left]
        } else {
            Self::CTCSS_TONES[right]
        }
    }
    
    /// 查找频率在表中的索引
    fn find_tone_index(&self, freq: f32) -> usize {
        for (i, &tone) in Self::CTCSS_TONES.iter().enumerate() {
            if (tone - freq).abs() < 0.1 {
                return i;
            }
        }
        0
    }

    /// 批量处理频率样本
    pub fn process(&mut self, freq_samples: &[f32]) -> bool {
        for &freq in freq_samples {
            self.process_freq_sample(freq);
        }
        self.detected
    }
    
    /// 低通滤波（FIR）
    fn lp_filter(&mut self, sample: f32) -> f32 {
        let len = self.lp_state.len();
        for i in (1..len).rev() {
            self.lp_state[i] = self.lp_state[i - 1];
        }
        self.lp_state[0] = sample;
        self.lp_state.iter().zip(self.lp_coeffs.iter()).map(|(s, c)| s * c).sum()
    }

    /// 重置检测器状态
    pub fn reset(&mut self) {
        self.ddc_phase = 0.0;
        self.decim_count = 0;
        self.mean = 0.0;
        self.var = 0.0;
        self.var_ok = false;
        self.detected = false;
        self.strength = 0.0;
        self.detected_freq = 0.0;
        self.sample_count = 0;
        self.mute = true;
        self.min_freq = 0.0;
        self.max_freq = 0.0;
    }
}

/// 设计汉明窗FIR低通滤波器
/// - order: 滤波器阶数（奇数，tap数 = order + 1）
/// - normalized_cutoff: 归一化截止频率（0.0~1.0，相对于奈奎斯特频率）
fn design_fir_lowpass(order: usize, normalized_cutoff: f32) -> Vec<f32> {
    let n_taps = order + 1;
    let mut coeffs = vec![0.0f32; n_taps];
    let m = order as f32 / 2.0;
    let pi = std::f32::consts::PI;

    for i in 0..n_taps {
        let n = i as f32 - m;
        // sinc函数
        let sinc = if n.abs() < 1e-6 {
            2.0 * normalized_cutoff
        } else {
            (2.0 * normalized_cutoff * pi * n).sin() / (pi * n)
        };
        // 汉明窗
        let window = 0.54 - 0.46 * (2.0 * pi * i as f32 / order as f32).cos();
        coeffs[i] = sinc * window;
    }

    // 归一化，使得直流增益为1
    let sum: f32 = coeffs.iter().sum();
    if sum.abs() > 1e-9 {
        coeffs.iter_mut().for_each(|c| *c /= sum);
    }
    coeffs
}

/// 设计FIR希尔伯特变换滤波器系数
/// 生成 90 度相移的滤波器
/// taps: 抽头数（应为奇数）
fn design_hilbert_fir(taps: usize) -> Vec<f32> {
    let n = taps;
    let mut coeffs = vec![0.0f32; n];
    let center = (n - 1) / 2;
    let pi = std::f32::consts::PI;
    
    for i in 0..n {
        let k = i as i32 - center as i32;
        if k == 0 {
            // 中心点 = 0
            coeffs[i] = 0.0;
        } else if k % 2 == 0 {
            // 偶数索引 = 0
            coeffs[i] = 0.0;
        } else {
            // 奇数索引: 2/(π*k)
            let hk = 2.0 / (pi * k as f32);
            // 汉明窗
            let window = 0.54 - 0.46 * (2.0 * pi * i as f32 / (n as f32 - 1.0)).cos();
            coeffs[i] = hk * window;
        }
    }
    
    coeffs
}

/// 获取 rtl_sdr 日志文件内容（请求时读取最后 N 行）
pub fn get_rtlsdr_log(last_lines: usize) -> String {
    let log_path = std::env::temp_dir().join("speakplain_rtlsdr.log");
    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(last_lines);
            lines[start..].join("\n")
        }
        Err(e) => format!("日志文件读取失败: {} ({})", e, log_path.display()),
    }
}

/// VAD（语音活动检测）：判断音频片段是否含有语音
/// 使用短时能量法，超过阈值认为有语音
pub fn vad_detect(audio: &[f32], threshold: f32) -> bool {
    if audio.is_empty() { return false; }
    let rms = (audio.iter().map(|&x| x * x).sum::<f32>() / audio.len() as f32).sqrt();
    rms > threshold
}

// ──────────────────────────────────────────────────────────────────────────────
// SDR设备管理器
// ──────────────────────────────────────────────────────────────────────────────

/// SDR设备管理器
pub struct SdrManager {
    config: Arc<Mutex<SdrConfig>>,
    /// rtl_sdr 进程句柄（连接后持有，断开后为 None）
    device: Arc<Mutex<Option<hw::RtlSdrProcess>>>,
    streaming: Arc<AtomicBool>,
    audio_stream: Arc<Mutex<Option<StreamHandle>>>,
    /// 实时信号强度（原子存储，供状态查询）
    signal_strength_raw: Arc<AtomicU32>,
    /// VAD状态（是否检测到语音）
    vad_active: Arc<AtomicBool>,
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
    audio_queue_len: Arc<AtomicU32>,
    /// 调试：音频输出采样率
    out_sample_rate: Arc<AtomicU32>,
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

    fn is_device_connected(&self) -> bool {
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
        let mut devices = hw::list_devices_hw()?;
        let connected_index = self.config.lock().device_index;
        for dev in &mut devices {
            dev.is_connected = Some(dev.index) == connected_index && self.is_device_connected();
        }
        Ok(devices)
    }

    /// 连接SDR设备（启动 rtl_sdr 子进程）
    pub fn connect(&self, device_index: u32) -> Result<()> {
        // 先确保之前的进程已清理
        *self.device.lock() = None;

        let cfg = self.config.lock().clone();
        log::info!("[SDR连接] 使用配置: 频率={}MHz, 增益={}dB, 自动增益={}, PPM={}, CTCSS={}Hz",
            cfg.frequency_mhz, cfg.gain_db, cfg.auto_gain, cfg.ppm_correction, cfg.ctcss_tone);
        let proc = hw::connect_hw(device_index, &cfg)?;
        *self.device.lock() = Some(proc);
        self.config.lock().device_index = Some(device_index);
        log::info!("SDR设备连接成功，设备索引: {}", device_index);
        Ok(())
    }

    /// 断开SDR设备
    pub fn disconnect(&self) -> Result<()> {
        self.stop_stream()?;
        *self.device.lock() = None; // Drop RtlSdrProcess → kill child
        self.config.lock().device_index = None;
        self.signal_strength_raw.store(0, Ordering::Relaxed);
        self.vad_active.store(false, Ordering::Relaxed);
        log::info!("SDR设备已断开");
        Ok(())
    }

    /// 设置接收频率（支持22~1100MHz）
    /// 注意：rtl_sdr 不支持运行时改频，需要断开重连
    pub fn set_frequency(&self, freq_mhz: f64) -> Result<()> {
        if freq_mhz < 22.0 || freq_mhz > 1100.0 {
            anyhow::bail!("频率必须在22MHz~1100MHz范围内");
        }
        self.config.lock().frequency_mhz = freq_mhz;
        log::info!("频率设置为 {} MHz (下次连接时生效)", freq_mhz);
        Ok(())
    }

    /// 设置增益（0~40 dB）
    /// 注意：rtl_sdr 不支持运行时改增益，需要断开重连
    pub fn set_gain(&self, gain_db: i32) -> Result<()> {
        if gain_db < 0 || gain_db > 40 {
            anyhow::bail!("增益必须在0~40 dB范围内");
        }
        let mut cfg = self.config.lock();
        cfg.gain_db = gain_db;
        cfg.auto_gain = false;
        log::info!("增益设置为 {} dB (下次连接时生效)", gain_db);
        Ok(())
    }

    /// 设置自动增益控制（AGC）
    /// 注意：rtl_sdr 不支持运行时改AGC，需要断开重连
    pub fn set_auto_gain(&self, enabled: bool) -> Result<()> {
        self.config.lock().auto_gain = enabled;
        log::info!("自动增益(AGC) {} (下次连接时生效)", if enabled { "开启" } else { "关闭" });
        Ok(())
    }

    /// 设置PPM频率校正（补偿晶振误差，典型值-50~+50 ppm）
    /// 注意：rtl_sdr 不支持运行时改PPM，需要断开重连
    pub fn set_ppm_correction(&self, ppm: i32) -> Result<()> {
        self.config.lock().ppm_correction = ppm;
        log::info!("PPM校正设置为 {} ppm (下次连接时生效)", ppm);
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
    pub fn set_ctcss_tone(&self, tone_hz: f32) {
        self.config.lock().ctcss_tone = tone_hz;
        // 如果正在运行，更新检测器
        if tone_hz > 0.0 {
            let sample_rate = self.out_sample_rate.load(Ordering::Relaxed) as f32;
            if sample_rate > 0.0 {
                let threshold = self.config.lock().ctcss_threshold;
                *self.ctcss_detector.lock() = Some(CtcssDetector::new(tone_hz, sample_rate, threshold));
                log::info!("CTCSS 设置为 {} Hz", tone_hz);
            }
        } else {
            *self.ctcss_detector.lock() = None;
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

    /// 启动IQ数据采集与DSP处理流（核心管线）
    ///
    /// 工作原理：
    /// 1. 开启cpal音频输出流（用于将解调音频路由到VB-Cable等虚拟麦克风）
    /// 2. 真实硬件模式下，另起线程读取RTL-SDR的IQ数据，经DSP管线处理后
    ///    写入音频输出流并存入audio_buffer供ASR消费
    pub fn start_stream(&self) -> Result<()> {
        if self.streaming.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !self.is_device_connected() {
            anyhow::bail!("SDR设备未连接");
        }

        let host = cpal::default_host();
        let output_device = {
            let target_name = self.config.lock().output_device.clone();
            if target_name.is_empty() {
                host.default_output_device().context("无法获取默认输出设备")?
            } else {
                let mut devices = host.output_devices()?;
                devices
                    .find(|d| d.name().map(|n| n == target_name).unwrap_or(false))
                    .context(format!("输出设备 '{}' 未找到", target_name))?
            }
        };

        // 使用设备默认支持的配置，避免 "not supported" 错误
        let default_cfg = output_device.default_output_config()
            .context("无法获取音频设备默认配置")?;
        let out_sample_rate = default_cfg.sample_rate().0;
        let out_channels = default_cfg.channels() as usize;
        let stream_config = cpal::StreamConfig {
            channels: default_cfg.channels(),
            sample_rate: default_cfg.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        log::info!("SDR音频输出: {}Hz {}ch 设备: {} 配置={:?}",
            out_sample_rate, out_channels,
            output_device.name().unwrap_or_default(), default_cfg);
        // 记录实际输出采样率供调试查询
        self.out_sample_rate.store(out_sample_rate, Ordering::Relaxed);

        // 共享音频队列（DSP线程写入，cpal回调消费）
        // 容量 = 1秒音频，防止累积延迟
        let queue_cap = out_sample_rate as usize;
        let audio_queue: Arc<Mutex<std::collections::VecDeque<f32>>> =
            Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(queue_cap)));
        let audio_queue_write = audio_queue.clone();
        let audio_buffer_ref = self.audio_buffer.clone();
        let signal_ref = self.signal_strength_raw.clone();
        let vad_ref = self.vad_active.clone();
        let audio_queue_len_ref = self.audio_queue_len.clone();
        let diag_audio_rms_ref = self.diag_audio_rms.clone();
        let diag_iq_range_ref = self.diag_iq_range.clone();
        let diag_iq_dc_i_ref = self.diag_iq_dc_i.clone();
        let ctcss_detected_ref = self.ctcss_detected.clone();
        let ctcss_strength_ref = self.ctcss_strength.clone();

        // DSP配置快照
        let cfg_snap = self.config.lock().clone();
        let vad_threshold = cfg_snap.vad_threshold;
        let on_speech_end_ref = self.on_speech_end.clone();
        let on_signal_ref = self.on_signal.clone();
        let on_vad_change_ref = self.on_vad_change.clone();

        // cpal输出回调：从队列取音频样本推送到音频设备
        // 设备可能是多声道，每帧重复同一样本填满所有声道
        let audio_queue_read = audio_queue.clone();
        let stream = output_device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut q = audio_queue_read.lock();
                let mut chunks = data.chunks_mut(out_channels);
                while let Some(frame) = chunks.next() {
                    let sample = q.pop_front().unwrap_or(0.0);
                    for ch in frame.iter_mut() {
                        *ch = sample;
                    }
                }
            },
            move |err| {
                log::error!("SDR音频输出流错误: {}", err);
            },
            None,
        )?;
        stream.play()?;
        *self.audio_stream.lock() = Some(StreamHandle(stream));
        self.streaming.store(true, Ordering::Relaxed);

        // 启动IQ读取线程（从 rtl_sdr stdout 读取 IQ 流）
        // 从 device 中取出 stdout，避免 Arc<Mutex> 导致的死锁问题
        let stdout_opt = self.device.lock().as_mut().and_then(|proc| proc.stdout.take());

        if let Some(mut stdout) = stdout_opt {
            let streaming_flag = self.streaming.clone();
            let sample_rate = cfg_snap.sample_rate;
            let demod_mode = cfg_snap.demod_mode.clone();
            let audio_out_rate = out_sample_rate;
            let bandwidth = cfg_snap.bandwidth;
            let call_test_mode_flag = self.call_test_mode.clone();
            let audio_queue_len_flag = audio_queue_len_ref.clone();

            std::thread::spawn(move || {
                use std::io::Read;
                let mut pipeline = DspPipeline::new(sample_rate, audio_out_rate, demod_mode, bandwidth);
                // IQ缓冲区：增大到500ms的数据，防止rtl_sdr数据丢失
                // rtl_sdr @ 2.4MHz = 4.8MB/s，500ms = 2.4MB
                let buf_size = (sample_rate / 2) as usize * 2; // 500ms的IQ字节
                let mut iq_buf = vec![0u8; buf_size];
                let mut prev_vad = false;
                let silence_timeout_frames: u32 = 3;
                let mut silence_frames: u32 = 0;
                let call_test_mode_ref = call_test_mode_flag;
                let audio_queue_len_ref = audio_queue_len_flag;
                let diag_audio_rms_ref = diag_audio_rms_ref;
                let diag_iq_range_ref = diag_iq_range_ref;
                let diag_iq_dc_i_ref = diag_iq_dc_i_ref;
                let ctcss_detected_ref = ctcss_detected_ref;
                let ctcss_strength_ref = ctcss_strength_ref;
                let mut diag_frame_count: u32 = 0;
                let mut first_frame_logged = false;
                let mut ctcss_detector: Option<CtcssDetector> = None;

                // 记录首次读取成功
                let mut first_read_logged = false;
                
                while streaming_flag.load(Ordering::Relaxed) {
                    // 从 stdout 读取 IQ 数据（使用 read 而非 read_exact，避免阻塞）
                    let n = match stdout.read(&mut iq_buf) {
                        Ok(n) if n > 0 => {
                            if !first_read_logged {
                                first_read_logged = true;
                                log::info!("[IQ读取] 首次成功读取 {} 字节", n);
                            }
                            n
                        }
                        Ok(0) => {
                            // 读取到0字节，可能是进程退出
                            if first_read_logged {
                                log::warn!("rtl_sdr stdout 读取0字节，进程可能已退出");
                            } else {
                                log::debug!("rtl_sdr stdout 等待数据中...");
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            continue;
                        }
                        Ok(_) => {
                            log::warn!("rtl_sdr stdout EOF，进程可能已退出");
                            break; // 退出线程
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                            log::warn!("rtl_sdr stdout 管道断开，进程已退出");
                            break; // 退出线程
                        }
                        Err(e) => {
                            log::error!("rtl_sdr 读取失败: {}", e);
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            continue;
                        }
                    };

                    let iq_bytes = &iq_buf[..n];

                    let dsp_output = pipeline.process(&iq_bytes);
                    let rms = pipeline.signal_rms;
                    signal_ref.store(rms.to_bits(), Ordering::Relaxed);
                    
                    // 提取音频样本用于后续处理
                    let audio_samples = dsp_output.audio;
                    let freq_samples = dsp_output.freq_samples;

                    // CTCSS 检测处理（SDR++ 风格：使用FM解调后的频率样本）
                    let ctcss_tone = cfg_snap.ctcss_tone;
                    let ctcss_threshold = cfg_snap.ctcss_threshold;
                    let is_call_test = call_test_mode_ref.load(Ordering::Relaxed);
                    
                    let (ctcss_detected, ctcss_strength) = if ctcss_tone > 0.0 {
                        // 延迟初始化检测器（使用stage1_rate采样率）
                        if ctcss_detector.is_none() {
                            ctcss_detector = Some(CtcssDetector::new(ctcss_tone, pipeline.stage1_rate as f32, ctcss_threshold));
                            log::info!("CTCSS检测器(SDR++风格)初始化: {} Hz, 输入采样率 {} Hz, 门限={}", 
                                ctcss_tone, pipeline.stage1_rate, ctcss_threshold);
                        }
                        if let Some(ref mut detector) = ctcss_detector {
                            // 处理频率样本（SDR++ 风格）
                            detector.process(&freq_samples);
                            (detector.detected, detector.strength)
                        } else {
                            (false, 0.0)
                        }
                    } else {
                        // 未设置CTCSS，始终返回检测通过
                        (true, 1.0)
                    };

                    // 存储 CTCSS 检测状态
                    ctcss_detected_ref.store(ctcss_detected, Ordering::Relaxed);
                    ctcss_strength_ref.store(ctcss_strength.to_bits(), Ordering::Relaxed);

                    // CTCSS 静音控制（参考 SDR++）：
                    // - CTCSS 检测器只输出 mute 状态
                    // - 主音频路径根据 mute 状态决定是否静音
                    // - 未设置 CTCSS 或通话测试模式时，始终不静音
                    let ctcss_mute = ctcss_tone > 0.0 && !ctcss_detected && !is_call_test;
                    
                    // SDR++ 风格：如果设置了亚音但未检测到，静音音频（输出0）
                    let audio_samples_filtered: Vec<f32> = if ctcss_mute {
                        // 静音：输出全0（参考 SDR++ memset(out, 0, ...)）
                        vec![0.0f32; audio_samples.len()]
                    } else {
                        // 直通：原样输出（参考 SDR++ memcpy(out, in, ...)）
                        audio_samples.clone()
                    };

                    // 首帧详细诊断：打印原始IQ字节值和统计
                    if !first_frame_logged {
                        first_frame_logged = true;
                        let sample16: Vec<u8> = iq_bytes.iter().take(32).cloned().collect();
                        // 计算前16个IQ对的均值
                        let mean_i = iq_bytes.chunks_exact(2).take(500)
                            .map(|c| c[0] as f32).sum::<f32>() / 500.0;
                        let mean_q = iq_bytes.chunks_exact(2).take(500)
                            .map(|c| c[1] as f32).sum::<f32>() / 500.0;
                        let max_i = iq_bytes.chunks_exact(2).take(500)
                            .map(|c| c[0]).max().unwrap_or(0);
                        let min_i = iq_bytes.chunks_exact(2).take(500)
                            .map(|c| c[0]).min().unwrap_or(255);
                        log::info!(
                            "[首帧IQ诊断] 字节数={} 帧前32字节={:?} \
                             I均值={:.1} Q均值={:.1}(正常均为127.4) I范围=[{},{}] \
                             RMS={:.4} 音频样本数={}",
                            iq_bytes.len(), sample16,
                            mean_i, mean_q, min_i, max_i,
                            rms, audio_samples.len()
                        );
                        if (mean_i - 127.4).abs() > 10.0 {
                            log::warn!("[诊断] IQ均值偏差较大（{:.1}），可能为全0或全127，请检查rtl_sdr连接。", mean_i);
                        }
                        if (max_i as i16 - min_i as i16) < 5 {
                            log::warn!("[诊断] IQ范围过小（{}），可能设备没有在返回IQ数据！请检查rtl_sdr进程是否正常运行。", max_i as i16 - min_i as i16);
                        }
                        
                        // [首帧音频诊断] - 验证音频样本正常性
                        if !audio_samples.is_empty() {
                            let audio_mean = audio_samples.iter().sum::<f32>() / audio_samples.len() as f32;
                            let audio_max = audio_samples.iter().cloned().fold(f32::MIN, f32::max);
                            let audio_min = audio_samples.iter().cloned().fold(f32::MAX, f32::min);
                            let audio_rms = (audio_samples.iter().map(|&x| x * x).sum::<f32>() / audio_samples.len() as f32).sqrt();
                            log::info!(
                                "[首帧音频诊断] 样本数={} 均值={:.4} 范围=[{:.4},{:.4}] RMS={:.4} 状态={}",
                                audio_samples.len(), audio_mean, audio_min, audio_max, audio_rms,
                                if audio_rms > 0.001 { "正常" } else { "信号弱" }
                            );
                        } else {
                            log::warn!("[首帧音频诊断] 音频样本为空，请检查FM解调是否正常");
                        }
                    }

                    // 更新诊断指标
                    diag_audio_rms_ref.store(pipeline.diag_audio_rms.to_bits(), Ordering::Relaxed);
                    diag_iq_range_ref.store(pipeline.diag_iq_range.to_bits(), Ordering::Relaxed);
                    diag_iq_dc_i_ref.store(pipeline.diag_iq_dc_i.to_bits(), Ordering::Relaxed);

                    // 每50帧（约5秒）输出一次诊断日志
                    diag_frame_count += 1;
                    if diag_frame_count % 50 == 0 {
                        let audio_rms_db = if pipeline.diag_audio_rms > 1e-7 {
                            20.0 * pipeline.diag_audio_rms.log10()
                        } else { -99.0 };
                        let ctcss_status = if ctcss_tone > 0.0 {
                            format!("CTCSS={}Hz 检测={} 强度={:.2}", ctcss_tone, ctcss_detected, ctcss_strength)
                        } else {
                            "CTCSS=未启用".to_string()
                        };
                        log::info!(
                            "[SDR诊断] 帧#{} IQ功率={:.4} IQ范围={:.3} DC_I={:.4} \
                             音频RMS={:.5}({:.1}dB) VAD={} 队列={} {}",
                            diag_frame_count,
                            rms,
                            pipeline.diag_iq_range,
                            pipeline.diag_iq_dc_i,
                            pipeline.diag_audio_rms,
                            audio_rms_db,
                            vad_ref.load(Ordering::Relaxed),
                            audio_queue_len_ref.load(Ordering::Relaxed),
                            ctcss_status
                        );
                    }

                    // 推送信号强度回调（不论是否有语音）
                    let signal_val = (rms * 6.0).powf(0.65).min(1.0);
                    if let Some(ref cb) = *on_signal_ref.lock() {
                        cb(signal_val);
                    }

                    // VAD 检测使用原始音频（不受 CTCSS 静音影响）
                    // 参考 SDR++：CTCSS 只控制静音开关，不影响主信号路径的 VAD
                    let has_voice = if audio_samples.is_empty() {
                        false
                    } else {
                        vad_detect(&audio_samples, vad_threshold)
                    };
                    
                    // 诊断：每帧打印VAD信息（仅前10帧）
                    if diag_frame_count < 10 {
                        let rms = if audio_samples.is_empty() { 0.0 } else {
                            (audio_samples.iter().map(|&x| x*x).sum::<f32>() / audio_samples.len() as f32).sqrt()
                        };
                        log::info!("[VAD诊断#{}] RMS={:.5} 阈值={:.3} 有语音={} 音频样本数={} CTCSS检测={}", 
                            diag_frame_count, rms, vad_threshold, has_voice, audio_samples.len(), ctcss_detected);
                    }

                    // ── PTT 静音延迟逻辑 ──────────────────────────────────────
                    // 当前帧有语音：重置静音计数，标记 VAD 活跃
                    // 当前帧无语音：累加静音帧数，达到阈值才真正结束语音段
                    // 这样可以容忍字间短暂停顿，只有 PTT 松开（信号彻底消失）
                    // 连续 silence_timeout_frames 帧才触发 on_speech_end
                    let vad_active_now = if has_voice {
                        silence_frames = 0;
                        true
                    } else if prev_vad {
                        silence_frames += 1;
                        silence_frames < silence_timeout_frames  // 还在延迟窗口内，保持活跃
                    } else {
                        silence_frames = 0;
                        false
                    };

                    vad_ref.store(vad_active_now, Ordering::Relaxed);

                    // VAD状态变化时推送回调
                    if vad_active_now != prev_vad {
                        if let Some(ref cb) = *on_vad_change_ref.lock() {
                            cb(vad_active_now);
                        }
                    }

                    // 音频输出到播放队列（应用 CTCSS 静音）
                    {
                        let mut q = audio_queue_write.lock();
                        // 通话测试模式：始终播放音频（绕过VAD）；正常模式：根据VAD状态播放
                        let should_output_audio = is_call_test || vad_active_now;
                        if should_output_audio {
                            // 输出音频（已应用 CTCSS 静音）
                            for s in &audio_samples_filtered { q.push_back(*s); }
                        } else {
                            // 推静音样本保持播放流畅滚
                            for _ in 0..audio_samples_filtered.len() { q.push_back(0.0); }
                        }
                        // 防止队列超过 1秒导致延迟累积：如果超过容量上限则丢弃老数据
                        while q.len() > queue_cap { q.pop_front(); }
                        audio_queue_len_ref.store(q.len() as u32, Ordering::Relaxed);
                    }

                    // 语音活跃（含延迟窗口内）时累积音频用于 ASR
                    // 参考 SDR++：CTCSS 只控制静音，不影响语音检测和累积
                    // 累积原始音频（不应用 CTCSS 静音，确保 ASR 能接收到完整语音）
                    if vad_active_now {
                        let mut buf = audio_buffer_ref.lock();
                        buf.extend_from_slice(&audio_samples);
                    }

                    // 语音段真正结束：prev_vad 活跃 → 当前已不活跃（超出延迟窗口）
                    // 参考 SDR++：CTCSS 只控制静音开关，不影响 PTT 触发
                    if prev_vad && !vad_active_now {
                        let audio_data = {
                            let mut buf = audio_buffer_ref.lock();
                            std::mem::take(&mut *buf)
                        };
                        
                        // 通话测试模式下不触发 ASR
                        if !call_test_mode_ref.load(Ordering::Relaxed) {
                            // 检查音频长度是否足够（至少300ms）
                            let min_samples = (audio_out_rate as f32 * 0.3) as usize;
                            
                            if !audio_data.is_empty() && audio_data.len() >= min_samples {
                                log::info!("PTT语音段结束，触发ASR，样本数={}", audio_data.len());
                                if let Some(ref cb) = *on_speech_end_ref.lock() {
                                    cb(audio_data);
                                }
                            } else if !audio_data.is_empty() {
                                log::debug!("音频段过短，跳过ASR，样本数={}", audio_data.len());
                            }
                        } else {
                            // 通话测试模式：清空缓冲，不触发 ASR
                            drop(audio_data); // 直接丢弃
                            log::info!("PTT语音段结束（通话测试模式，跳过ASR）");
                        }
                    }
                    prev_vad = vad_active_now;
                }
                log::info!("SDR IQ读取线程已退出");
            });
        }

        log::info!("SDR音频流已启动");
        Ok(())
    }

    /// 停止音频流
    pub fn stop_stream(&self) -> Result<()> {
        if !self.streaming.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.streaming.store(false, Ordering::Relaxed);
        // 等待IQ读取线程退出（最多500ms）
        std::thread::sleep(std::time::Duration::from_millis(50));
        *self.audio_stream.lock() = None;
        self.vad_active.store(false, Ordering::Relaxed);
        log::info!("SDR音频流已停止");
        Ok(())
    }

    /// 取出ASR音频缓冲（消费式，取出后清空）
    pub fn take_audio_buffer(&self) -> Vec<f32> {
        let mut buf = self.audio_buffer.lock();
        std::mem::take(&mut *buf)
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
        }
    }

    /// 获取配置
    pub fn get_config(&self) -> SdrConfig {
        self.config.lock().clone()
    }

    /// 批量更新配置（频率/增益变化时同步到硬件）
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
