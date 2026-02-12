use anyhow::Result;
use serde::{Deserialize, Serialize};

// --- Responses API types ---

#[derive(Serialize)]
struct ResponsesRequest {
    model: String,
    input: Vec<InputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
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

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    base_url: Option<&str>,
) -> Result<String> {
    let client = &*crate::HTTP_CLIENT;
    let base = base_url.unwrap_or("https://api.openai.com/v1");

    // Try Responses API first (newer models) — only for OpenAI
    if base_url.is_none() {
        match complete_responses(client, api_key, model, system_prompt, user_text, max_tokens, base).await {
            Ok(text) => return Ok(text),
            Err(e) => {
                log::debug!("Responses API failed, falling back to Chat Completions: {}", e);
            }
        }
    }

    // Fall back to Chat Completions API
    complete_chat(client, api_key, model, system_prompt, user_text, max_tokens, base).await
}

async fn complete_responses(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
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
    };

    let response = client
        .post(format!("{}/responses", base_url))
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI Responses API error ({}): {}", status, body);
    }

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
    base_url: &str,
) -> Result<String> {
    let request = ChatRequest {
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
    };

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

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::error!("[Whisperi] Chat API error ({}): {}", status, body);
        anyhow::bail!("Chat API error ({}): {}", status, body);
    }

    let result: ChatResponse = response.json().await?;

    let text = result
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();

    Ok(text)
}
