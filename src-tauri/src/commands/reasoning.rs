use crate::reasoning::{self, ReasoningRequest};

#[tauri::command]
pub async fn process_reasoning(
    text: String,
    model: String,
    provider: String,
    system_prompt: String,
    api_key: String,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    let req = ReasoningRequest {
        text,
        model,
        provider,
        system_prompt,
        api_key,
        max_tokens,
    };

    let response = reasoning::process(&req).await.map_err(|e| e.to_string())?;
    Ok(response.text)
}
