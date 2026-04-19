//! DSP 处理模块
//! 
//! 参考 SDR++ 实现：
//! - DDC（数字下变频）：频率偏移 + 抽取
//! - FM 解调：正交解调（atan2 相位差）

use super::stream::Complex;
use std::f32::consts::PI;

/// 数字下变频器（DDC）
/// 
/// 将输入信号从中心频率下变频到基带，并降采样
pub struct Ddc {
    /// 输入采样率
    in_rate: f32,
    /// 输出采样率
    out_rate: f32,
    /// 频率偏移（Hz）
    freq_offset: f32,
    /// 抽取因子
    decimation: usize,
    /// 本振相位
    phase: f32,
    /// 相位增量
    phase_inc: f32,
    /// 抽取计数器
    decim_counter: usize,
    /// 抽取滤波器状态（简单平均）
    accum_re: f32,
    accum_im: f32,
    accum_count: usize,
}

impl Ddc {
    /// 创建新的 DDC
    /// 
    /// - in_rate: 输入采样率（Hz）
    /// - out_rate: 输出采样率（Hz）
    /// - freq_offset: 频率偏移（Hz，正数表示向上偏移）
    pub fn new(in_rate: f32, out_rate: f32, freq_offset: f32) -> Self {
        let decimation = (in_rate / out_rate).round() as usize;
        let phase_inc = -2.0 * PI * freq_offset / in_rate;
        
        log::info!("DDC 初始化: 输入={}Hz 输出={}Hz 偏移={}Hz 抽取={}", 
            in_rate, out_rate, freq_offset, decimation);
        
        Self {
            in_rate,
            out_rate,
            freq_offset,
            decimation,
            phase: 0.0,
            phase_inc,
            decim_counter: 0,
            accum_re: 0.0,
            accum_im: 0.0,
            accum_count: 0,
        }
    }
    
    /// 处理单个样本
    pub fn process_sample(&mut self, input: Complex) -> Option<Complex> {
        // 复数混频（下变频）
        let cos_phase = self.phase.cos();
        let sin_phase = self.phase.sin();
        
        let mixed_re = input.re * cos_phase - input.im * sin_phase;
        let mixed_im = input.re * sin_phase + input.im * cos_phase;
        
        // 更新相位
        self.phase += self.phase_inc;
        while self.phase > PI {
            self.phase -= 2.0 * PI;
        }
        while self.phase < -PI {
            self.phase += 2.0 * PI;
        }
        
        // 累加用于抽取
        self.accum_re += mixed_re;
        self.accum_im += mixed_im;
        self.accum_count += 1;
        self.decim_counter += 1;
        
        // 抽取输出
        if self.decim_counter >= self.decimation {
            let out_re = self.accum_re / self.accum_count as f32;
            let out_im = self.accum_im / self.accum_count as f32;
            
            // 重置累加器
            self.decim_counter = 0;
            self.accum_re = 0.0;
            self.accum_im = 0.0;
            self.accum_count = 0;
            
            Some(Complex::new(out_re, out_im))
        } else {
            None
        }
    }
    
    /// 批量处理
    pub fn process(&mut self, input: &[Complex]) -> Vec<Complex> {
        let mut output = Vec::with_capacity(input.len() / self.decimation + 1);
        
        for &sample in input {
            if let Some(out) = self.process_sample(sample) {
                output.push(out);
            }
        }
        
        output
    }
}

/// FM 解调器（正交解调）
/// 
/// 参考 SDR++ 的 Quadrature 实现：使用 atan2 计算相位差
pub struct FmDemod {
    /// 上一个相位
    prev_phase: f32,
    /// 频偏增益（用于调整输出幅度）
    gain: f32,
}

impl FmDemod {
    /// 创建新的 FM 解调器
    /// 
    /// - sample_rate: 采样率（Hz）
    /// - deviation: 频偏（Hz）
    pub fn new(sample_rate: f32, deviation: f32) -> Self {
        // 增益 = sample_rate / (2 * PI * deviation)
        let gain = sample_rate / (2.0 * PI * deviation);
        
        Self {
            prev_phase: 0.0,
            gain,
        }
    }
    
    /// 处理单个样本
    pub fn process_sample(&mut self, input: Complex) -> f32 {
        // 计算当前相位
        let current_phase = input.phase();
        
        // 计算相位差
        let mut phase_diff = current_phase - self.prev_phase;
        
        // 归一化到 [-PI, PI]
        while phase_diff > PI {
            phase_diff -= 2.0 * PI;
        }
        while phase_diff < -PI {
            phase_diff += 2.0 * PI;
        }
        
        // 保存当前相位
        self.prev_phase = current_phase;
        
        // 应用增益
        phase_diff * self.gain
    }
    
    /// 批量处理
    pub fn process(&mut self, input: &[Complex]) -> Vec<f32> {
        input.iter().map(|&s| self.process_sample(s)).collect()
    }
    
    /// 重置状态
    pub fn reset(&mut self) {
        self.prev_phase = 0.0;
    }
}

/// 完整 DSP 管线
pub struct DspPipeline {
    /// 输入采样率
    in_rate: f32,
    /// 输出采样率
    out_rate: f32,
    /// 主 DDC（用于解调）
    main_ddc: Ddc,
    /// CTCSS DDC（160.55Hz 偏移）
    ctcss_ddc: Ddc,
    /// FM 解调器
    fm_demod: FmDemod,
    /// CTCSS FM 解调器
    ctcss_fm: FmDemod,
    /// 信号强度（RMS）
    pub signal_rms: f32,
}

impl DspPipeline {
    /// 创建新的 DSP 管线
    pub fn new(in_rate: f32, out_rate: f32) -> Self {
        // 主 DDC：无偏移，降采样到 out_rate
        let main_ddc = Ddc::new(in_rate, out_rate, 0.0);
        
        // CTCSS DDC：160.55Hz 偏移，500Hz 输出（参考 SDR++）
        let ctcss_ddc = Ddc::new(in_rate, 500.0, 160.55);
        
        // FM 解调器
        let fm_demod = FmDemod::new(out_rate, 5000.0); // NBFM 5kHz 频偏
        let ctcss_fm = FmDemod::new(500.0, 100.0); // CTCSS 100Hz 频偏
        
        Self {
            in_rate,
            out_rate,
            main_ddc,
            ctcss_ddc,
            fm_demod,
            ctcss_fm,
            signal_rms: 0.0,
        }
    }
    
    /// 处理 IQ 数据
    /// 
    /// 返回：(音频样本, CTCSS 解调样本)
    pub fn process(&mut self, iq: &[Complex]) -> (Vec<f32>, Vec<f32>) {
        // 计算信号强度
        let power_sum: f32 = iq.iter().map(|s| s.re * s.re + s.im * s.im).sum();
        self.signal_rms = (power_sum / iq.len() as f32).sqrt();
        
        // 主路径：DDC -> FM 解调
        let main_iq = self.main_ddc.process(iq);
        let audio = self.fm_demod.process(&main_iq);
        
        // CTCSS 路径：DDC(160.55Hz 偏移) -> FM 解调
        let ctcss_iq = self.ctcss_ddc.process(iq);
        let ctcss_audio = self.ctcss_fm.process(&ctcss_iq);
        
        (audio, ctcss_audio)
    }
}
