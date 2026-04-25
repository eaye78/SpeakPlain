// LLM 数据类型与 trait

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderType {
    OpenaiCompatible,
    Ollama,
    Vllm,
}

impl std::fmt::Display for LlmProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmProviderType::OpenaiCompatible => write!(f, "openai_compatible"),
            LlmProviderType::Ollama           => write!(f, "ollama"),
            LlmProviderType::Vllm             => write!(f, "vllm"),
        }
    }
}

impl std::str::FromStr for LlmProviderType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "openai_compatible" => Ok(LlmProviderType::OpenaiCompatible),
            "ollama"            => Ok(LlmProviderType::Ollama),
            "vllm"              => Ok(LlmProviderType::Vllm),
            _                   => Err(anyhow::anyhow!("未知 provider 类型: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub id:            String,
    pub name:          String,
    pub provider_type: LlmProviderType,
    pub api_base_url:  String,
    pub api_key:       String,
    pub model_name:    String,
    pub timeout_secs:  u64,
    pub max_tokens:    u32,
    pub temperature:   f32,
}

impl LlmProviderConfig {
    /// 根据 provider_type 返回预填的默认配置
    pub fn default_for(provider_type: LlmProviderType) -> Self {
        let (api_base_url, model_name, timeout_secs) = match provider_type {
            LlmProviderType::OpenaiCompatible =>
                ("https://api.openai.com/v1".to_string(), "gpt-4o-mini".to_string(), 30u64),
            LlmProviderType::Ollama =>
                ("http://localhost:11434".to_string(), "qwen2.5:7b".to_string(), 120u64),
            LlmProviderType::Vllm =>
                ("http://localhost:8000/v1".to_string(), "Qwen/Qwen2.5-7B-Instruct".to_string(), 120u64),
        };
        Self {
            id:            uuid::Uuid::new_v4().to_string(),
            name:          provider_type.to_string(),
            provider_type,
            api_base_url,
            api_key:       String::new(),
            model_name,
            timeout_secs,
            max_tokens:    512,
            temperature:   0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub id:            String,
    pub name:          String,
    pub description:   Option<String>,
    pub system_prompt: String,
    pub is_builtin:    bool,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送单轮润色请求，返回润色后文字
    async fn refine(&self, system_prompt: &str, user_text: &str) -> anyhow::Result<String>;
    /// 测试连通性，返回简短的成功描述
    async fn ping(&self) -> anyhow::Result<String>;
}
