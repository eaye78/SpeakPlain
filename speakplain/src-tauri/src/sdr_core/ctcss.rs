//! CTCSS 检测器 - IQ 域实现
//! 
//! 参考 SDR++ 的 ctcss_squelch.h：
//! 1. 从 CTCSS 解调音频中提取频率（运行均值）
//! 2. 计算方差判断信号稳定性
//! 3. 施密特触发器避免抖动
//! 4. 与标准 CTCSS 频率表匹配

/// 标准 CTCSS 频率表（Hz）
pub const CTCSS_TONES: [f32; 50] = [
    67.0, 69.3, 71.9, 74.4, 77.0, 79.7, 82.5, 85.4, 88.5, 91.5,
    94.8, 97.4, 100.0, 103.5, 107.2, 110.9, 114.8, 118.8, 123.0, 127.3,
    131.8, 136.5, 141.3, 146.2, 150.0, 151.4, 156.7, 159.8, 162.2, 165.5,
    167.9, 171.3, 173.8, 177.3, 179.9, 183.5, 186.2, 189.9, 192.8, 196.6,
    199.5, 203.5, 206.5, 210.7, 218.1, 225.7, 229.1, 233.6, 241.8, 250.3,
];

/// CTCSS 检测器
pub struct CtcssDetector {
    /// 目标频率（Hz），0 表示检测任意频率
    target_freq: f32,
    /// 运行均值（估计的频率值）
    mean: f32,
    /// 运行方差
    var: f32,
    /// 方差是否稳定（施密特触发器状态）
    var_ok: bool,
    /// 检测到的频率
    detected_freq: f32,
    /// 最小/最大频率（用于锁定检测）
    min_freq: f32,
    max_freq: f32,
    /// 静音状态
    mute: bool,
    /// 样本计数
    sample_count: usize,
    /// 检测状态
    pub detected: bool,
    /// 信号强度
    pub strength: f32,
}

impl CtcssDetector {
    /// 创建新的 CTCSS 检测器
    /// 
    /// - target_freq: 目标频率（Hz），0 表示检测任意有效 CTCSS
    pub fn new(target_freq: f32) -> Self {
        log::info!("CTCSS 检测器初始化: 目标频率={}Hz", 
            if target_freq > 0.0 { format!("{:.1}", target_freq) } else { "任意".to_string() });
        
        Self {
            target_freq,
            mean: 0.0,
            var: 0.0,
            var_ok: false,
            detected_freq: 0.0,
            min_freq: 0.0,
            max_freq: 0.0,
            mute: true,
            sample_count: 0,
            detected: false,
            strength: 0.0,
        }
    }
    
    /// 处理 CTCSS 解调后的音频样本
    /// 
    /// 输入是经过 CTCSS DDC + FM 解调后的频率值（相对 160.55Hz 偏移）
    pub fn process_sample(&mut self, freq_sample: f32) -> bool {
        // 计算绝对频率（加上 160.55Hz 偏移）
        let absolute_freq = freq_sample + 160.55;
        
        // 更新运行均值（EMA，时间常数约 20ms @ 500Hz）
        self.mean = 0.95 * self.mean + 0.05 * absolute_freq;
        
        // 更新运行方差
        let err = absolute_freq - self.mean;
        self.var = 0.95 * self.var + 0.05 * err * err;
        
        self.sample_count += 1;
        
        // 每 50 个样本做一次检测决策（约 100ms @ 500Hz）
        if self.sample_count >= 50 {
            self.sample_count = 0;
            
            // 施密特触发器判断方差稳定性
            // 方差小表示频率稳定（有持续的 CTCSS 信号）
            let var_threshold_low = 25.0;   // 进入稳定状态的阈值
            let var_threshold_high = 100.0; // 退出稳定状态的阈值
            
            let new_var_ok = if self.var_ok {
                self.var < var_threshold_high
            } else {
                self.var < var_threshold_low
            };
            
            // 如果方差稳定，检查频率是否在 CTCSS 范围内
            if new_var_ok && (!self.var_ok || self.mean < self.min_freq || self.mean > self.max_freq) {
                // 查找匹配的 CTCSS 频率
                let detected = self.find_nearest_tone(self.mean);
                
                if detected > 0.0 {
                    self.detected_freq = detected;
                    
                    // 计算容限范围
                    let idx = self.find_tone_index(detected);
                    let left_bound = if idx > 0 { 
                        (CTCSS_TONES[idx - 1] + detected) / 2.0 
                    } else { 
                        detected - 2.5 
                    };
                    let right_bound = if idx < CTCSS_TONES.len() - 1 { 
                        (CTCSS_TONES[idx + 1] + detected) / 2.0 
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
                    
                    // 更新静音状态
                    self.mute = !self.detected;
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
    
    /// 批量处理
    pub fn process(&mut self, samples: &[f32]) -> bool {
        for &s in samples {
            self.process_sample(s);
        }
        self.detected
    }
    
    /// 查找最近的 CTCSS 频率
    fn find_nearest_tone(&self, freq: f32) -> f32 {
        // 检查是否在 CTCSS 范围内
        if freq < CTCSS_TONES[0] - 2.5 || freq > CTCSS_TONES[CTCSS_TONES.len() - 1] + 2.5 {
            return 0.0;
        }
        
        // 二分查找最近的频率
        let mut left = 0;
        let mut right = CTCSS_TONES.len() - 1;
        
        while right - left > 1 {
            let mid = (left + right) / 2;
            if CTCSS_TONES[mid] < freq {
                left = mid;
            } else {
                right = mid;
            }
        }
        
        // 选择更近的
        if (freq - CTCSS_TONES[left]).abs() < (freq - CTCSS_TONES[right]).abs() {
            CTCSS_TONES[left]
        } else {
            CTCSS_TONES[right]
        }
    }
    
    /// 查找频率在表中的索引
    fn find_tone_index(&self, freq: f32) -> usize {
        for (i, &tone) in CTCSS_TONES.iter().enumerate() {
            if (tone - freq).abs() < 0.1 {
                return i;
            }
        }
        0
    }
    
    /// 获取检测到的频率
    pub fn get_detected_freq(&self) -> f32 {
        self.detected_freq
    }
    
    /// 是否应该静音
    pub fn should_mute(&self) -> bool {
        self.mute
    }
    
    /// 重置状态
    pub fn reset(&mut self) {
        self.mean = 0.0;
        self.var = 0.0;
        self.var_ok = false;
        self.detected_freq = 0.0;
        self.min_freq = 0.0;
        self.max_freq = 0.0;
        self.mute = true;
        self.sample_count = 0;
        self.detected = false;
        self.strength = 0.0;
    }
}
