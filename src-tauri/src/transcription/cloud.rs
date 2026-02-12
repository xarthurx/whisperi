use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

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

// --- Qwen ASR types (multimodal chat completions) ---

#[derive(Serialize)]
struct QwenAsrRequest {
    model: String,
    messages: Vec<QwenAsrMessage>,
    stream: bool,
}

#[derive(Serialize)]
struct QwenAsrMessage {
    role: String,
    content: Vec<QwenAsrContent>,
}

#[derive(Serialize)]
struct QwenAsrContent {
    #[serde(rename = "type")]
    content_type: String,
    input_audio: QwenAsrAudio,
}

#[derive(Serialize)]
struct QwenAsrAudio {
    data: String,
}

/// Transcribe audio via Qwen ASR (DashScope multimodal chat completions)
pub async fn transcribe_qwen(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
) -> Result<String> {
    let b64 = BASE64.encode(&audio_data);
    let data_url = format!("data:audio/wav;base64,{}", b64);

    let request = QwenAsrRequest {
        model: model.to_string(),
        messages: vec![QwenAsrMessage {
            role: "user".to_string(),
            content: vec![QwenAsrContent {
                content_type: "input_audio".to_string(),
                input_audio: QwenAsrAudio { data: data_url },
            }],
        }],
        stream: false,
    };

    let response = crate::HTTP_CLIENT
        .post("https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Qwen ASR API error ({}): {}", status, body);
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<ChatChoice>,
    }
    #[derive(Deserialize)]
    struct ChatChoice {
        message: ChatMsg,
    }
    #[derive(Deserialize)]
    struct ChatMsg {
        content: Option<String>,
    }

    let result: ChatResponse = response.json().await?;

    let text = result
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();

    Ok(text)
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
