// 说人话 LLM 润色模块
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use log::info;

// ─── 数据结构 ──────────────────────────────────────────────────────────────────

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

// ─── LLM Provider trait ───────────────────────────────────────────────────────

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送单轮润色请求，返回润色后文字
    async fn refine(&self, system_prompt: &str, user_text: &str) -> anyhow::Result<String>;
    /// 测试连通性，返回简短的成功描述
    async fn ping(&self) -> anyhow::Result<String>;
}

// ─── OpenAI Chat Completions 格式（openai_compatible + vllm 共用） ────────────

pub struct OpenAICompatibleProvider {
    config: LlmProviderConfig,
    client: reqwest::Client,
}

impl OpenAICompatibleProvider {
    pub fn new(config: LlmProviderConfig) -> anyhow::Result<Self> {
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()?;
        Ok(Self { config, client })
    }

    fn chat_url(&self) -> String {
        let base = self.config.api_base_url.trim_end_matches('/');
        format!("{}/chat/completions", base)
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if !self.config.api_key.is_empty() {
            let bearer = format!("Bearer {}", self.config.api_key);
            if let Ok(v) = reqwest::header::HeaderValue::from_str(&bearer) {
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
        }
        headers
    }
}

#[async_trait]
impl LlmProvider for OpenAICompatibleProvider {
    async fn refine(&self, system_prompt: &str, user_text: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": self.config.model_name,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user",   "content": user_text }
            ],
            "max_tokens":  self.config.max_tokens,
            "temperature": self.config.temperature,
            "stream":      false
        });

        info!("[LLM] 发送润色请求 → {}", self.chat_url());

        let resp = self.client
            .post(&self.chat_url())
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("HTTP {} - {}", status, text));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("响应格式异常: {}", text))?;

        Ok(clean_llm_output(content))
    }

    async fn ping(&self) -> anyhow::Result<String> {
        // 用最小 token 发一个轻量请求测试可用性
        let body = serde_json::json!({
            "model": self.config.model_name,
            "messages": [{ "role": "user", "content": "hi" }],
            "max_tokens": 1
        });
        let resp = self.client
            .post(&self.chat_url())
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(format!("已连接：{}", self.config.model_name))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!("HTTP {} - {}", status, body))
        }
    }
}

// ─── Ollama 原生 API (/api/chat) ──────────────────────────────────────────────

pub struct OllamaProvider {
    config: LlmProviderConfig,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(config: LlmProviderConfig) -> anyhow::Result<Self> {
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()?;
        Ok(Self { config, client })
    }

    fn chat_url(&self) -> String {
        let base = self.config.api_base_url.trim_end_matches('/');
        format!("{}/api/chat", base)
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn refine(&self, system_prompt: &str, user_text: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": self.config.model_name,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user",   "content": user_text }
            ],
            "options": {
                "num_predict": self.config.max_tokens,
                "temperature": self.config.temperature
            },
            "stream": false
        });

        info!("[LLM/Ollama] 发送润色请求 → {}", self.chat_url());

        let resp = self.client
            .post(&self.chat_url())
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("HTTP {} - {}", status, text));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        // Ollama 非流式响应结构：{ "message": { "role": "assistant", "content": "..." } }
        let content = json["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Ollama 响应格式异常: {}", text))?;

        Ok(clean_llm_output(content))
    }

    async fn ping(&self) -> anyhow::Result<String> {
        // Ollama 使用 /api/tags 列出模型来测试连通性
        let base = self.config.api_base_url.trim_end_matches('/');
        let tags_url = format!("{}/api/tags", base);

        let resp = self.client.get(&tags_url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("HTTP {}", resp.status()));
        }
        let json: serde_json::Value = resp.json().await?;
        let model_count = json["models"].as_array().map(|a| a.len()).unwrap_or(0);
        Ok(format!("已连接 Ollama，已安装 {} 个模型", model_count))
    }
}

// ─── 工厂：根据配置创建对应的 Provider ────────────────────────────────────────

pub fn create_provider(config: LlmProviderConfig) -> anyhow::Result<Box<dyn LlmProvider>> {
    match config.provider_type {
        LlmProviderType::OpenaiCompatible | LlmProviderType::Vllm =>
            Ok(Box::new(OpenAICompatibleProvider::new(config)?)),
        LlmProviderType::Ollama =>
            Ok(Box::new(OllamaProvider::new(config)?)),
    }
}

// ─── 内置人设 ─────────────────────────────────────────────────────────────────

pub fn builtin_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "formal".into(),
            name: "正式书面".into(),
            description: Some("工作邮件、汇报、公文".into()),
            system_prompt: "你是一个文字润色助手。将用户输入的口语化文字改写为正式、书面的表达方式。\n要求：\n- 保持原意，不添加、不删减实质内容\n- 使用正式语气，避免口语化词汇\n- 语句通顺，标点规范\n- 直接输出改写后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "concise".into(),
            name: "简洁精炼".into(),
            description: Some("备忘录、清单、摘要".into()),
            system_prompt: "你是一个文字压缩助手。将用户输入的文字提炼为简洁、清晰的表达。\n要求：\n- 去除冗余词语和重复内容\n- 保留核心信息\n- 每个要点尽量控制在一句话内\n- 直接输出改写后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "casual".into(),
            name: "口语自然".into(),
            description: Some("聊天、日常沟通".into()),
            system_prompt: "你是一个文字整理助手。将用户输入的语音文字整理为自然、流畅的口语表达。\n要求：\n- 保持口语风格，但去除明显的停顿词（嗯、啊、那个等）\n- 语句连贯，易于阅读\n- 保持原有的情感和语气\n- 直接输出整理后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "logical".into(),
            name: "逻辑严谨".into(),
            description: Some("技术文档、分析报告".into()),
            system_prompt: "你是一个逻辑整理助手。将用户输入的文字重新组织为条理清晰、逻辑严谨的表达。\n要求：\n- 识别并明确论点、论据、结论\n- 使用条件、因果、递进等逻辑连接词\n- 如有多个要点，使用编号或分段表达\n- 直接输出整理后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "creative".into(),
            name: "创意文案".into(),
            description: Some("营销、推广、故事".into()),
            system_prompt: "你是一个创意写作助手。将用户输入的文字改写为生动、有感染力的创意表达。\n要求：\n- 可以适当使用比喻、排比等修辞手法\n- 语言生动，有画面感\n- 保持原意的核心信息\n- 直接输出创作后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "translator".into(),
            name: "中译英".into(),
            description: Some("输入中文，输出英文翻译".into()),
            system_prompt: "你是一个专业翻译。将用户输入的中文翻译为自然流畅的英文。\n要求：\n- 准确传达原文含义\n- 使用地道的英文表达，避免直译\n- 保持原文的语气和风格\n- 直接输出翻译结果，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "en_to_zh".into(),
            name: "英译中".into(),
            description: Some("输入英文，输出中文翻译".into()),
            system_prompt: "你是一个专业翻译。将用户输入的英文翻译为自然流畅的中文。\n要求：\n- 准确传达原文含义\n- 使用地道的中文表达\n- 保持原文的语气和风格\n- 直接输出翻译结果，不加任何解释或前缀".into(),
            is_builtin: true,
        },
    ]
}

// ─── 润色结果清洗 ─────────────────────────────────────────────────────────────

fn clean_llm_output(s: &str) -> String {
    let s = s.trim();
    // 去除 markdown 代码块包裹（```text ... ``` 或 ``` ... ```）
    let s = if s.starts_with("```") {
        let after_first = s.trim_start_matches('`');
        // 跳过语言标识行（如 `text`）
        let after_lang = if let Some(newline) = after_first.find('\n') {
            after_first[newline + 1..].trim_start()
        } else {
            after_first
        };
        // 去除尾部 ```
        after_lang.trim_end_matches('`').trim_end_matches('\n').trim()
    } else {
        s
    };
    s.to_string()
}

// ─── 核心润色入口（供 main.rs 调用） ─────────────────────────────────────────

/// 移除 LLM 输出中的 <think>...</think> 思考块（支持嵌套、跨行）
fn regex_strip_think(s: &str) -> String {
    let mut result = String::new();
    let mut rest = s;
    loop {
        if let Some(start) = rest.find("<think>") {
            result.push_str(&rest[..start]);
            let after = &rest[start + 7..];
            if let Some(end) = after.find("</think>") {
                rest = &after[end + 8..];
            } else {
                result.push_str(after);
                break;
            }
        } else {
            result.push_str(rest);
            break;
        }
    }
    result.trim().to_string()
}

/// 根据当前配置对 `raw_text` 进行 LLM 润色。
/// 失败时返回 Err，调用方应降级使用原始文字。
/// 完整润色流程：构建 provider → 调用 refine → 返回润色文字
pub async fn do_refine(
    provider_config: &LlmProviderConfig,
    persona: &Persona,
    raw_text: &str,
) -> anyhow::Result<String> {
    let provider = create_provider(provider_config.clone())?;
    let result = provider.refine(&persona.system_prompt, raw_text).await?;
    // 过滤掉 <think>...</think> 思考过程，只保留实际结果
    let result = regex_strip_think(&result);
    info!("[LLM] 润色完成: {} chars → {} chars", raw_text.len(), result.len());
    Ok(result)
}

/// 连接测试
pub async fn test_provider(provider_config: LlmProviderConfig) -> anyhow::Result<String> {
    let provider = create_provider(provider_config)?;
    provider.ping().await
}
