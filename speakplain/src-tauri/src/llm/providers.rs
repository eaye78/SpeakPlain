// LLM Provider 实现

use async_trait::async_trait;
use log::info;

use crate::llm::types::{LlmProviderConfig, LlmProviderType, LlmProvider};

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

// ─── 润色结果清洗 ─────────────────────────────────────────────────────────────

pub fn clean_llm_output(s: &str) -> String {
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
