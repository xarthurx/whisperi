use anyhow::Result;
use serde::{Deserialize, Serialize};

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

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
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

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API error ({}): {}", status, body);
    }

    let result: ResponsesResponse = response.json().await?;

    // Extract text from the first message output
    let text = result
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .filter_map(|item| item.content.as_ref())
        .flatten()
        .filter(|block| block.block_type == "output_text")
        .filter_map(|block| block.text.clone())
        .collect::<Vec<String>>()
        .join("");

    Ok(text)
}
