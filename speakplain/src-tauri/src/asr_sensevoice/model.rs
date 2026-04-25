use std::path::PathBuf;
use ort::session::{Session, builder::GraphOptimizationLevel};
use log::{info, warn};

use crate::asr::find_models_dir;

pub fn build_session(model_path: &PathBuf, use_gpu: bool) -> anyhow::Result<Session> {
    info!("开始创建 ONNX Session... (GPU={})", use_gpu);

    let mut builder = Session::builder()
        .map_err(|e| anyhow::anyhow!("创建 session builder 失败: {}", e))?
        .with_intra_threads(4)
        .map_err(|e| anyhow::anyhow!("设置线程数失败: {}", e))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow::anyhow!("设置优化级别失败: {}", e))?;

    if use_gpu {
        builder = builder.with_execution_providers([ort::execution_providers::DirectMLExecutionProvider::default().build()])
            .map_err(|e| anyhow::anyhow!("配置 DirectML 失败: {}", e))?;
        info!("已配置 DirectML GPU 执行提供程序");
    }

    let session = builder.commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("加载模型失败: {}", e))?;

    info!("ONNX Session 创建完成");
    Ok(session)
}

pub fn load_tokens(tokens_path: &PathBuf) -> anyhow::Result<Vec<String>> {
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
pub fn load_cmvn(cmvn_path: &PathBuf) -> anyhow::Result<(Vec<f32>, Vec<f32>)> {
    const N_MELS: usize = 80;
    const LFR_M: usize = 7;
    const DIM: usize = N_MELS * LFR_M;

    if !cmvn_path.exists() {
        warn!("am.mvn 不存在，跳过 CMVN");
        return Ok((vec![0.0f32; DIM], vec![1.0f32; DIM]));
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

pub fn get_model_dir() -> anyhow::Result<PathBuf> {
    find_models_dir()
        .map(|models| models.join("sensevoice"))
        .filter(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!("未找到模型目录，请将模型放到 models/sensevoice/ 下"))
}
