use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
) -> Result<String> {
    let request = MessagesRequest {
        model: model.to_string(),
        max_tokens: max_tokens.unwrap_or(4096),
        system: system_prompt.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: user_text.to_string(),
        }],
        temperature,
    };

    let response = crate::http::HTTP_CLIENT
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    let response = crate::http::check_response(response, "Anthropic API error").await?;

    let result: MessagesResponse = response.json().await?;

    let text = result
        .content
        .iter()
        .filter(|block| block.block_type == "text")
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}
