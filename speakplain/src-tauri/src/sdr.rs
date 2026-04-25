//! SDR设备管理模块
//! 支持RTL2832U设备，通过 rtl_sdr 进程读取IQ数据，参考ShinySDR架构实现DSP管线
//!
//! DSP管线：IQ原始数据 → NBFM解调 → FIR低通滤波 → 降采样(2.4MHz→16kHz) → VAD检测 → 输出
//!
//! 使用 rtl_sdr.exe（项目sdr/目录内置）直接读取USB设备数据，不使用网络TCP模式。

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use ringbuf::{HeapRb, traits::{Consumer, Observer, Producer, Split}};
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
struct StreamHandle(cpal::Stream);
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}

// ──────────────────────────────────────────────────────────────────────────────
// RTL-SDR 硬件连接封装
// 直接调用 librtlsdr 库（rtlsdr crate），复刻 SDR++ 架构
// ──────────────────────────────────────────────────────────────────────────────
mod hw {
    use super::*;
    use std::sync::mpsc::{channel, Sender};

    /// RTL-SDR 设备句柄（连接后持有）
    #[derive(Clone)]
    pub struct RtlSdrDeviceHandle {
        pub dev_index: u32,
        pub gain_list: Vec<i32>,
    }

    /// 运行时设备命令（用于读取线程中实时改参）
    #[derive(Debug, Clone)]
    pub enum DeviceCommand {
        SetFrequency(u32),   // Hz
        SetGain(i32),        // tenths of dB
        SetAutoGain(bool),
        SetPpm(i32),
    }

    /// 在增益列表中找到最接近目标增益的值（单位：tenths of dB）
    pub fn find_nearest_gain(gain_db: f32, gains: &[i32]) -> i32 {
        let target = (gain_db * 10.0).round() as i32;
        gains.iter()
            .min_by_key(|&&g| (g - target).abs())
            .copied()
            .unwrap_or(0)
    }

    /// 枚举 RTL-SDR 设备：直接调用 librtlsdr API
    pub fn list_devices_hw() -> Result<Vec<SdrDeviceInfo>> {
        let count = rtlsdr::get_device_count();
        let mut devices = Vec::new();
        for i in 0..count {
            let name = rtlsdr::get_device_name(i);
            let serial = rtlsdr::get_device_usb_strings(i)
                .map(|s| s.serial)
                .unwrap_or_default();
            // 尝试打开获取调谐器信息
            let tuner_name = if let Ok(mut dev) = rtlsdr::open(i as i32) {
                let t = dev.get_tuner_type();
                // 注意：不要显式调用 dev.close()，RTLSDRDevice 的 Drop 会自动释放
                // 显式 close + Drop 会导致 double-free → STATUS_ACCESS_VIOLATION
                t.1
            } else {
                "Unknown".to_string()
            };
            devices.push(SdrDeviceInfo {
                index: i as u32,
                name,
                tuner: tuner_name,
                serial,
                is_connected: false,
            });
        }
        if devices.is_empty() {
            log::warn!("RTL-SDR: 未发现设备。确认：1.已插USB  2.已用Zadig装WinUSB驱动  3.无其他SDR软件占用");
        }
        Ok(devices)
    }

    /// 连接 RTL-SDR 设备：验证设备可打开并获取信息
    pub fn connect_hw(device_index: u32, cfg: &SdrConfig) -> Result<RtlSdrDeviceHandle> {
        let mut dev = rtlsdr::open(device_index as i32)
            .map_err(|e| anyhow::anyhow!("无法打开RTL-SDR设备 #{}: {:?}", device_index, e))?;

        let sample_rate = cfg.sample_rate;
        let freq_hz = (cfg.frequency_mhz * 1e6) as u32;

        dev.set_sample_rate(sample_rate)
            .map_err(|e| anyhow::anyhow!("设置采样率失败: {:?}", e))?;
        dev.set_center_freq(freq_hz)
            .map_err(|e| anyhow::anyhow!("设置频率失败: {:?}", e))?;
        // 禁用 direct sampling（确保使用调谐器模式，而非直接采样I/Q引脚）
        if let Err(e) = dev.set_direct_sampling(rtlsdr::DirectSampling::Disabled) {
            log::warn!("设置DirectSampling失败: {:?}", e);
        }
        // PPM 校正：某些设备（如 FC0013）可能不支持或范围受限，失败时不阻断连接
        if let Err(e) = dev.set_freq_correction(cfg.ppm_correction) {
            log::warn!("设置PPM失败（设备可能不支持PPM校正）: {:?}", e);
        }

        let gain_list = dev.get_tuner_gains()
            .map_err(|e| anyhow::anyhow!("获取增益列表失败: {:?}", e))?;

        const MAX_GAIN_DB: f32 = 19.7;
        if cfg.auto_gain {
            dev.set_tuner_gain_mode(false)
                .map_err(|e| anyhow::anyhow!("设置自动增益失败: {:?}", e))?;
            log::info!("使用自动增益模式（推荐）");
        } else {
            let limited = cfg.gain_db.min(MAX_GAIN_DB);
            if (limited - cfg.gain_db).abs() > 0.01 {
                log::warn!("增益 {}dB 超过最大限制 {}dB，已限制为 {}dB",
                    cfg.gain_db, MAX_GAIN_DB, limited);
            }
            let nearest = find_nearest_gain(limited, &gain_list);
            dev.set_tuner_gain_mode(true)
                .map_err(|e| anyhow::anyhow!("设置手动增益模式失败: {:?}", e))?;
            dev.set_tuner_gain(nearest)
                .map_err(|e| anyhow::anyhow!("设置增益失败: {:?}", e))?;
            log::info!("增益设置为 {:.1} dB (硬件值: {} tenths-dB)", nearest as f32 / 10.0, nearest);
        }

        // 注意：不要显式调用 dev.close()，RTLSDRDevice 的 Drop 会自动释放
        // 显式 close + Drop 会导致 double-free → STATUS_ACCESS_VIOLATION
        drop(dev);

        log::info!("RTL-SDR设备连接验证成功，设备索引: {} 可用增益数: {}",
            device_index, gain_list.len());
        Ok(RtlSdrDeviceHandle {
            dev_index: device_index,
            gain_list,
        })
    }

    /// 启动 RTL-SDR 读取线程，返回命令发送端
    ///
    /// 复刻 SDR++ worker 线程：在线程中 open 设备、reset_buffer、read_sync 循环
    /// IQ 数据通过 channel 异步发送给 DSP 线程，读取与 DSP 并行执行
    pub fn start_read_thread(
        handle: &RtlSdrDeviceHandle,
        cfg: &SdrConfig,
        streaming: Arc<AtomicBool>,
        mut iq_callback: impl FnMut(&[u8]) + Send + 'static,
    ) -> Result<Sender<DeviceCommand>> {
        let (cmd_tx, cmd_rx) = channel::<DeviceCommand>();

        let dev_index = handle.dev_index;
        let sample_rate = cfg.sample_rate;
        let freq_hz = (cfg.frequency_mhz * 1e6) as u32;
        let ppm = cfg.ppm_correction;
        let auto_gain = cfg.auto_gain;
        let gain_db = cfg.gain_db;
        let gain_list = handle.gain_list.clone();

        // read_sync 每次读取最小单元（512字节 = 256个IQ样本 = ~0.1ms @ 2.4MHz）
        // 小缓冲 = 短等待时间 = IQ数据近乎实时进入DSP线程
        let read_len = 512 * 2; // 1024字节，约0.2ms数据

        // IQ 数据 channel：读取线程 → DSP线程
        // 容量设大，避免读取线程因channel满而丢包
        let (iq_tx, iq_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(64);
        let streaming_dsp = streaming.clone();

        // DSP 处理批大小：每积累约 10ms 数据（采样率/100）才批量处理
        // 太小 → DSP 调用过于频繁；太大 → 延迟高
        let dsp_batch_bytes = ((sample_rate as usize / 100) * 2).max(4096); // ~10ms
        // DSP 线程：累积足够数据后批量处理，与读取线程并行
        std::thread::spawn(move || {
            let mut accum: Vec<u8> = Vec::with_capacity(dsp_batch_bytes * 2);
            while streaming_dsp.load(Ordering::Relaxed) {
                match iq_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(buf) => {
                        accum.extend_from_slice(&buf);
                        // 累积够一批再处理，避免过于频繁的 DSP 调用
                        if accum.len() >= dsp_batch_bytes {
                            iq_callback(&accum);
                            accum.clear();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // 超时：如果有积累数据则立即处理，避免尾部延迟
                        if !accum.is_empty() {
                            iq_callback(&accum);
                            accum.clear();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // 处理剩余数据
            if !accum.is_empty() {
                iq_callback(&accum);
            }
            log::info!("DSP线程已退出");
        });

        std::thread::spawn(move || {
            let mut device = match rtlsdr::open(dev_index as i32) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("读取线程无法打开RTL-SDR设备: {:?}", e);
                    return;
                }
            };

            // 配置设备
            if let Err(e) = device.set_sample_rate(sample_rate) {
                log::error!("设置采样率失败: {:?}", e); return;
            }
            if let Err(e) = device.set_center_freq(freq_hz) {
                log::error!("设置频率失败: {:?}", e); return;
            }
            if let Err(e) = device.set_freq_correction(ppm) {
                log::warn!("读取线程设置PPM失败（设备可能不支持）: {:?}", e);
            }
            // 禁用 direct sampling（确保使用调谐器模式）
            if let Err(e) = device.set_direct_sampling(rtlsdr::DirectSampling::Disabled) {
                log::warn!("读取线程设置DirectSampling失败: {:?}", e);
            }
            if auto_gain {
                if let Err(e) = device.set_tuner_gain_mode(false) {
                    log::error!("设置自动增益失败: {:?}", e); return;
                }
            } else {
                let nearest = find_nearest_gain(gain_db, &gain_list);
                if let Err(e) = device.set_tuner_gain_mode(true) {
                    log::error!("设置手动增益模式失败: {:?}", e); return;
                }
                if let Err(e) = device.set_tuner_gain(nearest) {
                    log::error!("设置增益失败: {:?}", e); return;
                }
            }
            if let Err(e) = device.reset_buffer() {
                log::error!("重置缓冲区失败: {:?}", e); return;
            }

            log::info!("RTL-SDR 读取线程已启动，频率={}Hz 采样率={}Hz", freq_hz, sample_rate);

            let mut current_freq = freq_hz;
            let mut current_ppm = ppm;
            let mut first_read_logged = false;

            while streaming.load(Ordering::Relaxed) {
                // 处理运行时命令
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        DeviceCommand::SetFrequency(f) => if f != current_freq {
                            if device.set_center_freq(f).is_ok() {
                                current_freq = f;
                            }
                        },
                        DeviceCommand::SetGain(g) => {
                            if device.set_tuner_gain_mode(true).is_ok() && device.set_tuner_gain(g).is_ok() {
                                // 增益已实时更新
                            }
                        },
                        DeviceCommand::SetAutoGain(en) => {
                            if device.set_tuner_gain_mode(!en).is_ok() {
                                // AGC已实时更新
                            }
                        },
                        DeviceCommand::SetPpm(p) => if p != current_ppm {
                            if device.set_freq_correction(p).is_ok() {
                                current_ppm = p;
                            }
                        },
                    }
                }

                match device.read_sync(read_len) {
                    Ok(buf) => {
                        if !buf.is_empty() {
                            if !first_read_logged {
                                first_read_logged = true;
                                log::info!("[IQ读取] 首次成功读取 {} 字节", buf.len());
                            }
                            // 发送给 DSP 线程，如果 channel 满则丢弃最旧的再发送
                            let _ = iq_tx.try_send(buf);
                        }
                    }
                    Err(e) => {
                        log::error!("RTL-SDR 读取错误: {:?}", e);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }

            log::info!("RTL-SDR 读取线程已退出");
        });

        Ok(cmd_tx)
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
    /// AGC 目标电平
    agc_target: f32,
    /// AGC 攻击系数
    agc_attack: f32,
    /// AGC 释放系数
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
/// DSP处理结果（音频 + CTCSS用频偏样本）
#[derive(Clone, Debug)]
pub struct DspOutput {
    /// 音频样本（单声道）
    pub audio: Vec<f32>,
    /// stage1 抽取后的 IQ 样本（用于CTCSS检测，对齐SDR++）
    /// 采样率为 stage1_rate
    pub iq_samples: Vec<(f32, f32)>,
    /// FM 解调后的频偏样本（用于CTCSS检测，SDR++风格）
    /// 采样率为 stage1_rate
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
    /// NBFM/WBFM 前一个相位值（用于Quadrature解调，参考SDR++）
    prev_phase: f32,
    /// Quadrature 解调的 1/deviation（SDR++ 风格）
    inv_deviation: f32,
    /// 级1抽取因子（大整数降采样，降到约240kHz）
    pub stage1_decim: usize,
    /// 级1抽取后采样率
    pub stage1_rate: u32,
    /// 级2抽取因子（降到output_rate）
    stage2_decim: usize,
    /// 级1 FIR低通：I通道状态（复数FIR，分别对I/Q滤波）
    fir1_i_state: Vec<f32>,
    /// 级1 FIR低通：Q通道状态
    fir1_q_state: Vec<f32>,
    /// 级1 FIR系数
    fir1_coeffs: Vec<f32>,
    /// 级1 FIR I通道环形缓冲区位置
    fir1_i_pos: usize,
    /// 级1 FIR Q通道环形缓冲区位置
    fir1_q_pos: usize,
    /// 级2 FIR低通（截止频率 = output_rate*0.4，消除级2混叠+音频带外噪声）
    fir2_state: Vec<f32>,
    fir2_coeffs: Vec<f32>,
    /// 级2 FIR环形缓冲区位置
    fir2_pos: usize,
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
    /// stage1 抽取后的 IQ 样本缓冲区（用于CTCSS检测，对齐SDR++）
    iq_samples: Vec<(f32, f32)>,
    /// FM 解调后的频偏样本缓冲区（用于CTCSS检测，SDR++风格）
    freq_samples: Vec<f32>,
    /// 音频输出缓冲区
    audio_out: Vec<f32>,
    /// 音频高通滤波器状态1（二阶级联第一级）
    hp_state1: f32,
    /// 音频高通滤波器上一输入1（第一级）
    hp_prev_in1: f32,
    /// 音频高通滤波器状态2（二阶级联第二级）
    hp_state2: f32,
    /// 音频高通滤波器上一输入2（第二级）
    hp_prev_in2: f32,
    /// AGC 增益状态（自适应，避免削波）
    agc_gain: f32,
    /// 去加重 alpha 系数（预计算，基于 output_rate）
    deemph_alpha: f32,
    /// 音频低通滤波器状态（3400Hz，限制语音带宽）
    lp_audio_state: f32,
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

        // 级1 FIR：抗混叠滤波器，截止 = stage1_rate * 0.45
        // 过渡带放宽到50%，大幅降低抽头数（tap ≈ 3.8*2.4M/(108k*0.5) ≈ 67）
        let fir1_cutoff = (stage1_rate as f32 * 0.45).min(input_rate as f32 / 2.0);
        let fir1_trans = fir1_cutoff * 0.5; // 宽过渡带，降低抽头数
        let _cutoff1_norm = fir1_cutoff / input_rate as f32;
        let fir1_coeffs = design_fir_lowpass_sdrpp(fir1_cutoff, fir1_trans, input_rate as f64);

        // 级2 FIR：音频低通，截止 = min(带宽/2, output_rate*0.45)
        // 在 stage1_rate 频域设计，过渡带放宽到40%（tap ≈ 3.8*240k/(21600*0.4) ≈ 106）
        let half_bandwidth = (bandwidth as f32 / 2.0).min(stage1_rate as f32 / 2.0);
        let audio_bandwidth = half_bandwidth.min(output_rate as f32 * 0.45);
        let _cutoff2_norm = audio_bandwidth / stage1_rate as f32;
        let fir2_coeffs = design_fir_lowpass_sdrpp(audio_bandwidth, audio_bandwidth * 0.4, stage1_rate as f64);

        // SDR++ Quadrature 解调：deviation = bandwidth / 2.0
        let deviation = bandwidth as f32 / 2.0;
        let inv_deviation = stage1_rate as f32 / (2.0 * std::f32::consts::PI * deviation);

        log::debug!("DSP管线(SDR++风格): input={}Hz bandwidth={}Hz stage1={}/{} stage2={}/{} output={}Hz deviation={}Hz",
            input_rate, bandwidth, stage1_rate, stage1_decim, output_rate, stage2_decim, output_rate, deviation);

        let fir1_len = fir1_coeffs.len();
        let fir2_len = fir2_coeffs.len();

        // 去加重在 output_rate 下预计算（修复：之前错误地在 stage1_rate 下计算）
        const DEEMPH_TAU: f32 = 50e-6;
        let deemph_dt = 1.0 / output_rate as f32;
        let deemph_alpha = DEEMPH_TAU / (DEEMPH_TAU + deemph_dt);
        
        log::debug!("DSP管线: 去加重 alpha={:.4} @ {}Hz (50μs)", deemph_alpha, output_rate);
        
        Self {
            mode,
            iq_frontend,
            prev_phase: 0.0,
            inv_deviation,
            stage1_decim,
            stage1_rate,
            stage2_decim,
            fir1_i_state: vec![0.0f32; fir1_len],
            fir1_q_state: vec![0.0f32; fir1_len],
            fir1_coeffs,
            fir1_i_pos: 0,
            fir1_q_pos: 0,
            fir2_state: vec![0.0f32; fir2_len],
            fir2_coeffs,
            fir2_pos: 0,
            decim1_counter: 0,
            decim2_counter: 0,
            signal_rms: 0.0,
            deemph_state: 0.0,
            diag_iq_dc_i: 0.0,
            diag_iq_dc_q: 0.0,
            diag_iq_range: 0.0,
            diag_audio_rms: 0.0,
            iq_samples: Vec::new(),
            freq_samples: Vec::new(),
            audio_out: Vec::new(),
            hp_state1: 0.0,
            hp_prev_in1: 0.0,
            hp_state2: 0.0,
            hp_prev_in2: 0.0,
            agc_gain: 1.0,
            deemph_alpha,
            lp_audio_state: 0.0,
        }
    }

    /// 处理一批IQ原始字节，返回解调后的音频和IQ样本
    /// 音频样本用于播放，IQ样本用于CTCSS检测（对齐SDR++）
    pub fn process(&mut self, iq_bytes: &[u8]) -> DspOutput {
        let t_start = std::time::Instant::now();
        let n_samples = iq_bytes.len() / 2;
        let expected_audio_out = n_samples / (self.stage1_decim * self.stage2_decim) + 4;
        let expected_iq_out = n_samples / self.stage1_decim + 4;
        self.audio_out.clear();
        self.audio_out.reserve(expected_audio_out);
        self.iq_samples.clear();
        self.iq_samples.reserve(expected_iq_out);
        self.freq_samples.clear();
        self.freq_samples.reserve(expected_iq_out);

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
        self.diag_iq_dc_i = i_sum / n_samples as f32;
        self.diag_iq_dc_q = q_sum / n_samples as f32;
        self.diag_iq_range = i_max - i_min;

        // 2. 检测 IQ 饱和（仅用于诊断，不跳过处理）
        // IqFrontend 已做限幅和 AGC，即使原始 IQ 饱和也能正常处理
        let _iq_saturated = self.diag_iq_range > 1.9 && self.signal_rms > 1.0;

        for chunk in iq_bytes.chunks_exact(2) {
            // 原始 IQ 转换为浮点（-1.0 ~ 1.0）
            let i_raw = (chunk[0] as f32 - 127.4) / 128.0;
            let q_raw = (chunk[1] as f32 - 127.4) / 128.0;

            // IQ Frontend 预处理：DC 去除、IQ 平衡、限幅、AGC
            let (i, q) = self.iq_frontend.process(i_raw, q_raw);

            // 级1：复数 FIR 低通 + 抽取（SDR++ 风格：先滤波抽取，再解调）
            let fi = Self::fir_filter_ring(&mut self.fir1_i_state, &self.fir1_coeffs, i, &mut self.fir1_i_pos);
            let fq = Self::fir_filter_ring(&mut self.fir1_q_state, &self.fir1_coeffs, q, &mut self.fir1_q_pos);
            self.decim1_counter += 1;
            if self.decim1_counter < self.stage1_decim {
                continue;
            }
            self.decim1_counter = 0;

            // 保存 stage1 抽取后的 IQ 样本（用于CTCSS检测，对齐SDR++）
            self.iq_samples.push((fi, fq));

            // FM Quadrature 解调（在 stage1_rate 下，SDR++ 风格）
            let demod_sample = match self.mode {
                DemodMode::Nbfm | DemodMode::Wbfm => {
                    // SDR++ Quadrature 解调：
                    // out = normalizePhase(cphase - phase) * invDeviation
                    let current_phase = fq.atan2(fi);
                    let mut phase_diff = current_phase - self.prev_phase;
                    self.prev_phase = current_phase;

                    // normalizePhase：归一化到 [-π, π]
                    while phase_diff > std::f32::consts::PI {
                        phase_diff -= 2.0 * std::f32::consts::PI;
                    }
                    while phase_diff < -std::f32::consts::PI {
                        phase_diff += 2.0 * std::f32::consts::PI;
                    }

                    // 乘以 invDeviation，输出归一化频偏（SDR++ 风格）
                    phase_diff * self.inv_deviation
                }
                DemodMode::Am => {
                    (fi * fi + fq * fq).sqrt() - 0.5
                }
                DemodMode::Usb => fi,
                DemodMode::Lsb => fq,
            };

            // 保存频偏样本（用于CTCSS检测，去加重之前，SDR++风格）
            self.freq_samples.push(demod_sample);

            // 注意：去加重已移到 stage2 抽取后（output_rate），不再在 stage1_rate 下执行

            // 级2：FIR低通 + 抽取到 output_rate
            let fir2_out = Self::fir_filter_ring(&mut self.fir2_state, &self.fir2_coeffs, demod_sample, &mut self.fir2_pos);
            self.decim2_counter += 1;
            if self.decim2_counter < self.stage2_decim {
                continue;
            }
            self.decim2_counter = 0;

            // 去加重：WFM 广播有预加重，必须去加重；NBFM 通常无预加重
            let after_deemph = match self.mode {
                DemodMode::Wbfm => {
                    self.deemph_state = self.deemph_state * self.deemph_alpha + fir2_out * (1.0 - self.deemph_alpha);
                    self.deemph_state
                }
                _ => fir2_out,
            };

            // 音频低通：fir2 已做抗混叠，WFM 保留更宽带宽不做额外限制
            let lp_audio_out = after_deemph;

            // AGC：自适应增益，目标电平 0.3，避免削波
            const TARGET: f32 = 0.3;
            const ATTACK: f32 = 0.9;
            const RELEASE: f32 = 0.995;
            let amp = lp_audio_out.abs();
            if amp > TARGET {
                self.agc_gain = self.agc_gain * ATTACK + (TARGET / amp.max(0.001)) * (1.0 - ATTACK);
            } else {
                self.agc_gain = self.agc_gain * RELEASE + 1.0 * (1.0 - RELEASE);
            }
            let audio_sample = lp_audio_out * self.agc_gain;

            let audio_sample = audio_sample.clamp(-1.0, 1.0);
            self.audio_out.push(audio_sample);
        }

        // 计算解调后音频 RMS（诊断用）
        if !self.audio_out.is_empty() {
            let ar = (self.audio_out.iter().map(|&x| x * x).sum::<f32>() / self.audio_out.len() as f32).sqrt();
            self.diag_audio_rms = ar;
        }

        let elapsed = t_start.elapsed();
        if elapsed.as_millis() > 5 {
            log::warn!("[DSP耗时] process {} 样本耗时 {:.2}ms (过长！)", n_samples, elapsed.as_secs_f32() * 1000.0);
        }
        DspOutput {
            audio: self.audio_out.clone(),
            iq_samples: self.iq_samples.clone(),
            freq_samples: self.freq_samples.clone(),
        }
    }

    /// FIR卷积滤波（环形缓冲区实现，O(1)写入 + O(n)点积，无内存移位）
    fn fir_filter_ring(state: &mut [f32], coeffs: &[f32], sample: f32, pos: &mut usize) -> f32 {
        state[*pos] = sample;
        let n = coeffs.len();
        let mut sum = 0.0f32;
        // 从当前位置倒序读取，与系数正序相乘
        for i in 0..n {
            let idx = if *pos >= i { *pos - i } else { n + *pos - i };
            sum += state[idx] * coeffs[i];
        }
        *pos = (*pos + 1) % n;
        sum
    }
}

/// CTCSS 检测器（Goertzel 相干检测，处理 FM 解调后的频偏样本）
/// 
/// 核心思路（对齐 SDR++）：
/// FM 解调后的频偏样本中，CTCSS 是一个纯正弦波叠加在语音基带上。
/// 语音能量主要分布在 300Hz 以上，CTCSS 在 67~250Hz。
/// 因此：低通滤波（保留 <300Hz）→ 降采样 → Goertzel 检测目标频率 → 判定有无。
pub struct CtcssDetector {
    target_freq: f32,
    threshold: f32,

    // === 低通滤波（stage1_rate → 保留 <300Hz）===
    lp_state: Vec<f32>,
    lp_coeffs: Vec<f32>,

    // === 抽取到 1000Hz ===
    decim_ratio: usize,
    decim_count: usize,

    // === Goertzel 状态 ===
    goertzel_coeff: f32,
    goertzel_cosw: f32,
    goertzel_sinw: f32,
    goertzel_s1: f32,
    goertzel_s2: f32,

    // === 滑动窗口 ===
    sample_count: usize,
    window_size: usize,

    // === 平滑 ===
    smoothed_strength: f32,

    // === 施密特触发器：释放门限 ===
    release_threshold: f32,

    // === 无信号计数（连续低强度窗口数）===
    no_signal_count: u32,

    // === 上一个检测状态（用于状态变化诊断）===
    prev_detected: bool,

    // === 诊断计数 ===
    window_log_count: usize,

    // 检测结果
    pub detected: bool,
    pub strength: f32,
    pub detected_freq: f32,
}

impl CtcssDetector {
    /// Goertzel 目标采样率（Hz）
    const GOERTZEL_RATE: f32 = 1000.0;
    /// 低通截止频率（Hz），保留 CTCSS 最高 250Hz 并留裕量
    const LP_CUTOFF: f32 = 300.0;
    /// 低通过渡带（Hz）—— 放宽到 800Hz 降低抽头数（3.8*240k/800≈1140）
    const LP_TRANSITION: f32 = 800.0;
    /// Goertzel 窗口大小（1000Hz 下 200 样本 = 200ms）
    const WINDOW_SIZE: usize = 200;

    pub fn new(tone_freq: f32, input_sample_rate: f32, threshold: f32) -> Self {
        // 低通 FIR：截止 300Hz，在 stage1_rate 下
        let lp_coeffs = design_fir_lowpass_sdrpp(
            Self::LP_CUTOFF,
            Self::LP_TRANSITION,
            input_sample_rate as f64,
        );

        // 抽取到 1000Hz
        let decim_ratio = (input_sample_rate / Self::GOERTZEL_RATE) as usize;

        // Goertzel 参数（在 1000Hz 采样率下）
        let w = 2.0 * std::f32::consts::PI * tone_freq / Self::GOERTZEL_RATE;
        let cosw = w.cos();
        let sinw = w.sin();
        let coeff = 2.0 * cosw;

        log::info!(
            "CTCSS检测器(Goertzel): 目标={}Hz 输入率={}Hz 低通taps={} 抽取=1/{} GoertzelRate={}Hz 窗口={}ms",
            tone_freq,
            input_sample_rate,
            lp_coeffs.len(),
            decim_ratio,
            Self::GOERTZEL_RATE,
            Self::WINDOW_SIZE as f32 / Self::GOERTZEL_RATE * 1000.0,
        );

        Self {
            target_freq: tone_freq,
            threshold,
            lp_state: vec![0.0f32; lp_coeffs.len()],
            lp_coeffs,
            decim_ratio: decim_ratio.max(1),
            decim_count: 0,
            goertzel_coeff: coeff,
            goertzel_cosw: cosw,
            goertzel_sinw: sinw,
            goertzel_s1: 0.0,
            goertzel_s2: 0.0,
            sample_count: 0,
            window_size: Self::WINDOW_SIZE,
            smoothed_strength: 0.0,
            release_threshold: threshold * 0.5,
            no_signal_count: 0,
            prev_detected: false,
            window_log_count: 0,
            detected: false,
            strength: 0.0,
            detected_freq: 0.0,
        }
    }

    /// 处理单个频偏样本
    fn process_freq_sample(&mut self, sample: f32) {
        // 1. 低通滤波（去除语音高频，保留 CTCSS）
        let lp_out = Self::fir_filter_one(&mut self.lp_state, &self.lp_coeffs, sample);

        // 2. 抽取到 1000Hz
        self.decim_count += 1;
        if self.decim_count < self.decim_ratio {
            return;
        }
        self.decim_count = 0;

        // 3. Goertzel 迭代
        let s0 = self.goertzel_coeff * self.goertzel_s1 - self.goertzel_s2 + lp_out;
        self.goertzel_s2 = self.goertzel_s1;
        self.goertzel_s1 = s0;

        self.sample_count += 1;

        // 4. 窗口满，计算能量并判定
        if self.sample_count >= self.window_size {
            let real = self.goertzel_s1 - self.goertzel_s2 * self.goertzel_cosw;
            let imag = self.goertzel_s2 * self.goertzel_sinw;
            let power = real * real + imag * imag;

            // 振幅估计：A = sqrt(2 * power) / N
            // 对于振幅为 A 的正弦波，Goertzel power = A² * N² / 2
            let n = self.window_size as f32;
            let amplitude = (2.0 * power).sqrt() / n;

            // 映射 strength 到 0~1（0.01 振幅对应满强度，适配 WFM 大 deviation）
            let raw_strength = (amplitude / 0.01).min(1.0);

            // 平滑（IIR，更快释放：信号消失后约 2~3 个窗口释放）
            // 施密特触发器：检测用 threshold，释放用 release_threshold（threshold 的一半）
            let release_thr = self.release_threshold;
            if raw_strength < release_thr {
                // 信号弱：快速衰减
                self.no_signal_count += 1;
                if self.no_signal_count >= 3 {
                    // 连续 3 个窗口无信号，强制归零
                    self.smoothed_strength = 0.0;
                } else {
                    self.smoothed_strength = 0.3 * self.smoothed_strength;
                }
            } else {
                // 信号正常：正常平滑
                self.no_signal_count = 0;
                self.smoothed_strength = 0.3 * self.smoothed_strength + 0.7 * raw_strength;
            }
            self.strength = self.smoothed_strength;

            // 施密特触发器判定
            if self.strength >= self.threshold {
                self.detected = true;
            } else if self.strength < self.release_threshold {
                self.detected = false;
            }
            // 中间状态保持当前值
            self.detected_freq = if self.detected { self.target_freq } else { 0.0 };

            // 仅在状态变化时打印
            let state_changed = self.detected != self.prev_detected;
            self.prev_detected = self.detected;
            self.window_log_count += 1;
            if state_changed {
                log::info!("[CTCSS] {}Hz {} (strength={:.3})",
                    self.target_freq, if self.detected { "检测到" } else { "消失" }, self.smoothed_strength);
            }

            // 重置 Goertzel 状态（开始新窗口）
            self.goertzel_s1 = 0.0;
            self.goertzel_s2 = 0.0;
            self.sample_count = 0;
        }
    }

    /// 批量处理频偏样本
    pub fn process(&mut self, freq_samples: &[f32]) -> bool {
        for &sample in freq_samples {
            self.process_freq_sample(sample);
        }
        self.detected
    }

    fn fir_filter_one(state: &mut Vec<f32>, coeffs: &[f32], sample: f32) -> f32 {
        let len = state.len();
        for i in (1..len).rev() {
            state[i] = state[i - 1];
        }
        state[0] = sample;
        state.iter().zip(coeffs.iter()).map(|(s, c)| s * c).sum()
    }

    /// 重置检测器状态
    pub fn reset(&mut self) {
        self.decim_count = 0;
        self.goertzel_s1 = 0.0;
        self.goertzel_s2 = 0.0;
        self.sample_count = 0;
        self.smoothed_strength = 0.0;
        self.no_signal_count = 0;
        self.prev_detected = false;
        self.window_log_count = 0;
        self.detected = false;
        self.strength = 0.0;
        self.detected_freq = 0.0;
        for s in &mut self.lp_state {
            *s = 0.0;
        }
    }
}

/// 设计 SDR++ 风格 FIR 低通滤波器（Nuttall窗 + sinc）
/// - cutoff: 截止频率（Hz）
/// - trans_width: 过渡带宽度（Hz）
/// - sample_rate: 采样率（Hz）
fn design_fir_lowpass_sdrpp(cutoff: f32, trans_width: f32, sample_rate: f64) -> Vec<f32> {
    // SDR++ estimateTapCount = 3.8 * samplerate / transWidth
    let tap_count = ((3.8 * sample_rate / trans_width as f64).round() as usize).max(3);
    // 确保奇数抽头数
    let tap_count = if tap_count % 2 == 0 { tap_count + 1 } else { tap_count };
    // 限制最大抽头数为 31，确保 debug 模式下每帧 DSP < 5ms
    // 31 抽头对应 ~6dB/octave 滚降，对 FM 广播音质足够
    let n_taps = tap_count.min(31);
    let mut coeffs = vec![0.0f32; n_taps];
    let half = n_taps as f64 / 2.0;
    let omega = 2.0 * std::f64::consts::PI * cutoff as f64 / sample_rate;
    let corr = omega / std::f64::consts::PI;

    for i in 0..n_taps {
        let t = i as f64 - half + 0.5;
        // sinc 函数
        let sinc_val = if t.abs() < 1e-9 {
            1.0
        } else {
            (t * omega).sin() / (t * omega)
        };
        // Nuttall 窗
        let f = (2.0 * std::f64::consts::PI * i as f64) / (n_taps as f64 - 1.0);
        let a0 = 0.3635819;
        let a1 = 0.4891775;
        let a2 = 0.1365995;
        let a3 = 0.0106411;
        let window = a0 - a1 * f.cos() + a2 * (2.0 * f).cos() - a3 * (3.0 * f).cos();
        coeffs[i] = (sinc_val * window * corr) as f32;
    }

    // 归一化
    let sum: f32 = coeffs.iter().sum();
    if sum.abs() > 1e-9 {
        coeffs.iter_mut().for_each(|c| *c /= sum);
    }
    coeffs
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
    /// RTL-SDR 设备句柄（连接后持有，断开后为 None）
    device: Arc<Mutex<Option<hw::RtlSdrDeviceHandle>>>,
    /// 运行时命令发送端（用于读取线程实时改参）
    cmd_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<hw::DeviceCommand>>>>,
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
        let mut devices = hw::list_devices_hw()?;
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
        let handle = hw::connect_hw(device_index, &cfg)?;
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
            let _ = tx.send(hw::DeviceCommand::SetFrequency(freq_hz));
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
                let nearest = hw::find_nearest_gain(gain_db, &handle.gain_list);
                let _ = tx.send(hw::DeviceCommand::SetGain(nearest));
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
            let _ = tx.send(hw::DeviceCommand::SetAutoGain(enabled));
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
            let _ = tx.send(hw::DeviceCommand::SetPpm(ppm));
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

        // 对齐 SDR++ audio_sink：
        //   - 强制立体声2ch，采样率48000Hz（对应 Realtek Audio 硬件配置）
        //   - bufferFrames = sampleRate / 60 ≈ 800帧 (~16.7ms)，最小化延迟
        //   - 使用 RTAUDIO_MINIMIZE_LATENCY 等价：cpal Default buffer size
        //   - ringbuf 存立体声 interleaved f32，回调直接 copy_from_slice（等价 memcpy）
        const OUT_CHANNELS: usize = 2;
        const OUT_SAMPLE_RATE: u32 = 48000;
        // SDR++ bufferFrames = sampleRate / 60
        let buffer_frames = OUT_SAMPLE_RATE / 60; // 800帧 = 16.7ms
        let stream_config = cpal::StreamConfig {
            channels: OUT_CHANNELS as u16,
            sample_rate: cpal::SampleRate(OUT_SAMPLE_RATE),
            // Default 让驱动自选最小延迟，对应 SDR++ RTAUDIO_MINIMIZE_LATENCY
            buffer_size: cpal::BufferSize::Fixed(buffer_frames),
        };
        let out_sample_rate = OUT_SAMPLE_RATE;
        let out_channels = OUT_CHANNELS;
        log::info!("[SDR音频] SDR++对齐 - 设备:{} 采样率:{}Hz {}ch bufferFrames={} ({:.1}ms)",
            output_device.name().unwrap_or_default(),
            out_sample_rate, out_channels, buffer_frames,
            buffer_frames as f32 / out_sample_rate as f32 * 1000.0);
        self.out_sample_rate.store(out_sample_rate, Ordering::Relaxed);

        // SDR++ stereoPacker 等价：ringbuf 存立体声 interleaved (L, R) = (sample, sample)
        // 容量 = 1秒立体声样本（每帧2个f32），约够60次回调缓冲
        let queue_cap = out_sample_rate as usize * out_channels; // 1秒立体声
        let ring = HeapRb::<f32>::new(queue_cap);
        let (mut audio_prod, mut audio_cons) = ring.split();
        let audio_queue_len_ref = self.audio_queue_len.clone();

        // DSP配置快照
        let cfg_snap = self.config.lock().clone();

        // SDR++ callback 等价：
        //   count = stereoPacker.out.read();
        //   memcpy(outputBuffer, readBuf, nBufferFrames * sizeof(stereo_t));
        // Rust实现：从无锁ringbuf批量读取立体声样本直接填充data切片
        let stream = output_device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // data.len() = buffer_frames * channels (interleaved stereo)
                let available = audio_cons.occupied_len();
                if available < data.len() {
                    // 欠载：用静音填充缺失部分
                    if available == 0 {
                        // 完全欠载：静音（对应 SDR++ 无数据时不输出噪声）
                        data.fill(0.0);
                        return;
                    }
                    log::warn!("[音频欠载] 环形缓冲仅{}个f32，需{}个，补静音", available, data.len());
                }
                // SDR++ 等价 memcpy：直接从ringbuf批量复制到输出缓冲
                let copied = audio_cons.pop_slice(data);
                // 若不够则用0填充剩余（上面欠载判断已覆盖，这里做保底）
                if copied < data.len() {
                    data[copied..].fill(0.0);
                }
            },
            move |err| {
                log::error!("SDR音频输出流错误: {}", err);
            },
            None,
        )?;
        // 先标记 streaming 为 true，再启动读取线程，让数据先开始积累
        self.streaming.store(true, Ordering::Relaxed);

        // 启动IQ读取线程（直接调用 librtlsdr read_sync）
        let device_handle = {
            let dev = self.device.lock();
            dev.as_ref().cloned().context("SDR设备未连接")?
        };
        let streaming_flag = self.streaming.clone();
        let sample_rate = cfg_snap.sample_rate;
        let demod_mode = cfg_snap.demod_mode.clone();
        let audio_out_rate = out_sample_rate;
        let bandwidth = cfg_snap.bandwidth;
        let call_test_mode_flag = self.call_test_mode.clone();
        let audio_queue_len_flag = audio_queue_len_ref.clone();
        let signal_ref = self.signal_strength_raw.clone();
        let audio_buffer_ref = self.audio_buffer.clone();
        let vad_ref = self.vad_active.clone();
        let diag_audio_rms_ref = self.diag_audio_rms.clone();
        let diag_iq_range_ref = self.diag_iq_range.clone();
        let diag_iq_dc_i_ref = self.diag_iq_dc_i.clone();
        let ctcss_detected_ref = self.ctcss_detected.clone();
        let ctcss_strength_ref = self.ctcss_strength.clone();
        // 传入实时配置Arc，让DSP回调可以读取最新的ctcss_tone/ctcss_threshold
        let config_arc = self.config.clone();
        let vad_threshold = cfg_snap.vad_threshold;
        let on_speech_end_ref = self.on_speech_end.clone();
        let on_signal_ref = self.on_signal.clone();
        let on_vad_change_ref = self.on_vad_change.clone();

        let mut pipeline = DspPipeline::new(sample_rate, audio_out_rate, demod_mode, bandwidth);
        let mut prev_vad = false;
        let silence_timeout_frames: u32 = 3;
        let mut silence_frames: u32 = 0;
        let mut first_frame_logged = false;
        let mut ctcss_detector: Option<CtcssDetector> = None;
        let mut last_iq_time = std::time::Instant::now();

        let cmd_tx = hw::start_read_thread(
            &device_handle,
            &cfg_snap,
            streaming_flag.clone(),
            move |iq_bytes: &[u8]| {
                let _iq_interval = last_iq_time.elapsed();
                last_iq_time = std::time::Instant::now();
                let dsp_output = pipeline.process(iq_bytes);
                let _process_time = last_iq_time.elapsed();
                let rms = pipeline.signal_rms;
                signal_ref.store(rms.to_bits(), Ordering::Relaxed);
                
                // 提取音频样本和IQ样本用于后续处理
                // Squelch 静噪：基于 IQ RMS 判断有无信号
                // 修复：阈值从 0.03 提高到 0.15。噪声 floor RMS≈sqrt(0.0079)=0.089，
                // 原阈值 0.03 低于噪声 floor，导致 squelch 始终开启、持续输出噪声。
                // 0.15 略高于噪声 floor，确保无信号时 squelch 关闭。
                let squelch_open = pipeline.signal_rms > 0.15;
                let audio_samples: Vec<f32> = dsp_output.audio;
                let _iq_samples = dsp_output.iq_samples;

                // CTCSS 检测处理（Goertzel：使用 FM 解调后的频偏样本）
                // 实时读取配置，确保流运行期间修改亚音设置立即生效
                let (ctcss_tone, ctcss_threshold) = {
                    let cfg = config_arc.lock();
                    (cfg.ctcss_tone, cfg.ctcss_threshold)
                };
                let is_call_test = call_test_mode_flag.load(Ordering::Relaxed);
                
                // 始终初始化并运行CTCSS检测器（方便诊断），但只在Squelch开启时信任结果
                let (ctcss_detected, ctcss_strength) = if ctcss_tone > 0.0 {
                    // 延迟初始化检测器（使用stage1_rate采样率）
                    if ctcss_detector.is_none() {
                        ctcss_detector = Some(CtcssDetector::new(ctcss_tone, pipeline.stage1_rate as f32, ctcss_threshold));
                        log::info!("CTCSS检测器(Goertzel)初始化: {} Hz, 输入采样率 {} Hz, 门限={}", 
                            ctcss_tone, pipeline.stage1_rate, ctcss_threshold);
                    }
                    if let Some(ref mut detector) = ctcss_detector {
                        // 处理频偏样本（Goertzel 直接检测目标频率）
                        detector.process(&dsp_output.freq_samples);
                        if squelch_open {
                            // 有信号：使用检测器结果
                            (detector.detected, detector.strength)
                        } else {
                            // 无信号：显示检测器结果但标记为未检测（避免false positive）
                            (false, detector.strength)
                        }
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

                // CTCSS 静音控制：设置了亚音且未检测到时静音
                let ctcss_mute = ctcss_tone > 0.0 && !ctcss_detected;
                
                // SDR++ 风格：如果设置了亚音但未检测到，静音音频（输出0）
                let audio_samples_filtered: Vec<f32> = if ctcss_mute {
                    // 静音：输出全0（参考 SDR++ memset(out, 0, ...)）
                    vec![0.0f32; audio_samples.len()]
                } else {
                    // 直通：原样输出（参考 SDR++ memcpy(out, in, ...)）
                    audio_samples.clone()
                };

                // 首帧异常诊断：仅在有明显问题时输出 warn
                if !first_frame_logged {
                    first_frame_logged = true;
                    let mean_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0] as f32).sum::<f32>() / 500.0;
                    let max_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0]).max().unwrap_or(0);
                    let min_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0]).min().unwrap_or(255);
                    if (mean_i - 127.4).abs() > 10.0 {
                        log::warn!("[首帧诊断] IQ均值偏差较大（{:.1}），可能为全0或全127，请检查rtl_sdr连接。", mean_i);
                    }
                    if (max_i as i16 - min_i as i16) < 5 {
                        log::warn!("[首帧诊断] IQ范围过小（{}），可能设备没有在返回IQ数据！", max_i as i16 - min_i as i16);
                    }
                    if audio_samples.is_empty() {
                        log::warn!("[首帧诊断] 音频样本为空，请检查FM解调是否正常");
                    }
                    if dsp_output.freq_samples.is_empty() {
                        log::warn!("[首帧诊断] 频偏样本为空，CTCSS检测器无输入！");
                    }
                }

                // 更新诊断指标
                diag_audio_rms_ref.store(pipeline.diag_audio_rms.to_bits(), Ordering::Relaxed);
                diag_iq_range_ref.store(pipeline.diag_iq_range.to_bits(), Ordering::Relaxed);
                diag_iq_dc_i_ref.store(pipeline.diag_iq_dc_i.to_bits(), Ordering::Relaxed);

                // 帧处理完成

                // 推送信号强度回调：基于实际输出到扬声器的音频（audio_samples_filtered）
                // 与扬声器声音保持一致：CTCSS静音时为0，有声音时随音量变化
                let audio_out_rms = if audio_samples_filtered.is_empty() {
                    0.0f32
                } else {
                    (audio_samples_filtered.iter().map(|&x| x * x).sum::<f32>() / audio_samples_filtered.len() as f32).sqrt()
                };
                let signal_val = (audio_out_rms * 6.0).powf(0.65).min(1.0);
                if let Some(ref cb) = *on_signal_ref.lock() {
                    cb(signal_val);
                }

                // VAD 检测：直接使用 squelch 状态（基于 IQ 功率）
                // IQ 功率无人发射时 ≈0.017，有人发射时 ≈1.4，区分度远优于音频 RMS
                // squelch_open 阈值 0.15 已在上方基于实测噪底校准，无需用户调整
                let has_voice = squelch_open;

                // ── PTT 静音延迟逻辑 ──────────────────────────────────────
                // 当前帧有语音：重置静音计数，标记 VAD 活跃
                // 当前帧无语音：累加静音帧数，达到阈值才真正结束语音段
                // 这样可以容忍字间短暂停顿，只有 PTT 松开（信号彻底消失）
                // 连续 silence_timeout_frames 帧才触发 on_speech_end
                let was_in_silence_window = prev_vad && silence_frames > 0;
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
                // 特殊情况：在静音延迟窗口内重新检测到语音（PTT松开后立即再按），
                // vad_active_now 保持 true 不变，但需要通知前端重新开始计时
                if vad_active_now != prev_vad {
                    if let Some(ref cb) = *on_vad_change_ref.lock() {
                        cb(vad_active_now);
                    }
                } else if vad_active_now && was_in_silence_window && has_voice {
                    // 静音窗口内重新有信号：补发一次 true，让前端重置计时器
                    if let Some(ref cb) = *on_vad_change_ref.lock() {
                        cb(true);
                    }
                }

                // 推送音频样本到无锁环形缓冲区（DSP线程写，cpal回调读）
                // SDR++ stereoPacker 等价：单声道样本展开为立体声 interleaved (L=s, R=s)
                {
                    let n = audio_samples_filtered.len();
                    // 预分配立体声缓冲：每个单声道样本展开为 2 个 f32
                    let stereo_len = n * 2;
                    let free = audio_prod.vacant_len();
                    if free < stereo_len {
                        // 环形缓冲已满：跳过本批数据（保持实时性，避免延迟累积）
                        log::debug!("[音频丢弃] 环形缓冲已满 ({} free, need {})，丢弃本批", free, stereo_len);
                    } else {
                        // SDR++ stereoPacker等价：单声道 -> 立体声 interleaved
                        // 直接展开写入ringbuf，避免中间Vec分配
                        for &s in &audio_samples_filtered {
                            // (L, R) = (s, s) 对应 SDR++ stereo_t{l: s, r: s}
                            let _ = audio_prod.try_push(s);
                            let _ = audio_prod.try_push(s);
                        }
                        // 更新队列占用长度（单位：f32个数，包含左右声道）
                        audio_queue_len_flag.store(
                            (audio_prod.occupied_len() / 2) as u32, // 头投为帧数
                            Ordering::Relaxed);
                    }
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
                    if !call_test_mode_flag.load(Ordering::Relaxed) {
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
            },
        ).context("启动RTL-SDR读取线程失败")?;
        *self.cmd_tx.lock() = Some(cmd_tx);

        // 直接启动音频流（无需预填充，无锁设计天然支持起始欠载）
        stream.play()?;
        *self.audio_stream.lock() = Some(StreamHandle(stream));

        log::info!("SDR音频流已启动 (音频设备: {}Hz {}ch ringbuf容量={})",
            out_sample_rate, out_channels, queue_cap);
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
        *self.cmd_tx.lock() = None; // Drop sender → read thread channel closed
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
            bandwidth: cfg.bandwidth,
            auto_gain: cfg.auto_gain,
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
