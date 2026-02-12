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
    let key_preview = if api_key.len() > 8 {
        format!("{}...{}", &api_key[..4], &api_key[api_key.len()-4..])
    } else {
        "(too short)".to_string()
    };
    log::info!("[Whisperi] Enhancing: provider={}, model={}, key={}", provider, model, key_preview);

    let req = ReasoningRequest {
        text,
        model,
        provider,
        system_prompt,
        api_key,
        max_tokens,
    };

    match reasoning::process(&req).await {
        Ok(response) => {
            log::info!("[Whisperi] Enhancement complete ({} chars)", response.text.len());
            Ok(response.text)
        }
        Err(e) => {
            log::error!("[Whisperi] Enhancement failed: {}", e);
            Err(e.to_string())
        }
    }
}
