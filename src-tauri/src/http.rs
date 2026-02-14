use anyhow::Result;
use reqwest::Response;
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
