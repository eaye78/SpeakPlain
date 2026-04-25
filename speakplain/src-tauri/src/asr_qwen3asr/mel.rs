use realfft::RealFftPlanner;
use mel_filter::{mel, NormalizationFactor};

use super::{Qwen3ASREngine, SAMPLE_RATE, N_FFT, HOP_LENGTH, N_MELS};

impl Qwen3ASREngine {
    pub(crate) fn build_mel_filters() -> Vec<Vec<f32>> {
        let filters: Vec<Vec<f64>> = mel::<f64>(
            SAMPLE_RATE,
            N_FFT,
            Some(N_MELS),
            Some(0.0f64),
            Some((SAMPLE_RATE / 2) as f64),
            false,
            NormalizationFactor::One,
        );
        filters.into_iter()
            .map(|row| row.into_iter().map(|v| v as f32).collect())
            .collect()
    }

    pub(crate) fn compute_mel_spectrogram(&self, wav: &[f32]) -> Vec<Vec<f32>> {
        let n_frames = wav.len().saturating_add(HOP_LENGTH - 1) / HOP_LENGTH;
        let n_bins = N_FFT / 2 + 1;

        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        let mut spectrum = fft.make_output_vec();

        let mut magnitudes: Vec<Vec<f32>> = vec![vec![0.0; n_frames]; n_bins];

        for frame_idx in 0..n_frames {
            let start = frame_idx * HOP_LENGTH;
            let mut frame = vec![0.0f32; N_FFT];

            for i in 0..N_FFT {
                let sample_idx = start + i;
                if sample_idx < wav.len() {
                    let window = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos();
                    frame[i] = wav[sample_idx] * window;
                }
            }

            fft.process(&mut frame, &mut spectrum).unwrap();

            for k in 0..n_bins {
                let real = spectrum[k].re;
                let imag = spectrum[k].im;
                magnitudes[k][frame_idx] = real * real + imag * imag;
            }
        }

        let mut mel_spec = vec![vec![0.0f32; n_frames]; N_MELS];
        for m in 0..N_MELS {
            for f in 0..n_frames {
                let sum: f32 = self.mel_filters[m].iter()
                    .zip(magnitudes.iter())
                    .map(|(w, mag_col)| w * mag_col[f])
                    .sum();
                mel_spec[m][f] = sum;
            }
        }

        let mut max_val = f32::NEG_INFINITY;
        for m in 0..N_MELS {
            for f in 0..n_frames {
                let log_val = mel_spec[m][f].max(1e-10).log10();
                mel_spec[m][f] = log_val;
                if log_val > max_val {
                    max_val = log_val;
                }
            }
        }

        for m in 0..N_MELS {
            for f in 0..n_frames {
                let clamped = mel_spec[m][f].max(max_val - 8.0);
                mel_spec[m][f] = (clamped + 4.0) / 4.0;
            }
        }

        mel_spec
    }

    pub(crate) fn get_feat_extract_output_lengths(input_lengths: usize) -> usize {
        let mut len = input_lengths;
        for _ in 0..3 {
            len = (len - 1) / 2 + 1;
        }
        len
    }
}
