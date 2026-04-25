// ─── Fbank 常量（与 funasr WavFrontend 一致）───
pub const SAMPLE_RATE: usize = 16000;
pub const N_MELS: usize = 80;
pub const FRAME_LEN: usize = 400;   // 25ms @ 16kHz
pub const FRAME_SHIFT: usize = 160; // 10ms @ 16kHz
pub const LFR_M: usize = 7;         // 拼帧窗口
pub const LFR_N: usize = 6;         // 拼帧步长

/// 完整前端：Fbank(80) → LFR(7,6) → CMVN → (flat_vec [1 * T_lfr * 560], T_lfr)
pub fn extract_features(cmvn_means: &[f32], cmvn_scales: &[f32], samples: &[f32]) -> anyhow::Result<(Vec<f32>, usize)> {
    let n_frames = (samples.len() - FRAME_LEN) / FRAME_SHIFT + 1;
    let dim = N_MELS * LFR_M;

    // ── Hamming 窗 ──
    let window: Vec<f32> = (0..FRAME_LEN)
        .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_LEN - 1) as f32).cos())
        .collect();

    // ── Mel 滤波器（只建一次）──
    let mel_filters = build_mel_filters();

    // ── Fbank：每帧 FFT → 功率谱 → Mel → log ──
    let mut raw_feats = vec![0.0f32; n_frames * N_MELS];
    for i in 0..n_frames {
        let start = i * FRAME_SHIFT;
        let end   = (start + FRAME_LEN).min(samples.len());
        let mut frame = vec![0.0f32; FRAME_LEN];
        for k in 0..(end - start) {
            frame[k] = samples[start + k] * window[k];
        }
        let power = compute_power_spectrum(&frame);
        for m in 0..N_MELS {
            let energy: f32 = mel_filters[m].iter().zip(power.iter()).map(|(w, p)| w * p).sum();
            raw_feats[i * N_MELS + m] = energy.max(1e-10_f32).ln();
        }
    }

    // ── LFR 拼帧：每 LFR_N 帧取一次，拼 LFR_M 帧，边界用最近帧填充 ──
    let n_lfr = (n_frames.saturating_sub(1)) / LFR_N + 1;
    let mut lfr_feats = vec![0.0f32; n_lfr * dim];
    let half = LFR_M / 2;
    for i in 0..n_lfr {
        let center = i * LFR_N;
        for m in 0..LFR_M {
            let frame_idx = if m < half {
                center.saturating_sub(half - m)
            } else {
                (center + m - half).min(n_frames - 1)
            };
            for j in 0..N_MELS {
                lfr_feats[i * dim + m * N_MELS + j] = raw_feats[frame_idx * N_MELS + j];
            }
        }
    }

    // ── CMVN: y = (x + mean) * scale ──
    if cmvn_means.len() == dim {
        for t in 0..n_lfr {
            for d in 0..dim {
                let v = lfr_feats[t * dim + d];
                lfr_feats[t * dim + d] = (v + cmvn_means[d]) * cmvn_scales[d];
            }
        }
    }

    Ok((lfr_feats, n_lfr))
}

/// 构建 Mel 滤波器组 [N_MELS][n_fft/2+1]
pub fn build_mel_filters() -> Vec<Vec<f32>> {
    let n_fft  = FRAME_LEN;
    let n_bins = n_fft / 2 + 1;
    let hz_to_mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
    let mel_to_hz = |m: f64| 700.0 * (10.0f64.powf(m / 2595.0) - 1.0);

    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(SAMPLE_RATE as f64 / 2.0);
    let mel_pts: Vec<f64> = (0..=N_MELS + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (N_MELS + 1) as f64)
        .collect();
    let bin_pts: Vec<usize> = mel_pts.iter()
        .map(|&m| ((n_fft + 1) as f64 * mel_to_hz(m) / SAMPLE_RATE as f64).floor() as usize)
        .collect();

    let mut filters = vec![vec![0.0f32; n_bins]; N_MELS];
    for m in 0..N_MELS {
        let (l, c, r) = (bin_pts[m], bin_pts[m + 1], bin_pts[m + 2]);
        for k in l..c {
            if k < n_bins && c > l {
                filters[m][k] = (k - l) as f32 / (c - l) as f32;
            }
        }
        for k in c..r {
            if k < n_bins && r > c {
                filters[m][k] = (r - k) as f32 / (r - c) as f32;
            }
        }
    }
    filters
}

/// 计算帧功率谱（Cooley-Tukey FFT，radix-2）
pub fn compute_power_spectrum(frame: &[f32]) -> Vec<f32> {
    let padded = FRAME_LEN.next_power_of_two();
    let mut re = vec![0.0f64; padded];
    let mut im = vec![0.0f64; padded];
    for (i, &s) in frame.iter().enumerate().take(FRAME_LEN) {
        re[i] = s as f64;
    }
    fft_inplace(&mut re, &mut im, padded);
    let n_bins = FRAME_LEN / 2 + 1;
    (0..n_bins).map(|k| (re[k] * re[k] + im[k] * im[k]) as f32).collect()
}

pub fn fft_inplace(re: &mut [f64], im: &mut [f64], n: usize) {
    // bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j ^= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    // Cooley-Tukey butterfly
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut k = 0;
        while k < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for l in 0..len / 2 {
                let (tr, ti) = (
                    cr * re[k + l + len/2] - ci * im[k + l + len/2],
                    cr * im[k + l + len/2] + ci * re[k + l + len/2],
                );
                re[k + l + len/2] = re[k + l] - tr;
                im[k + l + len/2] = im[k + l] - ti;
                re[k + l] += tr;
                im[k + l] += ti;
                let new_cr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = new_cr;
            }
            k += len;
        }
        len <<= 1;
    }
}
