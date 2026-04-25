//! DSP管线（参考 SDRPlusPlus 架构）
//!
//! 架构参考 SDRPlusPlus：
//! - IQ Frontend：DC 去除、IQ 平衡
//! - 正交解调（Quadrature Demodulation）使用 atan2 计算相位差
//! - 两级抽取策略：
//!   级1：粗抽取（步长 = input_rate / 240000，到约240kHz）
//!   级2：精抽取（到 output_rate，通常48kHz）
//! - 多相 FIR 滤波器实现高效抽取

use crate::sdr::types::DemodMode;

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
// DSP管线
// ──────────────────────────────────────────────────────────────────────────────

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
    /// AGC 增益状态（自适应，避免削波）
    agc_gain: f32,
    /// 去加重 alpha 系数（预计算，基于 output_rate）
    deemph_alpha: f32,
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
        let fir1_coeffs = design_fir_lowpass_sdrpp(fir1_cutoff, fir1_trans, input_rate as f64);

        // 级2 FIR：音频低通，截止 = min(带宽/2, output_rate*0.45)
        // 在 stage1_rate 频域设计，过渡带放宽到40%（tap ≈ 3.8*240k/(21600*0.4) ≈ 106）
        let half_bandwidth = (bandwidth as f32 / 2.0).min(stage1_rate as f32 / 2.0);
        let audio_bandwidth = half_bandwidth.min(output_rate as f32 * 0.45);
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
            agc_gain: 1.0,
            deemph_alpha,
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

// ──────────────────────────────────────────────────────────────────────────────
// FIR 设计工具函数
// ──────────────────────────────────────────────────────────────────────────────

/// 设计 SDR++ 风格 FIR 低通滤波器（Nuttall窗 + sinc）
/// - cutoff: 截止频率（Hz）
/// - trans_width: 过渡带宽度（Hz）
/// - sample_rate: 采样率（Hz）
pub fn design_fir_lowpass_sdrpp(cutoff: f32, trans_width: f32, sample_rate: f64) -> Vec<f32> {
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


