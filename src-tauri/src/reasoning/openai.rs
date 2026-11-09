use anyhow::Result;
use serde::{Deserialize, Serialize};

// --- Responses API types ---

#[derive(Serialize)]
struct ResponsesRequest {
    model: String,
    input: Vec<InputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct InputItem {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ResponsesResponse {
    output: Vec<OutputItem>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<ContentBlock>>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

// --- Chat Completions API types ---

/// Groq's OpenAI-compatible endpoint.
pub const GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";

/// Completion budget (reasoning + answer) for gpt-oss cleanup on Groq.
///
/// Groq caps a request that names no limit at 2,048 completion tokens, and
/// gpt-oss counts its hidden reasoning against that cap. At the default
/// (medium) effort, Light cleanup of a 100–200 character dictation often
/// spent more than 2,000 tokens reasoning (up to 8,190) — it checks each word
/// against the filler list — so the reply hit the cap with no text.
/// Low effort used at most ~2,900 reasoning tokens on the same dictations;
/// 8,192 leaves room for a long dictation's answer on top.
const GPT_OSS_MAX_COMPLETION_TOKENS: u32 = 8192;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    base_url: Option<&str>,
) -> Result<String> {
    let client = &*crate::http::HTTP_CLIENT;
    let base = base_url.unwrap_or("https://api.openai.com/v1");

    // Try Responses API first (newer models) — only for OpenAI
    if base_url.is_none() {
        match complete_responses(client, api_key, model, system_prompt, user_text, max_tokens, temperature, base).await {
            Ok(text) => return Ok(text),
            Err(e) => {
                log::debug!("Responses API failed, falling back to Chat Completions: {}", e);
            }
        }
    }

    // Fall back to Chat Completions API
    complete_chat(client, api_key, model, system_prompt, user_text, max_tokens, temperature, base).await
}

async fn complete_responses(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    base_url: &str,
) -> Result<String> {
    let request = ResponsesRequest {
        model: model.to_string(),
        input: vec![
            InputItem {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            InputItem {
                role: "user".to_string(),
                content: user_text.to_string(),
            },
        ],
        max_output_tokens: max_tokens,
        temperature,
    };

    let response = client
        .post(format!("{}/responses", base_url))
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;

    let response = crate::http::check_response(response, "OpenAI Responses API error").await?;

    let result: ResponsesResponse = response.json().await?;

    let text = result
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .filter_map(|item| item.content.as_ref())
        .flatten()
        .filter(|block| block.block_type == "output_text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}

async fn complete_chat(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    base_url: &str,
) -> Result<String> {
    let request = build_chat_request(model, system_prompt, user_text, max_tokens, temperature, base_url);

    let url = format!("{}/chat/completions", base_url);
    log::info!("[Whisperi] POST {} (model={})", url, model);
    let mut req_builder = client
        .post(&url)
        .bearer_auth(api_key);

    // OpenRouter requires these headers for proper authentication routing
    if base_url.contains("openrouter.ai") {
        req_builder = req_builder
            .header("HTTP-Referer", "https://github.com/xarthurx/whisperi")
            .header("X-Title", "Whisperi");
    }

    let response = req_builder
        .json(&request)
        .send()
        .await?;

    let response = crate::http::check_response(response, "Chat API error").await?;

    let result: crate::http::ChatCompletionsResponse = response.json().await?;

    result.complete_text()
}

/// gpt-oss on Groq gets low reasoning effort and an explicit budget (see
/// [`GPT_OSS_MAX_COMPLETION_TOKENS`]); dictation cleanup does not need deep
/// reasoning. Other providers and models keep their defaults — only this
/// combination was verified against the live API.
fn build_chat_request(
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    base_url: &str,
) -> ChatRequest {
    let groq_gpt_oss = base_url == GROQ_BASE_URL && model.contains("gpt-oss");
    ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_text.to_string(),
            },
        ],
        max_tokens,
        max_completion_tokens: (groq_gpt_oss && max_tokens.is_none())
            .then_some(GPT_OSS_MAX_COMPLETION_TOKENS),
        reasoning_effort: groq_gpt_oss.then_some("low"),
        temperature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(model: &str, max_tokens: Option<u32>, base_url: &str) -> serde_json::Value {
        serde_json::to_value(build_chat_request(
            model,
            "system",
            "[TRANSCRIBED_SPEECH]: text",
            max_tokens,
            Some(0.1),
            base_url,
        ))
        .unwrap()
    }

    #[test]
    fn groq_gpt_oss_gets_low_reasoning_effort_and_an_explicit_budget() {
        for model in ["openai/gpt-oss-20b", "openai/gpt-oss-120b"] {
            let body = body(model, None, GROQ_BASE_URL);

            assert_eq!(body["reasoning_effort"], "low", "{model}");
            assert_eq!(body["max_completion_tokens"], 8192, "{model}");
            assert!(body.get("max_tokens").is_none(), "{model}");
            assert_eq!(body["temperature"], 0.1, "{model}");
        }
    }

    #[test]
    fn explicit_max_tokens_is_kept_without_a_second_budget() {
        let body = body("openai/gpt-oss-20b", Some(1000), GROQ_BASE_URL);

        assert_eq!(body["max_tokens"], 1000);
        assert!(body.get("max_completion_tokens").is_none());
        assert_eq!(body["reasoning_effort"], "low");
    }

    /// Only the Groq + gpt-oss combination was verified live; other providers
    /// and models keep the request they had.
    #[test]
    fn reasoning_controls_are_scoped_to_groq_gpt_oss() {
        for (model, base_url) in [
            ("qwen/qwen3.6-27b", GROQ_BASE_URL),
            ("openai/gpt-oss-20b", "https://openrouter.ai/api/v1"),
            ("qwen3.7-plus", "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
            ("gpt-5.6-terra", "https://api.openai.com/v1"),
        ] {
            let body = body(model, None, base_url);

            assert!(body.get("reasoning_effort").is_none(), "{model} @ {base_url}");
            assert!(body.get("max_completion_tokens").is_none(), "{model} @ {base_url}");
            assert!(body.get("max_tokens").is_none(), "{model} @ {base_url}");
        }
    }
}
