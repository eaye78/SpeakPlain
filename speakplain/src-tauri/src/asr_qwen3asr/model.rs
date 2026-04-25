use ort::session::{Session, builder::GraphOptimizationLevel};
use std::path::PathBuf;

use super::Qwen3ASREngine;

impl Qwen3ASREngine {
    pub(crate) fn build_session(model_path: &PathBuf) -> anyhow::Result<Session> {
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

    pub(crate) fn load_embed_tokens(path: &PathBuf) -> anyhow::Result<Vec<f32>> {
        let bytes = std::fs::read(path)?;
        let mut tokens = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            tokens.push(val);
        }
        Ok(tokens)
    }

    pub(crate) fn load_tokenizer_vocab(path: &PathBuf) -> anyhow::Result<(std::collections::HashMap<String, usize>, std::collections::HashMap<usize, String>)> {
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

    pub(crate) fn get_model_dir() -> anyhow::Result<PathBuf> {
        crate::asr::find_models_dir()
            .map(|models| models.join("Qwen3-ASR-0.6B-ONNX-CPU"))
            .filter(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("未找到 Qwen3-ASR 模型目录"))
    }
}
