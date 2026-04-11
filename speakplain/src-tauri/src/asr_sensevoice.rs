// 语音识别引擎模块
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use std::path::PathBuf;
use std::sync::Mutex;
use log::{info, warn};

// ─── Fbank 常量（与 funasr WavFrontend 一致）───
const SAMPLE_RATE: usize = 16000;
const N_MELS: usize = 80;
const FRAME_LEN: usize = 400;   // 25ms @ 16kHz
const FRAME_SHIFT: usize = 160; // 10ms @ 16kHz
const LFR_M: usize = 7;         // 拼帧窗口
const LFR_N: usize = 6;         // 拼帧步长
const VOCAB_SPECIAL_START: usize = 24884; // <|zh|> 起始，跳过语言/情感标记
const CTC_BLANK: usize = 0;

pub struct SenseVoiceEngine {
    session: Mutex<Session>,
    tokens: Vec<String>,
    cmvn_means: Vec<f32>,   // 560维，存储负均值（直接相加）
    cmvn_scales: Vec<f32>,  // 560维，存储1/std
    use_gpu: bool,
    hw_info: String,
}

impl SenseVoiceEngine {
    pub fn new() -> anyhow::Result<Self> {
        let model_dir   = Self::get_model_dir()?;
        let model_path  = model_dir.join("model.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        let cmvn_path   = model_dir.join("am.mvn");

        if !model_path.exists() {
            return Err(anyhow::anyhow!("模型文件不存在: {:?}", model_path));
        }
        info!("加载ONNX模型: {:?}", model_path);

        let (session, use_gpu) = match Self::build_session(&model_path, true) {
            Ok(s) => { info!("使用DirectML GPU加速"); (s, true) }
            Err(e) => {
                warn!("DirectML失败: {}，回退CPU", e);
                (Self::build_session(&model_path, false)?, false)
            }
        };

        let tokens = Self::load_tokens(&tokens_path)?;
        let (cmvn_means, cmvn_scales) = Self::load_cmvn(&cmvn_path)?;
        let hw_info = if use_gpu {
            "SenseVoice · DirectML (GPU)".to_string()
        } else {
            "SenseVoice · CPU".to_string()
        };

        info!("ASR引擎初始化完成: {}", hw_info);
        Ok(Self { session: Mutex::new(session), tokens, cmvn_means, cmvn_scales, use_gpu, hw_info })
    }

    fn build_session(model_path: &PathBuf, use_gpu: bool) -> anyhow::Result<Session> {
        info!("开始创建 ONNX Session... (GPU={})", use_gpu);

        let mut builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("创建 session builder 失败: {}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("设置线程数失败: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("设置优化级别失败: {}", e))?;

        // 配置执行提供程序
        if use_gpu {
            // 尝试使用 DirectML GPU 加速
            builder = builder.with_execution_providers([ort::execution_providers::DirectMLExecutionProvider::default().build()])
                .map_err(|e| anyhow::anyhow!("配置 DirectML 失败: {}", e))?;
            info!("已配置 DirectML GPU 执行提供程序");
        }

        let session = builder.commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("加载模型失败: {}", e))?;

        info!("ONNX Session 创建完成");
        Ok(session)
    }

    fn load_tokens(tokens_path: &PathBuf) -> anyhow::Result<Vec<String>> {
        if !tokens_path.exists() {
            return Ok((0..5000).map(|i| format!("<token{}>", i)).collect());
        }
        let tokens = std::fs::read_to_string(tokens_path)?
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(tokens)
    }

    /// 加载 Kaldi 格式 CMVN 文件（am.mvn）
    /// 应用公式: y = (x + mean) * scale  （mean 本身存的是负均值）
    fn load_cmvn(cmvn_path: &PathBuf) -> anyhow::Result<(Vec<f32>, Vec<f32>)> {
        if !cmvn_path.exists() {
            warn!("am.mvn 不存在，跳过 CMVN");
            return Ok((vec![0.0f32; N_MELS * LFR_M], vec![1.0f32; N_MELS * LFR_M]));
        }
        let content = std::fs::read_to_string(cmvn_path)?;
        let mut blocks: Vec<Vec<f32>> = Vec::new();
        let mut in_block = false;
        let mut current: Vec<f32> = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.contains('[') { in_block = true; }
            if in_block {
                for tok in line.split_whitespace() {
                    let t = tok.trim_matches(|c| c == '[' || c == ']');
                    if let Ok(v) = t.parse::<f32>() { current.push(v); }
                }
            }
            if line.contains(']') && in_block {
                in_block = false;
                if !current.is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
            }
        }
        if blocks.len() < 3 {
            return Err(anyhow::anyhow!("am.mvn 格式错误，期望至少 3 个数值块"));
        }
        let means  = blocks[1].clone(); // AddShift 均值（负值）
        let scales = blocks[2].clone(); // Rescale 1/std
        info!("CMVN 加载完成，维度={}", means.len());
        Ok((means, scales))
    }

    fn get_model_dir() -> anyhow::Result<PathBuf> {
        crate::asr::find_models_dir()
            .map(|models| models.join("sensevoice"))
            .filter(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("未找到模型目录，请将模型放到 models/sensevoice/ 下"))
    }

    // ─────────────────────────── 推理入口 ───────────────────────────

    pub fn recognize(&self, samples: &[f32]) -> anyhow::Result<String> {
        if samples.len() < FRAME_LEN {
            return Ok(String::new());
        }

        // 1. Fbank + LFR + CMVN => [1, T_lfr, 560]
        let (feats_flat, t_lfr) = self.extract_features(samples)?;
        let feat_len = t_lfr as i32;

        // 2. 构建四个输入 Tensor（不需要 ndarray，直接用 (shape, data) 元组）
        let speech_tensor = Tensor::<f32>::from_array(
            ([1usize, t_lfr, N_MELS * LFR_M], feats_flat.into_boxed_slice())
        )?;
        let len_tensor = Tensor::<i32>::from_array(
            ([1usize], vec![feat_len].into_boxed_slice())
        )?;
        let lang_tensor = Tensor::<i32>::from_array(
            ([1usize], vec![0i32].into_boxed_slice())
        )?;
        let textnorm_tensor = Tensor::<i32>::from_array(
            ([1usize], vec![15i32].into_boxed_slice())
        )?;

        // 3. 运行 ONNX 推理
        let mut session = self.session.lock().unwrap();
        let outputs = session.run(ort::inputs! {
            "speech"         => speech_tensor,
            "speech_lengths" => len_tensor,
            "language"       => lang_tensor,
            "textnorm"       => textnorm_tensor,
        })?;

        // 4. 提取输出 logits [1, T, 25055]
        let logits_val = &outputs["ctc_logits"];
        let (shape, logits_data) = logits_val.try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("提取输出张量失败: {}", e))?;
        let time_steps = shape[1] as usize;
        let vocab_size = shape[2] as usize;

        // 5. CTC 贪心解码
        Ok(self.ctc_greedy_decode(logits_data, time_steps, vocab_size))
    }

    // ─────────────────────────── 特征提取 ───────────────────────────

    /// 完整前端：Fbank(80) → LFR(7,6) → CMVN → (flat_vec [1 * T_lfr * 560], T_lfr)
    fn extract_features(&self, samples: &[f32]) -> anyhow::Result<(Vec<f32>, usize)> {
        let n_frames = (samples.len() - FRAME_LEN) / FRAME_SHIFT + 1;

        // ── Hamming 窗 ──
        let window: Vec<f32> = (0..FRAME_LEN)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_LEN - 1) as f32).cos())
            .collect();

        // ── Mel 滤波器（只建一次）──
        let mel_filters = Self::build_mel_filters();

        // ── Fbank：每帧 FFT → 功率谱 → Mel → log ──
        let mut raw_feats = vec![0.0f32; n_frames * N_MELS];
        for i in 0..n_frames {
            let start = i * FRAME_SHIFT;
            let end   = (start + FRAME_LEN).min(samples.len());
            let mut frame = vec![0.0f32; FRAME_LEN];
            for k in 0..(end - start) {
                frame[k] = samples[start + k] * window[k];
            }
            let power = Self::compute_power_spectrum(&frame);
            for m in 0..N_MELS {
                let energy: f32 = mel_filters[m].iter().zip(power.iter()).map(|(w, p)| w * p).sum();
                raw_feats[i * N_MELS + m] = energy.max(1e-10_f32).ln();
            }
        }

        // ── LFR 拼帧：每 LFR_N 帧取一次，拼 LFR_M 帧，边界用最近帧填充 ──
        let n_lfr = (n_frames.saturating_sub(1)) / LFR_N + 1;
        let dim = N_MELS * LFR_M;
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
        if self.cmvn_means.len() == dim {
            for t in 0..n_lfr {
                for d in 0..dim {
                    let v = lfr_feats[t * dim + d];
                    lfr_feats[t * dim + d] = (v + self.cmvn_means[d]) * self.cmvn_scales[d];
                }
            }
        }

        Ok((lfr_feats, n_lfr))
    }

    /// 构建 Mel 滤波器组 [N_MELS][n_fft/2+1]
    fn build_mel_filters() -> Vec<Vec<f32>> {
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
    fn compute_power_spectrum(frame: &[f32]) -> Vec<f32> {
        let padded = FRAME_LEN.next_power_of_two();
        let mut re = vec![0.0f64; padded];
        let mut im = vec![0.0f64; padded];
        for (i, &s) in frame.iter().enumerate().take(FRAME_LEN) {
            re[i] = s as f64;
        }
        Self::fft_inplace(&mut re, &mut im, padded);
        let n_bins = FRAME_LEN / 2 + 1;
        (0..n_bins).map(|k| (re[k] * re[k] + im[k] * im[k]) as f32).collect()
    }

    fn fft_inplace(re: &mut [f64], im: &mut [f64], n: usize) {
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

    // ─────────────────────────── 解码 ───────────────────────────

    /// CTC 贪心解码：去重复 + 去 blank + 过滤特殊标记
    fn ctc_greedy_decode(&self, logits: &[f32], time_steps: usize, vocab_size: usize) -> String {
        let mut prev_id = usize::MAX;
        let mut ids: Vec<usize> = Vec::new();

        for t in 0..time_steps {
            let row = &logits[t * vocab_size..(t + 1) * vocab_size];
            let best = row.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);
            if best != CTC_BLANK && best != prev_id {
                ids.push(best);
            }
            prev_id = best;
        }

        self.decode_tokens(&ids)
    }

    fn decode_tokens(&self, ids: &[usize]) -> String {
        let mut text = String::new();
        for &id in ids {
            if id >= self.tokens.len() || id >= VOCAB_SPECIAL_START { continue; }
            let tok = &self.tokens[id];
            // 跳过 <unk> <s> </s> 等尖括号标记
            if tok.starts_with('<') && tok.ends_with('>') { continue; }
            text.push_str(tok);
        }
        // ▁ (U+2581) 是 SentencePiece 词首空格
        let text = text.replace('\u{2581}', " ");
        let text = text.trim();
        Self::add_cjk_spacing(text)
    }

    /// 在中文字符和拉丁字符之间插入空格
    fn add_cjk_spacing(text: &str) -> String {
        let mut result = String::with_capacity(text.len() + 16);
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            result.push(chars[i]);
            if i + 1 < chars.len() {
                let a = Self::is_cjk(chars[i]);
                let b = Self::is_cjk(chars[i + 1]);
                if a != b && chars[i] != ' ' && chars[i + 1] != ' ' {
                    result.push(' ');
                }
            }
        }
        result
    }

    fn is_cjk(c: char) -> bool {
        matches!(c as u32,
            0x4E00..=0x9FFF |   // CJK 统一表意文字
            0x3400..=0x4DBF |   // CJK 扩展 A
            0x20000..=0x2A6DF | // CJK 扩展 B
            0x3000..=0x303F |   // CJK 符号和标点
            0xFF00..=0xFFEF     // 全角字符
        )
    }

    pub fn hardware_info(&self) -> &str { &self.hw_info }
    pub fn is_using_gpu(&self) -> bool  { self.use_gpu }
}

// ─────────────────────────── 独立后处理函数 ───────────────────────────

/// 去除语气词
#[allow(dead_code)]
pub fn remove_fillers(text: &str) -> String {
    let fillers = ["嗯", "啊", "呃", "哦", "哎", "um", "uh", "hmm", "ah", "er"];
    let mut result = text.to_string();
    for f in &fillers { result = result.replace(f, ""); }
    while result.contains("  ") { result = result.replace("  ", " "); }
    result.trim().to_string()
}

/// 句首字母大写
#[allow(dead_code)]
pub fn capitalize_sentences(text: &str) -> String {
    let mut result = String::new();
    let mut cap_next = true;
    for c in text.chars() {
        if cap_next && c.is_ascii_lowercase() {
            result.push(c.to_ascii_uppercase());
            cap_next = false;
        } else {
            result.push(c);
            if matches!(c, '.' | '?' | '!' | '。' | '？' | '！') {
                cap_next = true;
            }
        }
    }
    result
}
