//! IQ 数据流 - 环形缓冲区实现
//! 
//! 参考 SDR++ 的 dsp::stream 实现

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar};

/// 复数 IQ 样本
#[derive(Clone, Copy, Debug)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }
    
    /// 计算相位
    pub fn phase(&self) -> f32 {
        self.im.atan2(self.re)
    }
    
    /// 计算幅度
    pub fn magnitude(&self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

/// IQ 数据流 - 线程安全的环形缓冲区
pub struct IQStream {
    /// 缓冲区
    buffer: Mutex<VecDeque<Complex>>,
    /// 条件变量用于阻塞读取
    cond: Condvar,
    /// 容量
    capacity: usize,
    /// 写入停止标志
    write_stopped: Mutex<bool>,
}

impl IQStream {
    /// 创建新的 IQ 流
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            cond: Condvar::new(),
            capacity,
            write_stopped: Mutex::new(false),
        })
    }
    
    /// 写入 IQ 数据（从原始字节转换）
    pub fn write_raw(&self, raw: &[u8]) -> bool {
        let mut buf = self.buffer.lock().unwrap();
        
        if *self.write_stopped.lock().unwrap() {
            return false;
        }
        
        // 将原始字节转换为复数样本
        for chunk in raw.chunks_exact(2) {
            let re = (chunk[0] as f32 - 127.4) / 128.0;
            let im = (chunk[1] as f32 - 127.4) / 128.0;
            
            // 如果缓冲区满，丢弃最老的数据
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            
            buf.push_back(Complex::new(re, im));
        }
        
        // 通知等待的读取者
        self.cond.notify_one();
        true
    }
    
    /// 写入 IQ 样本
    pub fn write(&self, samples: &[Complex]) -> bool {
        let mut buf = self.buffer.lock().unwrap();
        
        if *self.write_stopped.lock().unwrap() {
            return false;
        }
        
        for &sample in samples {
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            buf.push_back(sample);
        }
        
        self.cond.notify_one();
        true
    }
    
    /// 读取指定数量的 IQ 样本（阻塞）
    pub fn read(&self, count: usize) -> Option<Vec<Complex>> {
        let mut buf = self.buffer.lock().unwrap();
        
        // 等待足够的数据
        while buf.len() < count {
            if *self.write_stopped.lock().unwrap() {
                // 写入已停止，返回剩余数据
                if buf.is_empty() {
                    return None;
                }
                break;
            }
            
            buf = self.cond.wait(buf).unwrap();
        }
        
        // 读取数据
        let n = count.min(buf.len());
        let mut result = Vec::with_capacity(n);
        
        for _ in 0..n {
            result.push(buf.pop_front().unwrap());
        }
        
        Some(result)
    }
    
    /// 非阻塞读取
    pub fn try_read(&self, count: usize) -> Option<Vec<Complex>> {
        let mut buf = self.buffer.lock().unwrap();
        
        if buf.is_empty() {
            return None;
        }
        
        let n = count.min(buf.len());
        let mut result = Vec::with_capacity(n);
        
        for _ in 0..n {
            result.push(buf.pop_front().unwrap());
        }
        
        Some(result)
    }
    
    /// 获取当前缓冲区大小
    pub fn len(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }
    
    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// 清空缓冲区
    pub fn clear(&self) {
        self.buffer.lock().unwrap().clear();
    }
    
    /// 停止写入
    pub fn stop_writer(&self) {
        *self.write_stopped.lock().unwrap() = true;
        self.cond.notify_all();
    }
    
    /// 清除停止标志
    pub fn clear_write_stop(&self) {
        *self.write_stopped.lock().unwrap() = false;
    }
}
