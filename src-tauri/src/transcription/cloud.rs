use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Split text into lowercase word tokens (letters + numbers only).
fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Check if every word in `text` appears in `prompt` (dictionary echo detection).
fn is_dictionary_echo(text: &str, prompt: &str) -> bool {
    let text_words = tokenize(text);
    if text_words.is_empty() {
        return true;
    }
    let prompt_words: std::collections::HashSet<String> = tokenize(prompt).into_iter().collect();
    text_words.iter().all(|w| prompt_words.contains(w))
}

/// Log transcription result, detecting silence (empty or dictionary echo).
pub fn log_transcription_result(provider: &str, text: &str, prompt: Option<&str>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        log::warn!("[Whisperi] {} transcription: empty (no voice detected)", provider);
    } else if let Some(p) = prompt {
        if !p.is_empty() && is_dictionary_echo(trimmed, p) {
            log::warn!(
                "[Whisperi] {} transcription: \"{}\" (dictionary echo, no voice detected)",
                provider, trimmed
            );
        } else {
            log::info!("[Whisperi] {} transcription: \"{}\" ({} chars)", provider, trimmed, trimmed.len());
        }
    } else {
        log::info!("[Whisperi] {} transcription: \"{}\" ({} chars)", provider, trimmed, trimmed.len());
    }
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

    log::info!("[Whisperi] POST {}", url);
    let response = crate::http::HTTP_CLIENT
        .post(&url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;

    let response = crate::http::check_response(response, "Transcription API error").await?;

    let result: TranscriptionResponse = response.json().await?;
    log_transcription_result("Cloud", &result.text, prompt);
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

    log::info!("[Whisperi] POST https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions");
    let response = crate::http::HTTP_CLIENT
        .post("https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await?;

    let response = crate::http::check_response(response, "Qwen ASR API error").await?;

    let result: crate::http::ChatCompletionsResponse = response.json().await?;
    let text = result.text();

    log_transcription_result("Qwen", &text, None);
    Ok(text)
}

// --- OpenRouter multimodal types (chat completions with audio) ---

#[derive(Serialize)]
struct OpenRouterAsrRequest {
    model: String,
    modalities: Vec<String>,
    messages: Vec<OpenRouterAsrMessage>,
}

#[derive(Serialize)]
struct OpenRouterAsrMessage {
    role: String,
    content: Vec<serde_json::Value>,
}

/// Transcribe audio via OpenRouter multimodal chat completions
pub async fn transcribe_openrouter(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<String> {
    log::info!(
        "[Whisperi] OpenRouter transcription: model={}, audio={} bytes ({:.1} KB base64)",
        model,
        audio_data.len(),
        audio_data.len() as f64 * 4.0 / 3.0 / 1024.0
    );
    let b64 = BASE64.encode(&audio_data);

    let mut instruction =
        String::from("Transcribe this audio. Output only the transcribed text, nothing else.");
    if let Some(lang) = language {
        if lang != "auto" {
            instruction.push_str(&format!(" Output language: {}.", lang));
        }
    }
    if let Some(p) = prompt {
        if !p.is_empty() {
            instruction.push_str(&format!(" Context/vocabulary hints: {}", p));
        }
    }

    let content = vec![
        serde_json::json!({ "type": "text", "text": instruction }),
        serde_json::json!({
            "type": "input_audio",
            "input_audio": { "data": b64, "format": "wav" }
        }),
    ];

    let request = OpenRouterAsrRequest {
        model: model.to_string(),
        modalities: vec!["text".to_string()],
        messages: vec![OpenRouterAsrMessage {
            role: "user".to_string(),
            content,
        }],
    };

    log::info!("[Whisperi] POST https://openrouter.ai/api/v1/chat/completions (transcription)");
    let response = crate::http::HTTP_CLIENT
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(api_key)
        .header("HTTP-Referer", "https://github.com/xarthurx/whisperi")
        .header("X-Title", "Whisperi")
        .json(&request)
        .send()
        .await?;

    let response = crate::http::check_response(response, "OpenRouter transcription API error").await?;

    let result: crate::http::ChatCompletionsResponse = response.json().await?;
    let text = result.text();

    log_transcription_result("OpenRouter", &text, prompt);
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
