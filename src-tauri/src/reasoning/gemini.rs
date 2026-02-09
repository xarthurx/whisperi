use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    system_instruction: SystemInstruction,
    contents: Vec<Content>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    max_output_tokens: u32,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

pub async fn complete(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_text: &str,
    max_tokens: Option<u32>,
) -> Result<String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request = GeminiRequest {
        system_instruction: SystemInstruction {
            parts: vec![Part {
                text: system_prompt.to_string(),
            }],
        },
        contents: vec![Content {
            role: Some("user".to_string()),
            parts: vec![Part {
                text: user_text.to_string(),
            }],
        }],
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens.unwrap_or(2048),
        },
    };

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Gemini API error ({}): {}", status, body);
    }

    let result: GeminiResponse = response.json().await?;

    let text = result
        .candidates
        .first()
        .map(|c| {
            c.content
                .parts
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    Ok(text)
}
