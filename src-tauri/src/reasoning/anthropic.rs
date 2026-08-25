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
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<OutputConfig>,
}

#[derive(Serialize)]
struct OutputConfig {
    effort: &'static str,
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

/// Claude Opus 5 / Sonnet 5 and the Opus 4.7+ family removed the sampling
/// parameters (`temperature`, `top_p`, `top_k`) from the Messages API —
/// sending one returns a 400. They also always reason, so we pin the cheapest
/// effort level: dictation cleanup does not need deep thinking, and thinking
/// tokens count against `max_tokens`.
fn is_adaptive_thinking_model(model: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "claude-opus-5",
        "claude-sonnet-5",
        "claude-fable-5",
        "claude-mythos-5",
        "claude-opus-4-8",
        "claude-opus-4-7",
    ];
    PREFIXES.iter().any(|p| model.starts_with(p))
}

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
) -> Result<String> {
    let adaptive = is_adaptive_thinking_model(model);

    let request = MessagesRequest {
        model: model.to_string(),
        max_tokens: max_tokens.unwrap_or(4096),
        system: system_prompt.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: user_text.to_string(),
        }],
        temperature: if adaptive { None } else { temperature },
        output_config: adaptive.then_some(OutputConfig { effort: "low" }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_5_family_rejects_sampling_params() {
        assert!(is_adaptive_thinking_model("claude-opus-5"));
        assert!(is_adaptive_thinking_model("claude-sonnet-5"));
        assert!(is_adaptive_thinking_model("claude-fable-5"));
        assert!(is_adaptive_thinking_model("claude-opus-4-8"));
    }

    #[test]
    fn older_models_still_accept_temperature() {
        assert!(!is_adaptive_thinking_model("claude-haiku-4-5"));
        assert!(!is_adaptive_thinking_model("claude-sonnet-4-6"));
        assert!(!is_adaptive_thinking_model("claude-opus-4-6"));
    }

    #[test]
    fn adaptive_request_omits_temperature_and_sets_effort() {
        let body = serde_json::to_value(MessagesRequest {
            model: "claude-opus-5".to_string(),
            max_tokens: 4096,
            system: "s".to_string(),
            messages: vec![],
            temperature: None,
            output_config: Some(OutputConfig { effort: "low" }),
        })
        .unwrap();
        assert!(body.get("temperature").is_none());
        assert_eq!(body["output_config"]["effort"], "low");
    }

    #[test]
    fn legacy_request_omits_output_config() {
        let body = serde_json::to_value(MessagesRequest {
            model: "claude-haiku-4-5".to_string(),
            max_tokens: 4096,
            system: "s".to_string(),
            messages: vec![],
            temperature: Some(0.3),
            output_config: None,
        })
        .unwrap();
        assert!(body.get("output_config").is_none());
        assert_eq!(body["temperature"], 0.3);
    }
}
