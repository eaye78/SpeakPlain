// Qwen3-ASR-0.6B 语音识别引擎模块 (1:1 复刻 Python onnx_inference.py)
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use std::path::PathBuf;
use std::sync::Mutex;
use log::info;
use realfft::RealFftPlanner;
use mel_filter::{mel, NormalizationFactor};

// ─── 常量定义 (与 Python 完全一致) ───
const SAMPLE_RATE: usize = 16000;
const N_FFT: usize = 400;
const HOP_LENGTH: usize = 160;
const N_MELS: usize = 128;
const CHUNK_SIZE: usize = 100;  // n_window * 2

const VOCAB_SIZE: usize = 151936;
const HIDDEN_SIZE: usize = 1024;
const ENCODER_HIDDEN_SIZE: usize = 896;
const NUM_LAYERS: usize = 28;
const NUM_HEADS: usize = 8;
const HEAD_DIM: usize = 128;

// 特殊 token IDs (与 Python 完全一致)
const AUDIO_START_ID: usize = 151669;
const AUDIO_END_ID: usize = 151670;
const AUDIO_PAD_ID: usize = 151676;
const IM_START_ID: usize = 151644;
const IM_END_ID: usize = 151645;
const ENDOFTEXT_ID: usize = 151643;
const NEWLINE_ID: usize = 198;

// VAD 常量 (与 Python 一致)
const SILENCE_THRESHOLD_DB: f32 = -40.0;
const SILENCE_HOP_SEC: f32 = 0.1;

/// Qwen3-ASR 完整 ONNX 引擎
pub struct Qwen3ASREngine {
    encoder_conv: Mutex<Session>,
    encoder_transformer: Mutex<Session>,
    decoder_init: Mutex<Session>,
    decoder_step: Mutex<Session>,
    embed_tokens: Vec<f32>,
    vocab: std::collections::HashMap<String, usize>,
    id_to_token: std::collections::HashMap<usize, String>,
    mel_filters: Vec<Vec<f32>>, // [N_MELS, N_FFT//2+1] - 由 mel_filter crate 生成
    hw_info: String,
}

impl Qwen3ASREngine {
    pub fn new() -> anyhow::Result<Self> {
        let model_dir = Self::get_model_dir()?;
        let onnx_dir = model_dir.join("onnx_models");

        // 检查所有必需的模型文件
        let required_files = [
            "encoder_conv.onnx",
            "encoder_transformer.onnx",
            "decoder_init.int8.onnx",
            "decoder_step.int8.onnx",
            "embed_tokens.bin",
        ];

        for file in &required_files {
            let path = onnx_dir.join(file);
            if !path.exists() {
                return Err(anyhow::anyhow!("Qwen3-ASR 文件不存在: {:?}", path));
            }
        }

        info!("加载 Qwen3-ASR 完整 ONNX 模型套件 (CPU 模式)...");

        // 加载所有 ONNX 会话
        let encoder_conv = Self::build_session(&onnx_dir.join("encoder_conv.onnx"))?;
        info!("✓ encoder_conv 加载成功");

        let encoder_transformer = Self::build_session(&onnx_dir.join("encoder_transformer.onnx"))?;
        info!("✓ encoder_transformer 加载成功");

        let decoder_init = Self::build_session(&onnx_dir.join("decoder_init.int8.onnx"))?;
        info!("✓ decoder_init 加载成功");

        let decoder_step = Self::build_session(&onnx_dir.join("decoder_step.int8.onnx"))?;
        info!("✓ decoder_step 加载成功");

        // 加载 token embeddings
        let embed_tokens = Self::load_embed_tokens(&onnx_dir.join("embed_tokens.bin"))?;
        info!("✓ embed_tokens 加载成功: {} 个元素", embed_tokens.len());

        // 加载 tokenizer (解析 tokenizer.json 词汇表)
        let (vocab, id_to_token) = Self::load_tokenizer_vocab(&model_dir.join("tokenizer.json"))?;
        info!("✓ tokenizer 加载成功: {} 个词汇", vocab.len());

        // 构建 mel 滤波器 (使用 mel_filter crate，与 librosa.filters.mel 完全一致)
        let mel_filters = Self::build_mel_filters();
        info!("✓ mel 滤波器构建成功 (mel_filter crate, {}x{})", mel_filters.len(), mel_filters[0].len());

        let hw_info = "Qwen3-ASR-0.6B-Full · CPU".to_string();

        info!("Qwen3-ASR 引擎初始化完成: {}", hw_info);
        Ok(Self {
            encoder_conv: Mutex::new(encoder_conv),
            encoder_transformer: Mutex::new(encoder_transformer),
            decoder_init: Mutex::new(decoder_init),
            decoder_step: Mutex::new(decoder_step),
            embed_tokens,
            vocab,
            id_to_token,
            mel_filters,
            hw_info,
        })
    }

    fn build_session(model_path: &PathBuf) -> anyhow::Result<Session> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("创建 session builder 失败: {}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("设置线程数失败: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("设置优化级别失败: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("加载模型失败: {}", e))?;

        Ok(session)
    }

    fn load_embed_tokens(path: &PathBuf) -> anyhow::Result<Vec<f32>> {
        let bytes = std::fs::read(path)?;
        let mut tokens = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            tokens.push(val);
        }
        Ok(tokens)
    }

    fn load_tokenizer_vocab(path: &PathBuf) -> anyhow::Result<(std::collections::HashMap<String, usize>, std::collections::HashMap<usize, String>)> {
        let content = std::fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        let mut vocab = std::collections::HashMap::new();
        let mut id_to_token = std::collections::HashMap::new();
        if let Some(model) = json.get("model") {
            if let Some(vocab_obj) = model.get("vocab") {
                if let Some(vocab_map) = vocab_obj.as_object() {
                    for (token, id_val) in vocab_map {
                        if let Some(id) = id_val.as_u64() {
                            vocab.insert(token.clone(), id as usize);
                            id_to_token.insert(id as usize, token.clone());
                        }
                    }
                }
            }
        }
        Ok((vocab, id_to_token))
    }

    fn get_model_dir() -> anyhow::Result<PathBuf> {
        crate::asr::find_models_dir()
            .map(|models| models.join("Qwen3-ASR-0.6B-ONNX-CPU"))
            .filter(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("未找到 Qwen3-ASR 模型目录"))
    }

    // ─── Mel 频谱图 (1:1 复刻 Python compute_mel_spectrogram) ───

    fn build_mel_filters() -> Vec<Vec<f32>> {
        // 使用 mel_filter crate 完全对标 librosa.filters.mel
        // sr=16000, n_fft=400, n_mels=128, fmin=0, fmax=8000, htk=false, norm="slaney"
        let filters: Vec<Vec<f64>> = mel::<f64>(
            SAMPLE_RATE,
            N_FFT,
            Some(N_MELS),
            Some(0.0f64),
            Some((SAMPLE_RATE / 2) as f64),
            false,               // htk=false，使用 Slaney 公式（与 librosa 一致）
            NormalizationFactor::One,  // area normalization = librosa norm="slaney"
        );
        // 转换 f64 → f32
        filters.into_iter()
            .map(|row| row.into_iter().map(|v| v as f32).collect())
            .collect()
    }

    fn compute_mel_spectrogram(&self, wav: &[f32]) -> Vec<Vec<f32>> {
        let n_frames = wav.len().saturating_add(HOP_LENGTH - 1) / HOP_LENGTH;
        let n_bins = N_FFT / 2 + 1;

        // 使用 realfft 进行 STFT (与 librosa.stft 一致)
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        let mut spectrum = fft.make_output_vec();

        let mut magnitudes: Vec<Vec<f32>> = vec![vec![0.0; n_frames]; n_bins];

        for frame_idx in 0..n_frames {
            let start = frame_idx * HOP_LENGTH;
            let mut frame = vec![0.0f32; N_FFT];

            // 应用 Hann 窗口
            for i in 0..N_FFT {
                let sample_idx = start + i;
                if sample_idx < wav.len() {
                    let window = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos();
                    frame[i] = wav[sample_idx] * window;
                }
            }

            // FFT
            fft.process(&mut frame, &mut spectrum).unwrap();

            // 计算幅度平方 (与 np.abs(stft) ** 2 一致)
            for k in 0..n_bins {
                let real = spectrum[k].re;
                let imag = spectrum[k].im;
                magnitudes[k][frame_idx] = real * real + imag * imag;
            }
        }

        // 应用 mel 滤波器: mel_spec = mel_filters @ magnitudes
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

        // Log scale (Whisper-style, 与 Python 一致)
        // log_spec = np.log10(np.maximum(mel_spec, 1e-10))
        // log_spec = np.maximum(log_spec, log_spec.max() - 8.0)
        // log_spec = (log_spec + 4.0) / 4.0
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

    fn get_feat_extract_output_lengths(input_lengths: usize) -> usize {
        let mut len = input_lengths;
        for _ in 0..3 {
            len = (len - 1) / 2 + 1;
        }
        len
    }

    // ─── 编码器 (1:1 复刻 Python _encode_audio) ───

    fn encode_audio(&self, mel: &[Vec<f32>], mel_len: usize) -> anyhow::Result<Vec<Vec<f32>>> {
        let chunk_num = (mel_len + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let mut chunk_lengths = Vec::new();

        for i in 0..chunk_num {
            let start = i * CHUNK_SIZE;
            let end = (start + CHUNK_SIZE).min(mel_len);
            chunk_lengths.push(end - start);
        }

        let max_chunk_len = *chunk_lengths.iter().max().unwrap();

        // 构建 padded chunks: [chunk_num, 1, N_MELS, max_chunk_len]
        let mut padded = vec![0.0f32; chunk_num * 1 * N_MELS * max_chunk_len];
        for i in 0..chunk_num {
            let start = i * CHUNK_SIZE;
            let cl = chunk_lengths[i];
            for m in 0..N_MELS {
                for j in 0..cl {
                    let idx = ((i * N_MELS + m) * max_chunk_len + j) as usize;
                    padded[idx] = mel[m][start + j];
                }
            }
        }

        let padded_tensor = Tensor::<f32>::from_array(
            ([chunk_num, 1, N_MELS, max_chunk_len], padded.into_boxed_slice())
        )?;

        // Conv block
        let (conv_shape, conv_data_vec): (Vec<i64>, Vec<f32>) = {
            let mut session = self.encoder_conv.lock().unwrap();
            let conv_out = session.run(ort::inputs! {
                "padded_mel_chunks" => padded_tensor,
            }).map_err(|e| anyhow::anyhow!("encoder_conv 推理失败: {}", e))?;

            let conv_out_val = conv_out.values().next()
                .ok_or_else(|| anyhow::anyhow!("encoder_conv 输出为空"))?;
            let (shape, data) = conv_out_val.try_extract_tensor::<f32>()?;
            (shape.to_vec(), data.to_vec())
        };

        // Pack features (remove padding)
        let max_seq_len = conv_shape[1] as usize;
        let conv_hidden_size = ENCODER_HIDDEN_SIZE;
        let mut features: Vec<f32> = Vec::new();

        for i in 0..chunk_num {
            let lens_after_cnn = Self::get_feat_extract_output_lengths(chunk_lengths[i]);
            let chunk_offset = i * max_seq_len;
            for j in 0..lens_after_cnn.min(max_seq_len) {
                let idx = chunk_offset + j;
                let data_offset = idx * conv_hidden_size;
                if data_offset + conv_hidden_size <= conv_data_vec.len() {
                    for k in 0..conv_hidden_size {
                        features.push(conv_data_vec[data_offset + k]);
                    }
                }
            }
        }

        let total_tokens = features.len() / conv_hidden_size;

        // Transformer block
        let hidden_states = Tensor::<f32>::from_array(
            ([total_tokens, conv_hidden_size], features.into_boxed_slice())
        )?;

        let attn_mask = Tensor::<f32>::from_array(
            ([1usize, 1, total_tokens, total_tokens], vec![0.0f32; total_tokens * total_tokens].into_boxed_slice())
        )?;

        let out_data_vec: Vec<f32> = {
            let mut session = self.encoder_transformer.lock().unwrap();
            let encoder_out = session.run(ort::inputs! {
                "hidden_states" => hidden_states,
                "attention_mask" => attn_mask,
            }).map_err(|e| anyhow::anyhow!("encoder_transformer 推理失败: {}", e))?;

            let out_val = encoder_out.values().next()
                .ok_or_else(|| anyhow::anyhow!("encoder_transformer 输出为空"))?;
            let (_, data) = out_val.try_extract_tensor::<f32>()?;
            data.to_vec()
        };

        let mut result = Vec::new();
        for i in 0..total_tokens {
            let mut row = Vec::new();
            for j in 0..HIDDEN_SIZE {
                row.push(out_data_vec[i * HIDDEN_SIZE + j]);
            }
            result.push(row);
        }

        Ok(result)
    }

    // ─── Prompt 构建 (1:1 复刻 Python _build_prompt_ids) ───

    fn build_prompt_ids(&self, num_audio_tokens: usize, language: Option<&str>) -> Vec<usize> {
        let mut ids = Vec::new();

        // <|im_start|>system\n<|im_end|>\n
        ids.push(IM_START_ID);
        ids.extend(self.encode_text("system"));
        ids.push(NEWLINE_ID);
        ids.push(IM_END_ID);
        ids.push(NEWLINE_ID);

        // <|im_start|>user\n<|audio_start|><|audio_pad|>...<|audio_end|><|im_end|>\n
        ids.push(IM_START_ID);
        ids.extend(self.encode_text("user"));
        ids.push(NEWLINE_ID);
        ids.push(AUDIO_START_ID);
        ids.extend(vec![AUDIO_PAD_ID; num_audio_tokens]);
        ids.push(AUDIO_END_ID);
        ids.push(IM_END_ID);
        ids.push(NEWLINE_ID);

        // <|im_start|>assistant\n
        ids.push(IM_START_ID);
        ids.extend(self.encode_text("assistant"));
        ids.push(NEWLINE_ID);

        // 语言提示 (可选)
        if let Some(lang) = language {
            let lang_text = format!("language {}<asr_text>", lang);
            ids.extend(self.encode_text(&lang_text));
        }

        ids
    }

    fn encode_text(&self, text: &str) -> Vec<usize> {
        // BPE 最长匹配简单编码
        let mut result = Vec::new();
        let mut remaining = text;
        while !remaining.is_empty() {
            let mut longest_id: Option<usize> = None;
            let mut longest_len = 0usize;
            for (token, &id) in &self.vocab {
                if remaining.starts_with(token.as_str()) && token.len() > longest_len {
                    longest_id = Some(id);
                    longest_len = token.len();
                }
            }
            if let Some(id) = longest_id {
                result.push(id);
                remaining = &remaining[longest_len..];
            } else {
                let c = remaining.chars().next().unwrap();
                remaining = &remaining[c.len_utf8()..];
            }
        }
        result
    }

    // ─── Embedding 融合 (1:1 复刻 Python _embed_and_fuse) ───

    fn embed_and_fuse(&self, token_ids: &[usize], audio_features: &[Vec<f32>]) -> Vec<f32> {
        let seq_len = token_ids.len();
        let mut embeds = vec![0.0f32; seq_len * HIDDEN_SIZE];

        // 基础嵌入: embeds = self.embed_tokens[ids_array]
        for (i, &id) in token_ids.iter().enumerate() {
            for j in 0..HIDDEN_SIZE {
                embeds[i * HIDDEN_SIZE + j] = self.embed_tokens[id * HIDDEN_SIZE + j];
            }
        }

        // 替换 audio_pad 位置: embeds[audio_positions] = audio_features
        let mut audio_pos = 0;
        for (i, &id) in token_ids.iter().enumerate() {
            if id == AUDIO_PAD_ID && audio_pos < audio_features.len() {
                for j in 0..HIDDEN_SIZE {
                    embeds[i * HIDDEN_SIZE + j] = audio_features[audio_pos][j];
                }
                audio_pos += 1;
            }
        }

        embeds
    }

    // ─── VAD 长音频分块 (1:1 复刻 Python find_silence_split_points) ───

    fn find_silence_split_points(&self, wav: &[f32], target_sec: usize) -> Vec<usize> {
        let min_sec = target_sec / 2;
        let max_sec = (target_sec as f32 * 1.5) as usize;

        let total_samples = wav.len();
        if total_samples <= max_sec * SAMPLE_RATE {
            return Vec::new();
        }

        let hop_samples = (SILENCE_HOP_SEC * SAMPLE_RATE as f32) as usize;
        let frame_length = hop_samples * 2;

        // 计算 RMS 能量
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
            // 转换为 dB: 20 * log10(rms / max_rms)
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

    // ─── 单块转录 (1:1 复刻 Python _transcribe_chunk) ───

    fn transcribe_chunk(&self, wav: &[f32], language: Option<&str>) -> anyhow::Result<(String, String)> {
        let mel = self.compute_mel_spectrogram(wav);
        let mel_len = mel[0].len();

        let audio_features = self.encode_audio(&mel, mel_len)?;
        let num_audio_tokens = audio_features.len();

        let token_ids = self.build_prompt_ids(num_audio_tokens, language);
        let input_embeds = self.embed_and_fuse(&token_ids, &audio_features);
        let seq_len = token_ids.len();

        let embeds_tensor = Tensor::<f32>::from_array(
            ([1usize, seq_len, HIDDEN_SIZE], input_embeds.clone().into_boxed_slice())
        )?;

        let position_ids: Vec<i64> = (0..seq_len as i64).collect();
        let pos_tensor = Tensor::<i64>::from_array(
            ([1usize, seq_len], position_ids.into_boxed_slice())
        )?;

        // Decoder init
        let (first_logits, mut present_keys, mut present_values): (Vec<f32>, Vec<f32>, Vec<f32>) = {
            let mut session = self.decoder_init.lock().unwrap();
            let init_out = session.run(ort::inputs! {
                "input_embeds" => embeds_tensor,
                "position_ids" => pos_tensor,
            }).map_err(|e| anyhow::anyhow!("decoder_init 推理失败: {}", e))?;

            let logits_val = init_out.get("logits")
                .ok_or_else(|| anyhow::anyhow!("缺少 logits 输出"))?;
            let (_, logits_data) = logits_val.try_extract_tensor::<f32>()?;

            let keys_val = init_out.get("present_keys")
                .ok_or_else(|| anyhow::anyhow!("缺少 present_keys 输出"))?;
            let (_, keys_data) = keys_val.try_extract_tensor::<f32>()?;

            let values_val = init_out.get("present_values")
                .ok_or_else(|| anyhow::anyhow!("缺少 present_values 输出"))?;
            let (_, values_data) = values_val.try_extract_tensor::<f32>()?;

            (logits_data.to_vec(), keys_data.to_vec(), values_data.to_vec())
        };

        // 自回归解码
        let mut generated: Vec<usize> = Vec::new();
        let mut next_token = self.argmax(&first_logits[(seq_len - 1) * VOCAB_SIZE..seq_len * VOCAB_SIZE]);
        let mut cur_pos = seq_len;

        let max_new_tokens = 512;

        for _ in 0..max_new_tokens {
            if next_token == IM_END_ID || next_token == ENDOFTEXT_ID {
                break;
            }

            generated.push(next_token);

            // 嵌入下一个 token
            let mut token_embed = vec![0.0f32; HIDDEN_SIZE];
            for j in 0..HIDDEN_SIZE {
                token_embed[j] = self.embed_tokens[next_token * HIDDEN_SIZE + j];
            }
            let token_tensor = Tensor::<f32>::from_array(
                ([1usize, 1, HIDDEN_SIZE], token_embed.into_boxed_slice())
            )?;

            let pos_tensor = Tensor::<i64>::from_array(
                ([1usize, 1], vec![cur_pos as i64].into_boxed_slice())
            )?;

            // KV-Cache 张量
            let past_k_tensor = Tensor::<f32>::from_array(
                ([NUM_LAYERS, 1usize, NUM_HEADS, cur_pos, HEAD_DIM], present_keys.clone().into_boxed_slice())
            )?;
            let past_v_tensor = Tensor::<f32>::from_array(
                ([NUM_LAYERS, 1usize, NUM_HEADS, cur_pos, HEAD_DIM], present_values.clone().into_boxed_slice())
            )?;

            // 运行 decoder_step
            let step_logits: Vec<f32>;
            {
                let mut session = self.decoder_step.lock().unwrap();
                let step_out = session.run(ort::inputs! {
                    "input_embeds" => token_tensor,
                    "position_ids" => pos_tensor,
                    "past_keys" => past_k_tensor,
                    "past_values" => past_v_tensor,
                }).map_err(|e| anyhow::anyhow!("decoder_step 推理失败: {}", e))?;

                let logits_val = step_out.get("logits")
                    .ok_or_else(|| anyhow::anyhow!("缺少 logits 输出"))?;
                let (_, logits_data) = logits_val.try_extract_tensor::<f32>()?;
                step_logits = logits_data.to_vec();

                let new_keys_val = step_out.get("present_keys")
                    .ok_or_else(|| anyhow::anyhow!("缺少 present_keys 输出"))?;
                let (_, new_keys_data) = new_keys_val.try_extract_tensor::<f32>()?;
                present_keys = new_keys_data.to_vec();

                let new_values_val = step_out.get("present_values")
                    .ok_or_else(|| anyhow::anyhow!("缺少 present_values 输出"))?;
                let (_, new_values_data) = new_values_val.try_extract_tensor::<f32>()?;
                present_values = new_values_data.to_vec();
            }

            next_token = self.argmax(&step_logits[0..VOCAB_SIZE]);
            cur_pos += 1;
        }

        // 解码
        let raw_text = self.decode_ids(&generated);

        // 解析语言和文本 (与 Python 完全一致)
        let (parsed_lang, parsed_text) = if raw_text.contains("language ") && raw_text.contains("<asr_text>") {
            // 格式: "language <lang><asr_text><text>"
            let parts: Vec<&str> = raw_text.split("<asr_text>").collect();
            let lang_part = parts[0];
            let lang = if lang_part.starts_with("language ") {
                lang_part["language ".len()..].to_string()
            } else {
                String::new()
            };
            let text = if parts.len() > 1 { parts[1].to_string() } else { String::new() };
            (lang, text)
        } else if raw_text.starts_with("language ") {
            // 模型输出了 "language <lang>" 前缀但没有 <asr_text>
            // 尝试解析: "language Chinese实际文本..."
            let after_language = &raw_text["language ".len()..];
            // 常见语言名称后跟空格或直接是文本
            let known_langs = ["Chinese", "English", "Japanese", "Korean", "French", "German", "Spanish"];
            let mut found_lang = String::new();
            let mut text_start = after_language;
            for lang in &known_langs {
                if after_language.starts_with(lang) {
                    found_lang = lang.to_string();
                    let rest = &after_language[lang.len()..];
                    text_start = rest.trim_start_matches(' ');
                    break;
                }
            }
            (found_lang, text_start.to_string())
        } else if language.is_some() {
            (language.unwrap().to_string(), raw_text)
        } else {
            (String::new(), raw_text)
        };

        Ok((parsed_text, parsed_lang))
    }

    fn decode_ids(&self, ids: &[usize]) -> String {
        // 拼接 token 字符串
        let text: String = ids.iter()
            .filter_map(|&id| self.id_to_token.get(&id))
            .cloned()
            .collect::<Vec<_>>()
            .join("");
        // GPT-2/Qwen BPE 字节解码
        // 使用标准 bytes_to_unicode() 的逆映射: unicode_char -> byte
        let unicode_to_byte = Self::build_unicode_to_byte_map();
        let mut bytes: Vec<u8> = Vec::new();
        for c in text.chars() {
            if let Some(&b) = unicode_to_byte.get(&c) {
                bytes.push(b);
            }
            // 不在映射表中的字符（特殊token等）跳过
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// 构建 GPT-2/Qwen BPE 标准 unicode → byte 映射表
    /// 对应 Python: bytes_to_unicode() 的逆映射
    fn build_unicode_to_byte_map() -> std::collections::HashMap<char, u8> {
        // bytes_to_unicode() 将 256 个字节值映射到 unicode 字符
        // 优先映射: 0x21~0x7E, 0xA1~0xAC, 0xAE~0xFF (保持原值)
        // 其余字节 (0x00~0x20, 0x7F~0xA0, 0xAD) 映射到 0x100~0x121
        let mut bs: Vec<u8> = Vec::new();
        let mut cs: Vec<u32> = Vec::new();

        // 第一批: 0x21~0x7E
        for b in 0x21u8..=0x7Eu8 { bs.push(b); cs.push(b as u32); }
        // 第二批: 0xA1~0xAC
        for b in 0xA1u8..=0xACu8 { bs.push(b); cs.push(b as u32); }
        // 第三批: 0xAE~0xFF
        for b in 0xAEu8..=0xFFu8 { bs.push(b); cs.push(b as u32); }

        // 剩余字节: 按顺序映射到 0x100, 0x101, ...
        let mut n: u32 = 0;
        for b in 0u8..=255u8 {
            if !bs.contains(&b) {
                bs.push(b);
                cs.push(0x100 + n);
                n += 1;
            }
        }

        // 构建逆映射: unicode char -> byte
        let mut map = std::collections::HashMap::new();
        for (b, c) in bs.iter().zip(cs.iter()) {
            map.insert(char::from_u32(*c).unwrap(), *b);
        }
        map
    }

    fn argmax(&self, slice: &[f32]) -> usize {
        slice.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    // ─── 推理入口 (1:1 复刻 Python transcribe) ───

    pub fn recognize(&self, samples: &[f32]) -> anyhow::Result<String> {
        // 最小样本数检查
        const MIN_SAMPLES: usize = CHUNK_SIZE * HOP_LENGTH;
        if samples.len() < MIN_SAMPLES {
            info!("Qwen3-ASR 样本数不足: {} < {}, 跳过识别", samples.len(), MIN_SAMPLES);
            return Ok(String::new());
        }

        info!("Qwen3-ASR 开始识别，样本数: {}", samples.len());

        // 长音频自动分块
        let split_points = self.find_silence_split_points(samples, 30);

        if split_points.is_empty() {
            // 短音频 - 单次识别
            let (text, _lang) = self.transcribe_chunk(samples, None)?;
            info!("Qwen3-ASR 识别结果: '{}'", text);
            return Ok(text.trim().to_string());
        }

        // 长音频 - 分块识别
        let boundaries: Vec<usize> = std::iter::once(0)
            .chain(split_points.iter().cloned())
            .chain(std::iter::once(samples.len()))
            .collect();

        let num_chunks = boundaries.len() - 1;
        info!("长音频分块: {} 个子块", num_chunks);

        let mut texts = Vec::new();

        for i in 0..num_chunks {
            let chunk_wav = &samples[boundaries[i]..boundaries[i + 1]];
            let (chunk_text, _lang) = self.transcribe_chunk(chunk_wav, None)?;
            texts.push(chunk_text.trim().to_string());
        }

        let full_text = texts.join(" ");
        info!("Qwen3-ASR 完整识别结果: '{}'", full_text);
        Ok(full_text)
    }

    pub fn hardware_info(&self) -> &str { &self.hw_info }
    pub fn is_using_gpu(&self) -> bool { false }
}

/// 检查 Qwen3-ASR 模型是否可用（仅检查文件）
pub fn is_qwen3_model_available() -> bool {
    let required_files = [
        "onnx_models/encoder_conv.onnx",
        "onnx_models/encoder_transformer.onnx",
        "onnx_models/decoder_init.int8.onnx",
        "onnx_models/decoder_step.int8.onnx",
        "onnx_models/embed_tokens.bin",
        "tokenizer.json",
    ];
    crate::asr::find_models_dir()
        .map(|models| {
            let p = models.join("Qwen3-ASR-0.6B-ONNX-CPU");
            required_files.iter().all(|f| p.join(f).exists())
        })
        .unwrap_or(false)
}

