// SenseVoice ASR 引擎模块
mod decode;
mod feature;
mod model;

use std::sync::Mutex;
use log::info;
use ort::session::Session;

pub struct SenseVoiceEngine {
    session: Mutex<Session>,
    tokens: Vec<String>,
    cmvn_means: Vec<f32>,
    cmvn_scales: Vec<f32>,
    #[allow(dead_code)]
    use_gpu: bool,
    hw_info: String,
}

impl SenseVoiceEngine {
    pub fn new() -> anyhow::Result<Self> {
        let model_dir   = model::get_model_dir()?;
        let model_path  = model_dir.join("model.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        let cmvn_path   = model_dir.join("am.mvn");

        if !model_path.exists() {
            return Err(anyhow::anyhow!("模型文件不存在: {:?}", model_path));
        }
        info!("加载ONNX模型: {:?}", model_path);

        let (session, use_gpu) = match model::build_session(&model_path, true) {
            Ok(s) => { info!("使用DirectML GPU加速"); (s, true) }
            Err(e) => {
                log::warn!("DirectML失败: {}，回退CPU", e);
                (model::build_session(&model_path, false)?, false)
            }
        };

        let tokens = model::load_tokens(&tokens_path)?;
        let (cmvn_means, cmvn_scales) = model::load_cmvn(&cmvn_path)?;
        let hw_info = if use_gpu {
            "SenseVoice · DirectML (GPU)".to_string()
        } else {
            "SenseVoice · CPU".to_string()
        };

        info!("ASR引擎初始化完成: {}", hw_info);
        Ok(Self { session: Mutex::new(session), tokens, cmvn_means, cmvn_scales, use_gpu, hw_info })
    }

    pub fn recognize(&self, samples: &[f32]) -> anyhow::Result<String> {
        if samples.len() < feature::FRAME_LEN {
            return Ok(String::new());
        }

        let (feats_flat, t_lfr) = feature::extract_features(&self.cmvn_means, &self.cmvn_scales, samples)?;
        let feat_len = t_lfr as i32;

        let speech_tensor = ort::value::Tensor::<f32>::from_array(
            ([1usize, t_lfr, feature::N_MELS * feature::LFR_M], feats_flat.into_boxed_slice())
        )?;
        let len_tensor = ort::value::Tensor::<i32>::from_array(
            ([1usize], vec![feat_len].into_boxed_slice())
        )?;
        let lang_tensor = ort::value::Tensor::<i32>::from_array(
            ([1usize], vec![0i32].into_boxed_slice())
        )?;
        let textnorm_tensor = ort::value::Tensor::<i32>::from_array(
            ([1usize], vec![15i32].into_boxed_slice())
        )?;

        let mut session = self.session.lock().unwrap();
        let outputs = session.run(ort::inputs! {
            "speech"         => speech_tensor,
            "speech_lengths" => len_tensor,
            "language"       => lang_tensor,
            "textnorm"       => textnorm_tensor,
        })?;

        let logits_val = &outputs["ctc_logits"];
        let (shape, logits_data) = logits_val.try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("提取输出张量失败: {}", e))?;
        let time_steps = shape[1] as usize;
        let vocab_size = shape[2] as usize;

        Ok(decode::ctc_greedy_decode(&self.tokens, logits_data, time_steps, vocab_size))
    }

    pub fn hardware_info(&self) -> &str { &self.hw_info }
    #[allow(dead_code)]
    pub fn is_using_gpu(&self) -> bool  { self.use_gpu }
}
