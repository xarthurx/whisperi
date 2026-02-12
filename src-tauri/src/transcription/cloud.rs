use anyhow::Result;
use reqwest::multipart;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Transcribe audio via OpenAI Whisper API
pub async fn transcribe_openai(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
    base_url: Option<&str>,
) -> Result<String> {
    let url = format!(
        "{}/audio/transcriptions",
        base_url.unwrap_or("https://api.openai.com/v1")
    );

    let file_part = multipart::Part::bytes(audio_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let mut form = multipart::Form::new()
        .text("model", model.to_string())
        .part("file", file_part);

    if let Some(lang) = language {
        if lang != "auto" {
            form = form.text("language", lang.to_string());
        }
    }

    if let Some(p) = prompt {
        if !p.is_empty() {
            form = form.text("prompt", p.to_string());
        }
    }

    let response = crate::HTTP_CLIENT
        .post(&url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Transcription API error ({}): {}", status, body);
    }

    let result: TranscriptionResponse = response.json().await?;
    Ok(result.text)
}

/// Transcribe audio via Groq Whisper API
pub async fn transcribe_groq(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<String> {
    transcribe_openai(
        audio_data,
        api_key,
        model,
        language,
        prompt,
        Some("https://api.groq.com/openai/v1"),
    )
    .await
}

/// Transcribe audio via Qwen (DashScope) API
pub async fn transcribe_qwen(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<String> {
    transcribe_openai(
        audio_data,
        api_key,
        model,
        language,
        prompt,
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
    )
    .await
}

/// Transcribe audio via Mistral Voxtral API
pub async fn transcribe_mistral(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<String> {
    transcribe_openai(
        audio_data,
        api_key,
        model,
        language,
        prompt,
        Some("https://api.mistral.ai/v1"),
    )
    .await
}
