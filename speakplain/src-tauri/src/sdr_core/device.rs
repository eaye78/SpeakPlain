//! RTL-SDR 设备管理 - 直接使用 librtlsdr 库
//! 
//! 参考 SDR++ 的 rtl_sdr_source 模块实现

use anyhow::{Context, Result};
use log;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;

/// RTL-SDR 设备封装
pub struct RtlSdrDevice {
    /// 设备索引
    dev_index: i32,
    /// 是否正在运行
    running: Arc<AtomicBool>,
    /// 工作线程
    worker_thread: Option<thread::JoinHandle<()>>,
    /// 采样率
    sample_rate: u32,
    /// 中心频率
    center_freq: u32,
    /// 增益
    gain: i32,
    /// PPM 校正
    ppm: i32,
}

impl RtlSdrDevice {
    /// 创建新的 RTL-SDR 设备实例
    pub fn new(dev_index: u32) -> Result<Self> {
        let idx = dev_index as i32;
        
        // 测试是否能打开设备
        log::info!("正在打开 RTL-SDR 设备 #{}...", dev_index);
        
        let mut device = rtlsdr::open(idx)
            .map_err(|e| anyhow::anyhow!("无法打开 RTL-SDR 设备 #{}: {:?}", dev_index, e))?;
        
        // 立即关闭，我们只是测试
        device.close().ok();
        
        log::info!("RTL-SDR 设备 #{} 已打开", dev_index);
        
        Ok(Self {
            dev_index: idx,
            running: Arc::new(AtomicBool::new(false)),
            worker_thread: None,
            sample_rate: 2_400_000,
            center_freq: 100_000_000,
            gain: 0,
            ppm: 0,
        })
    }
    
    /// 列出所有可用设备
    pub fn list_devices() -> Vec<(u32, String)> {
        let mut devices = Vec::new();
        
        let count = rtlsdr::get_device_count();
        for i in 0..count {
            let name = rtlsdr::get_device_name(i);
            devices.push((i as u32, name));
        }
        
        devices
    }
    
    /// 设置采样率
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<()> {
        self.sample_rate = rate;
        log::info!("RTL-SDR 采样率设置为 {} Hz", rate);
        Ok(())
    }
    
    /// 设置中心频率
    pub fn set_center_freq(&mut self, freq: u32) -> Result<()> {
        self.center_freq = freq;
        log::info!("RTL-SDR 中心频率设置为 {} Hz", freq);
        Ok(())
    }
    
    /// 设置增益
    pub fn set_gain(&mut self, gain: i32) -> Result<()> {
        self.gain = gain;
        if gain == 0 {
            log::info!("RTL-SDR 增益设置为自动");
        } else {
            log::info!("RTL-SDR 增益设置为 {} dB", gain as f32 / 10.0);
        }
        Ok(())
    }
    
    /// 设置 PPM 校正
    pub fn set_ppm(&mut self, ppm: i32) -> Result<()> {
        self.ppm = ppm;
        log::info!("RTL-SDR PPM 校正设置为 {}", ppm);
        Ok(())
    }
    
    /// 启动异步读取
    /// 
    /// callback: 接收 IQ 数据的回调函数
    pub fn start<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(&[u8]) + Send + 'static,
    {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }
        
        self.running.store(true, Ordering::Relaxed);
        
        let running = self.running.clone();
        let idx = self.dev_index;
        let sample_rate = self.sample_rate;
        let center_freq = self.center_freq;
        let gain = self.gain;
        let ppm = self.ppm;
        
        self.worker_thread = Some(thread::spawn(move || {
            // 打开设备
            let mut device = match rtlsdr::open(idx) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("无法打开 RTL-SDR 设备: {:?}", e);
                    return;
                }
            };
            
            // 配置设备
            if let Err(e) = device.set_sample_rate(sample_rate) {
                log::error!("设置采样率失败: {:?}", e);
                return;
            }
            
            if let Err(e) = device.set_center_freq(center_freq) {
                log::error!("设置中心频率失败: {:?}", e);
                return;
            }
            
            if let Err(e) = device.set_freq_correction(ppm) {
                log::error!("设置 PPM 校正失败: {:?}", e);
                return;
            }
            
            if gain == 0 {
                if let Err(e) = device.set_tuner_gain_mode(false) {
                    log::error!("设置自动增益失败: {:?}", e);
                    return;
                }
            } else {
                if let Err(e) = device.set_tuner_gain_mode(true) {
                    log::error!("设置手动增益模式失败: {:?}", e);
                    return;
                }
                if let Err(e) = device.set_tuner_gain(gain) {
                    log::error!("设置增益失败: {:?}", e);
                    return;
                }
            }
            
            // 重置缓冲区
            if let Err(e) = device.reset_buffer() {
                log::error!("重置缓冲区失败: {:?}", e);
                return;
            }
            
            log::info!("RTL-SDR 设备已启动，开始读取数据...");
            
            // 读取循环
            while running.load(Ordering::Relaxed) {
                match device.read_sync(16 * 1024) {
                    Ok(buf) => {
                        if !buf.is_empty() {
                            callback(&buf);
                        }
                    }
                    Err(e) => {
                        log::error!("RTL-SDR 读取错误: {:?}", e);
                        break;
                    }
                }
            }
            
            log::info!("RTL-SDR 读取线程已退出");
        }));
        
        Ok(())
    }
    
    /// 停止读取
    pub fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }
        
        self.running.store(false, Ordering::Relaxed);
        
        if let Some(thread) = self.worker_thread.take() {
            thread.join().ok();
        }
        
        log::info!("RTL-SDR 设备已停止");
        Ok(())
    }
    
    /// 获取设备信息
    pub fn get_info(&self) -> Result<String> {
        Ok(format!("RTL-SDR Device #{}", self.dev_index))
    }
}

impl Drop for RtlSdrDevice {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
