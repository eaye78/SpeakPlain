// 统一 ASR 引擎接口模块
use crate::asr_sensevoice::SenseVoiceEngine;
use crate::asr_qwen3asr::Qwen3ASREngine;
use log::info;
use std::path::PathBuf;

/// 查找 models 根目录，适配开发环境和生产环境
/// 
/// 开发环境：exe 在 `target/debug/speakplain.exe`
///   → 上两级到 `SpeakPlain/`，再进 `speakplain/models/`
/// 生产环境：exe 在安装目录，models 与 exe 同级
pub fn find_models_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(ref exe_dir) = exe_dir {
        // 1. 生产环境：exe 同级 models/
        candidates.push(exe_dir.join("models"));

        // 2. 开发环境：exe 在 target/debug/，上两级到工作区根，再进 speakplain/models/
        if let Some(workspace_root) = exe_dir.parent().and_then(|p| p.parent()) {
            candidates.push(workspace_root.join("speakplain").join("models"));
            // 3. 若工作区根本身就有 models/（其他布局）
            candidates.push(workspace_root.join("models"));
        }
    }

    // 4. 当前工作目录 models/（`cargo tauri dev` 时 CWD 为 speakplain/）
    candidates.push(PathBuf::from("models"));

    candidates.into_iter().find(|p| p.exists())
}

/// ASR 引擎类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ASRModelType {
    #[serde(rename = "sensevoice")]
    SenseVoice,
    #[serde(rename = "qwen3-asr")]
    Qwen3ASR,
}

impl Default for ASRModelType {
    fn default() -> Self {
        ASRModelType::SenseVoice
    }
}

impl std::fmt::Display for ASRModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ASRModelType::SenseVoice => write!(f, "sensevoice"),
            ASRModelType::Qwen3ASR => write!(f, "qwen3-asr"),
        }
    }
}

impl std::str::FromStr for ASRModelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sensevoice" | "sense_voice" => Ok(ASRModelType::SenseVoice),
            "qwen3-asr" | "qwen3_asr" | "qwen3" => Ok(ASRModelType::Qwen3ASR),
            _ => Err(format!("未知的 ASR 模型类型: {}", s)),
        }
    }
}

/// 统一的 ASR 引擎包装
pub enum ASREngine {
    SenseVoice(SenseVoiceEngine),
    Qwen3ASR(Qwen3ASREngine),
}

impl ASREngine {
    /// 创建新的 ASR 引擎
    pub fn new(model_type: ASRModelType) -> anyhow::Result<Self> {
        match model_type {
            ASRModelType::SenseVoice => {
                info!("初始化 SenseVoice 引擎...");
                let engine = SenseVoiceEngine::new()?;
                Ok(ASREngine::SenseVoice(engine))
            }
            ASRModelType::Qwen3ASR => {
                info!("初始化 Qwen3-ASR 引擎...");
                let engine = Qwen3ASREngine::new()?;
                Ok(ASREngine::Qwen3ASR(engine))
            }
        }
    }

    /// 语音识别
    pub fn recognize(&self, samples: &[f32]) -> anyhow::Result<String> {
        match self {
            ASREngine::SenseVoice(engine) => engine.recognize(samples),
            ASREngine::Qwen3ASR(engine) => engine.recognize(samples),
        }
    }

    /// 获取硬件信息
    pub fn hardware_info(&self) -> String {
        match self {
            ASREngine::SenseVoice(engine) => engine.hardware_info().to_string(),
            ASREngine::Qwen3ASR(engine) => engine.hardware_info().to_string(),
        }
    }

    /// 是否使用 GPU
    #[allow(dead_code)]
    pub fn is_using_gpu(&self) -> bool {
        match self {
            ASREngine::SenseVoice(engine) => engine.is_using_gpu(),
            ASREngine::Qwen3ASR(engine) => engine.is_using_gpu(),
        }
    }

    /// 获取当前引擎类型
    #[allow(dead_code)]
    pub fn model_type(&self) -> ASRModelType {
        match self {
            ASREngine::SenseVoice(_) => ASRModelType::SenseVoice,
            ASREngine::Qwen3ASR(_) => ASRModelType::Qwen3ASR,
        }
    }
}

/// 获取可用的 ASR 模型列表
pub fn get_available_models() -> Vec<(ASRModelType, String, bool)> {
    let mut models = Vec::new();

    // 检查 SenseVoice
    let sensevoice_available = check_sensevoice_available();
    models.push((
        ASRModelType::SenseVoice,
        "SenseVoice (阿里通义)".to_string(),
        sensevoice_available,
    ));

    // 检查 Qwen3-ASR
    let qwen3_available = crate::asr_qwen3asr::is_qwen3_model_available();
    models.push((
        ASRModelType::Qwen3ASR,
        "Qwen3-ASR-0.6B (阿里通义千问)".to_string(),
        qwen3_available,
    ));

    models
}

/// 检查 SenseVoice 模型是否可用
fn check_sensevoice_available() -> bool {
    find_models_dir()
        .map(|models| models.join("sensevoice").join("model.onnx").exists())
        .unwrap_or(false)
}

/// 获取指定模型类型的模型信息
pub fn get_model_info(model_type: ASRModelType) -> (String, String, bool) {
    let available = match model_type {
        ASRModelType::SenseVoice => check_sensevoice_available(),
        ASRModelType::Qwen3ASR => crate::asr_qwen3asr::is_qwen3_model_available(),
    };

    let (name, description) = match model_type {
        ASRModelType::SenseVoice => (
            "SenseVoice".to_string(),
            "阿里通义语音识别模型，支持多语言".to_string(),
        ),
        ASRModelType::Qwen3ASR => (
            "Qwen3-ASR-0.6B".to_string(),
            "阿里通义千问 3 语音识别模型，0.6B 参数".to_string(),
        ),
    };

    (name, description, available)
}
