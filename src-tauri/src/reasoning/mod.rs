pub mod anthropic;
pub mod gemini;
pub mod openai;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningRequest {
    pub text: String,
    pub model: String,
    pub provider: String,
    pub system_prompt: String,
    pub api_key: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResponse {
    pub text: String,
    pub model: String,
    pub provider: String,
}

/// Process text through the appropriate AI provider
pub async fn process(req: &ReasoningRequest) -> Result<ReasoningResponse> {
    let text = match req.provider.as_str() {
        "openai" => {
            openai::complete(&req.api_key, &req.model, &req.system_prompt, &req.text, req.max_tokens, req.temperature, None).await?
        }
        "groq" => {
            openai::complete(
                &req.api_key, &req.model, &req.system_prompt, &req.text,
                req.max_tokens, req.temperature, Some("https://api.groq.com/openai/v1"),
            ).await?
        }
        "qwen" => {
            openai::complete(
                &req.api_key, &req.model, &req.system_prompt, &req.text,
                req.max_tokens, req.temperature, Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
            ).await?
        }
        "openrouter" => {
            openai::complete(
                &req.api_key, &req.model, &req.system_prompt, &req.text,
                req.max_tokens, req.temperature, Some("https://openrouter.ai/api/v1"),
            ).await?
        }
        "anthropic" => {
            anthropic::complete(&req.api_key, &req.model, &req.system_prompt, &req.text, req.max_tokens, req.temperature).await?
        }
        "gemini" => {
            gemini::complete(&req.api_key, &req.model, &req.system_prompt, &req.text, req.max_tokens, req.temperature).await?
        }
        other => anyhow::bail!("Unknown reasoning provider: {}", other),
    };

    Ok(ReasoningResponse {
        text,
        model: req.model.clone(),
        provider: req.provider.clone(),
    })
}
