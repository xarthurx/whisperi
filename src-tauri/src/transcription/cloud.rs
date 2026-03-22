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

/// Strip echoed prompt/dictionary words from transcription output.
///
/// Whisper models (local and cloud) may echo the prompt at the beginning of their
/// output, especially when audio starts with silence or is very short.
///
/// - Full echo (all words from prompt): return empty string (silence detected)
/// - Prefix echo (text starts with prompt word sequence): strip the prefix
pub fn strip_prompt_echo(text: &str, prompt: Option<&str>) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Some(p) = prompt.filter(|s| !s.is_empty()) else {
        return trimmed.to_string();
    };

    // Full echo: every word in text comes from the prompt → silence
    if is_dictionary_echo(trimmed, p) {
        log::warn!(
            "[Whisperi] Stripped dictionary echo (silence): \"{}\"",
            trimmed
        );
        return String::new();
    }

    // Prefix echo: check if text starts with the prompt words in sequence.
    // Only applies when prompt has 2+ words (single-word matches are too ambiguous).
    let prompt_words: Vec<&str> = p.split_whitespace().collect();
    if prompt_words.len() < 2 {
        return trimmed.to_string();
    }

    let text_words: Vec<&str> = trimmed.split_whitespace().collect();
    if text_words.len() <= prompt_words.len() {
        // Not enough words for prefix echo + real speech
        return trimmed.to_string();
    }

    // Count consecutive matching prompt words at the start of text
    let mut match_count = 0;
    for (tw, pw) in text_words.iter().zip(prompt_words.iter()) {
        let tw_clean = tw.trim_matches(|c: char| !c.is_alphanumeric());
        if tw_clean.eq_ignore_ascii_case(pw) {
            match_count += 1;
        } else {
            break;
        }
    }

    // Require ALL prompt words to match at the start (conservative to avoid
    // stripping legitimate speech that happens to start with dictionary words)
    if match_count == prompt_words.len() {
        // Use pointer arithmetic on the slice to find the byte offset safely.
        // text_words are &str slices into trimmed, so this is always valid UTF-8.
        let last = text_words[match_count - 1];
        let byte_offset = last.as_ptr() as usize - trimmed.as_ptr() as usize + last.len();
        let stripped = trimmed[byte_offset..].trim();
        if !stripped.is_empty() {
            log::info!(
                "[Whisperi] Stripped prompt echo prefix: \"{}\"",
                trimmed[..byte_offset].trim()
            );
            return stripped.to_string();
        }
    }

    trimmed.to_string()
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

    let mut instruction = String::from(
        "Transcribe this audio with proper punctuation and capitalization. \
         Output only the transcribed text, nothing else.",
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_prompt_echo_no_prompt() {
        assert_eq!(strip_prompt_echo("Hello world", None), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_empty_prompt() {
        assert_eq!(strip_prompt_echo("Hello world", Some("")), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_full_echo_detected() {
        // All words in text come from dictionary → silence
        assert_eq!(strip_prompt_echo("Whisperi Tauri", Some("Whisperi Tauri")), "");
    }

    #[test]
    fn strip_prompt_echo_prefix_stripped() {
        // Text starts with all dictionary words in sequence → strip prefix
        assert_eq!(
            strip_prompt_echo("Whisperi Tauri this is real speech", Some("Whisperi Tauri")),
            "this is real speech"
        );
    }

    #[test]
    fn strip_prompt_echo_no_false_positive_on_common_words() {
        // Text contains common words that happen to be in conditioning text
        // but dictionary is different — should NOT be stripped
        assert_eq!(
            strip_prompt_echo("Hello, how are you today?", Some("Whisperi Tauri")),
            "Hello, how are you today?"
        );
    }

    #[test]
    fn strip_prompt_echo_single_word_dict_not_prefix_stripped() {
        // Single-word dictionary: prefix stripping disabled (too ambiguous)
        assert_eq!(
            strip_prompt_echo("Whisperi is great", Some("Whisperi")),
            "Whisperi is great"
        );
    }

    #[test]
    fn strip_prompt_echo_partial_prefix_not_stripped() {
        // Only some dictionary words match at start — don't strip
        assert_eq!(
            strip_prompt_echo("Whisperi is great software by Tauri", Some("Whisperi Tauri")),
            "Whisperi is great software by Tauri"
        );
    }

    #[test]
    fn strip_prompt_echo_trims_whitespace() {
        assert_eq!(strip_prompt_echo("  Hello world  ", None), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_empty_text() {
        assert_eq!(strip_prompt_echo("", Some("Whisperi")), "");
        assert_eq!(strip_prompt_echo("   ", Some("Whisperi")), "");
    }

    #[test]
    fn strip_prompt_echo_conditioning_echo_detected() {
        // When whisper echoes conditioning text during silence and the full
        // conditioning + dictionary prompt is passed, it should be detected
        let cond = crate::transcription::PUNCTUATION_PROMPT;
        let full_prompt = format!("{} Whisperi Tauri", cond);
        // Conditioning echo (all words from conditioning text) → silence
        assert_eq!(strip_prompt_echo(cond, Some(&full_prompt)), "");
    }

    #[test]
    fn strip_prompt_echo_real_speech_not_false_positive() {
        // Real speech with some words overlapping conditioning text should
        // NOT be detected as echo (not all words are in the prompt set)
        let cond = crate::transcription::PUNCTUATION_PROMPT;
        let full_prompt = format!("{} Whisperi Tauri", cond);
        assert_eq!(
            strip_prompt_echo(
                "Hello, I wanted to discuss the project timeline today.",
                Some(&full_prompt),
            ),
            "Hello, I wanted to discuss the project timeline today."
        );
    }
}
