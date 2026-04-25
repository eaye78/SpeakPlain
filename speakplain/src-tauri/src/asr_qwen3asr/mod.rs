// Qwen3-ASR-0.6B 语音识别引擎模块 (1:1 复刻 Python onnx_inference.py)
use ort::session::Session;
use std::sync::Mutex;
use log::info;

pub(crate) mod model;
pub(crate) mod mel;
pub(crate) mod vad;
pub(crate) mod encode;
pub(crate) mod decode;

// ─── 常量定义 (与 Python 完全一致) ───
pub(crate) const SAMPLE_RATE: usize = 16000;
pub(crate) const N_FFT: usize = 400;
pub(crate) const HOP_LENGTH: usize = 160;
pub(crate) const N_MELS: usize = 128;
pub(crate) const CHUNK_SIZE: usize = 100;  // n_window * 2

pub(crate) const VOCAB_SIZE: usize = 151936;
pub(crate) const HIDDEN_SIZE: usize = 1024;
pub(crate) const ENCODER_HIDDEN_SIZE: usize = 896;
pub(crate) const NUM_LAYERS: usize = 28;
pub(crate) const NUM_HEADS: usize = 8;
pub(crate) const HEAD_DIM: usize = 128;

// 特殊 token IDs (与 Python 完全一致)
pub(crate) const AUDIO_START_ID: usize = 151669;
pub(crate) const AUDIO_END_ID: usize = 151670;
pub(crate) const AUDIO_PAD_ID: usize = 151676;
pub(crate) const IM_START_ID: usize = 151644;
pub(crate) const IM_END_ID: usize = 151645;
pub(crate) const ENDOFTEXT_ID: usize = 151643;
pub(crate) const NEWLINE_ID: usize = 198;

// VAD 常量 (与 Python 一致)
pub(crate) const SILENCE_THRESHOLD_DB: f32 = -40.0;
pub(crate) const SILENCE_HOP_SEC: f32 = 0.1;

/// Qwen3-ASR 完整 ONNX 引擎
pub struct Qwen3ASREngine {
    pub(crate) encoder_conv: Mutex<Session>,
    pub(crate) encoder_transformer: Mutex<Session>,
    pub(crate) decoder_init: Mutex<Session>,
    pub(crate) decoder_step: Mutex<Session>,
    pub(crate) embed_tokens: Vec<f32>,
    pub(crate) vocab: std::collections::HashMap<String, usize>,
    pub(crate) id_to_token: std::collections::HashMap<usize, String>,
    pub(crate) mel_filters: Vec<Vec<f32>>,
    pub(crate) hw_info: String,
}

impl Qwen3ASREngine {
    pub fn new() -> anyhow::Result<Self> {
        let model_dir = Self::get_model_dir()?;
        let onnx_dir = model_dir.join("onnx_models");

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

        let encoder_conv = Self::build_session(&onnx_dir.join("encoder_conv.onnx"))?;
        info!("✓ encoder_conv 加载成功");

        let encoder_transformer = Self::build_session(&onnx_dir.join("encoder_transformer.onnx"))?;
        info!("✓ encoder_transformer 加载成功");

        let decoder_init = Self::build_session(&onnx_dir.join("decoder_init.int8.onnx"))?;
        info!("✓ decoder_init 加载成功");

        let decoder_step = Self::build_session(&onnx_dir.join("decoder_step.int8.onnx"))?;
        info!("✓ decoder_step 加载成功");

        let embed_tokens = Self::load_embed_tokens(&onnx_dir.join("embed_tokens.bin"))?;
        info!("✓ embed_tokens 加载成功: {} 个元素", embed_tokens.len());

        let (vocab, id_to_token) = Self::load_tokenizer_vocab(&model_dir.join("tokenizer.json"))?;
        info!("✓ tokenizer 加载成功: {} 个词汇", vocab.len());

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
}

impl Qwen3ASREngine {
    pub fn recognize(&self, samples: &[f32]) -> anyhow::Result<String> {
        const MIN_SAMPLES: usize = CHUNK_SIZE * HOP_LENGTH;
        if samples.len() < MIN_SAMPLES {
            info!("Qwen3-ASR 样本数不足: {} < {}, 跳过识别", samples.len(), MIN_SAMPLES);
            return Ok(String::new());
        }

        info!("Qwen3-ASR 开始识别，样本数: {}", samples.len());

        let split_points = self.find_silence_split_points(samples, 30);

        if split_points.is_empty() {
            let (text, _lang) = self.transcribe_chunk(samples, None)?;
            info!("Qwen3-ASR 识别结果: '{}'", text);
            return Ok(text.trim().to_string());
        }

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
}

impl Qwen3ASREngine {
    pub fn hardware_info(&self) -> &str { &self.hw_info }
    #[allow(dead_code)]
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
