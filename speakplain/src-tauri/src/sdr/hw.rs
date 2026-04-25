//! RTL-SDR 硬件连接封装
//! 直接调用 librtlsdr 库（rtlsdr crate），复刻 SDR++ 架构

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};

use crate::sdr::types::{SdrConfig, SdrDeviceInfo};

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
