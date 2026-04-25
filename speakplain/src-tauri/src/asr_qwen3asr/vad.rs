use super::{Qwen3ASREngine, SAMPLE_RATE, SILENCE_THRESHOLD_DB, SILENCE_HOP_SEC};

impl Qwen3ASREngine {
    pub(crate) fn find_silence_split_points(&self, wav: &[f32], target_sec: usize) -> Vec<usize> {
        let min_sec = target_sec / 2;
        let max_sec = (target_sec as f32 * 1.5) as usize;

        let total_samples = wav.len();
        if total_samples <= max_sec * SAMPLE_RATE {
            return Vec::new();
        }

        let hop_samples = (SILENCE_HOP_SEC * SAMPLE_RATE as f32) as usize;
        let frame_length = hop_samples * 2;

        let num_frames = (wav.len() - frame_length) / hop_samples + 1;
        let mut rms_db = Vec::with_capacity(num_frames);
        let max_rms = wav.iter().map(|&v| v.abs()).fold(0.0f32, f32::max);

        for i in 0..num_frames {
            let start = i * hop_samples;
            let mut sum_sq = 0.0f32;
            for j in 0..frame_length {
                if start + j < wav.len() {
                    sum_sq += wav[start + j] * wav[start + j];
                }
            }
            let rms = (sum_sq / frame_length as f32).sqrt();
            let db = if max_rms > 0.0 {
                20.0 * (rms / max_rms).log10()
            } else {
                f32::NEG_INFINITY
            };
            rms_db.push(db);
        }

        let is_silent: Vec<bool> = rms_db.iter().map(|&db| db < SILENCE_THRESHOLD_DB).collect();

        let mut split_points = Vec::new();
        let mut cursor = 0;

        while cursor + max_sec * SAMPLE_RATE < total_samples {
            let search_start_sec = (cursor as f32 / SAMPLE_RATE as f32 + min_sec as f32).max(0.0);
            let search_end_sec = cursor as f32 / SAMPLE_RATE as f32 + max_sec as f32;
            let target_abs_sec = cursor as f32 / SAMPLE_RATE as f32 + target_sec as f32;

            let frame_start = (search_start_sec / SILENCE_HOP_SEC) as usize;
            let frame_end = ((search_end_sec / SILENCE_HOP_SEC) as usize).min(is_silent.len());
            let frame_target = (target_abs_sec / SILENCE_HOP_SEC) as usize;

            let silent_frames: Vec<usize> = (frame_start..frame_end)
                .filter(|&i| is_silent[i])
                .collect();

            let split_sample = if !silent_frames.is_empty() {
                let best_idx = silent_frames.iter()
                    .enumerate()
                    .min_by_key(|(_, &frame)| (frame as i32 - frame_target as i32).abs())
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                let split_frame = silent_frames[best_idx];
                (split_frame * hop_samples) as usize
            } else {
                (target_abs_sec * SAMPLE_RATE as f32) as usize
            };

            let split_sample = split_sample.min(total_samples);
            split_points.push(split_sample);
            cursor = split_sample;
        }

        split_points
    }
}
