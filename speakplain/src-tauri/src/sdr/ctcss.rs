//! CTCSS 检测器（Goertzel 相干检测，处理 FM 解调后的频偏样本）
//!
//! 核心思路（对齐 SDR++）：
//! FM 解调后的频偏样本中，CTCSS 是一个纯正弦波叠加在语音基带上。
//! 语音能量主要分布在 300Hz 以上，CTCSS 在 67~250Hz。
//! 因此：低通滤波（保留 <300Hz）→ 降采样 → Goertzel 检测目标频率 → 判定有无。

use crate::sdr::dsp::design_fir_lowpass_sdrpp;

pub struct CtcssDetector {
    target_freq: f32,
    pub threshold: f32,

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
    #[allow(dead_code)]
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
