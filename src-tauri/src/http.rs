use anyhow::Result;
use reqwest::Response;
use serde::Deserialize;
use std::sync::LazyLock;

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Whisperi")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(120))
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
    pub usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
pub struct ChatChoice {
    pub message: ChatChoiceMessage,
    /// `stop` when the reply ended on its own, `length` when the output budget ran out.
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct ChatChoiceMessage {
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct ChatUsage {
    pub completion_tokens: Option<u32>,
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Deserialize)]
pub struct CompletionTokensDetails {
    pub reasoning_tokens: Option<u32>,
}

impl ChatCompletionsResponse {
    /// Extract the text from the first choice, or empty string if none.
    pub fn text(&self) -> String {
        self.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default()
    }

    /// The text of a reply that ended on its own. A `length` finish means the
    /// output budget ran out: a reasoning model can spend all of it before
    /// writing any answer (empty text) or stop mid-sentence, so the text is not
    /// a complete answer and must not stand in for one.
    pub fn complete_text(&self) -> Result<String> {
        let finish_reason = self.choices.first().and_then(|c| c.finish_reason.as_deref());
        if finish_reason == Some("length") {
            let usage = self.usage.as_ref();
            let count = |n: Option<u32>| n.map_or_else(|| "unknown".to_string(), |n| n.to_string());
            anyhow::bail!(
                "Reply cut off at the provider's output limit (finish_reason=length; completion tokens: {}, reasoning tokens: {})",
                count(usage.and_then(|u| u.completion_tokens)),
                count(
                    usage
                        .and_then(|u| u.completion_tokens_details.as_ref())
                        .and_then(|d| d.reasoning_tokens)
                ),
            );
        }
        Ok(self.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> ChatCompletionsResponse {
        serde_json::from_str(json).expect("valid chat completions JSON")
    }

    /// Shape of a real Groq `openai/gpt-oss-20b` reply to a long Chinese
    /// dictation (#8524): the default 2,048-token cap was spent on reasoning.
    #[test]
    fn length_finish_with_empty_content_is_an_error() {
        let response = parse(
            r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"","reasoning":"We need to process the speech."},"finish_reason":"length"}],
                "usage":{"prompt_tokens":1424,"completion_tokens":2048,"completion_tokens_details":{"reasoning_tokens":2046}}}"#,
        );

        let err = response.complete_text().unwrap_err().to_string();

        assert!(err.contains("finish_reason=length"), "{err}");
        assert!(err.contains("2048"), "{err}");
        assert!(err.contains("2046"), "{err}");
    }

    /// When the cap lands mid-answer, the text stops mid-sentence.
    #[test]
    fn length_finish_with_partial_content_is_an_error() {
        let response = parse(
            r#"{"choices":[{"message":{"content":"The lower part can be the current one. The upper part should"},"finish_reason":"length"}]}"#,
        );

        assert!(response.complete_text().is_err());
    }

    #[test]
    fn stop_finish_returns_the_text() {
        let response = parse(
            r#"{"choices":[{"message":{"content":"我不确定容量有多少。"},"finish_reason":"stop"}]}"#,
        );

        assert_eq!(response.complete_text().unwrap(), "我不确定容量有多少。");
    }

    #[test]
    fn missing_finish_reason_returns_the_text() {
        let response = parse(r#"{"choices":[{"message":{"content":"Hello."}}]}"#);

        assert_eq!(response.complete_text().unwrap(), "Hello.");
    }
}
