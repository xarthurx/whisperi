use anyhow::Result;
use reqwest::Response;
use serde::Deserialize;
use std::sync::LazyLock;

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Whisperi")
        .build()
        .expect("Failed to build HTTP client")
});

/// Check an HTTP response for errors; on non-2xx, read the body and bail with a descriptive message.
pub async fn check_response(response: Response, context: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    log::error!("[Whisperi] {} ({}): {}", context, status, body);
    anyhow::bail!("{} ({}): {}", context, status, body)
}

/// Common "chat completions" response shape used by OpenAI, Groq, Qwen, OpenRouter, etc.
#[derive(Deserialize)]
pub struct ChatCompletionsResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
pub struct ChatChoice {
    pub message: ChatChoiceMessage,
}

#[derive(Deserialize)]
pub struct ChatChoiceMessage {
    pub content: Option<String>,
}

impl ChatCompletionsResponse {
    /// Extract the text from the first choice, or empty string if none.
    pub fn text(&self) -> String {
        self.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default()
    }
}
