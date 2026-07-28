use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    /// Detected language code, returned by OpenAI/Groq when
    /// `response_format=verbose_json` is requested. Absent for chat-completion
    /// transcription paths (Qwen, OpenRouter).
    #[serde(default)]
    language: Option<String>,
}

/// Result of a cloud transcription: the text plus, when the provider supports
/// detection (e.g. OpenAI/Groq with verbose_json), the language code.
#[derive(Debug, Clone)]
pub struct CloudTranscription {
    pub text: String,
    pub detected_language: Option<String>,
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
pub fn strip_prompt_echo(
    text: &str,
    prompt: Option<&str>,
    protected_terms: &[String],
) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Some(p) = prompt.filter(|s| !s.is_empty()) else {
        return trimmed.to_string();
    };

    // Full echo: every word in text comes from the prompt → probable silence.
    // Never discard a protected canonical term, though: it may be exactly what
    // the user spoke, and prompt membership alone cannot distinguish that from
    // a hallucinated echo.
    let text_tokens = tokenize(trimmed);
    let exactly_protected = protected_terms
        .iter()
        .any(|term| tokenize(term) == text_tokens);
    if is_dictionary_echo(trimmed, p) && !exactly_protected {
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

/// Strip a contiguous run of pure-dictionary words at the leading and/or
/// trailing edge of `text`.
///
/// Whisper hallucinates ("echoes") the hotword dictionary from its `prompt` on
/// silence / very short audio, leaking dictionary terms that were never spoken
/// into the output. [`strip_prompt_echo`] only removes the cases where the
/// *entire* output is an echo (full echo, or the whole prompt echoed as a
/// prefix); this handles the common real-world case of a few echoed dictionary
/// words sitting next to genuine speech — including a leaked ASCII term in front
/// of CJK speech, which `strip_prompt_echo`'s whitespace tokenisation can't see.
///
/// Conservative by design:
/// - Only the leading and trailing edges are trimmed — an interior dictionary
///   word is far more likely to be genuine speech and is never touched.
/// - `keep_terms` (the agent name + aliases) are NEVER stripped, so a genuinely
///   spoken agent invocation survives for chat-mode detection.
/// - At least one non-dictionary word must remain; an all-dictionary output is
///   left unchanged here (it's silence, owned by the full-echo / empty guards).
pub fn strip_dictionary_edge_echo(
    text: &str,
    dictionary: &[String],
    keep_terms: &[String],
) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Words that may be stripped: dictionary tokens minus the keep terms.
    let keep: std::collections::HashSet<String> =
        keep_terms.iter().flat_map(|t| tokenize(t)).collect();
    let strippable: std::collections::HashSet<String> = dictionary
        .iter()
        .flat_map(|d| tokenize(d))
        .filter(|w| !keep.contains(w))
        .collect();
    if strippable.is_empty() {
        return trimmed.to_string();
    }

    // A word is an echo candidate iff every alphanumeric token in it is a
    // strippable dictionary token (so "Whisperi" and "Whisperi." both match, but
    // a word that also carries non-dictionary content does not).
    let is_echo = |w: &str| -> bool {
        let toks = tokenize(w);
        !toks.is_empty() && toks.iter().all(|t| strippable.contains(t))
    };

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let n = words.len();

    let mut start = 0;
    while start < n && is_echo(words[start]) {
        start += 1;
    }
    let mut end = n;
    while end > start && is_echo(words[end - 1]) {
        end -= 1;
    }

    // Nothing trimmed, or the whole output is dictionary words (silence — leave
    // it to the full-echo / empty guards): return unchanged.
    if (start == 0 && end == n) || start >= end {
        return trimmed.to_string();
    }

    // words[start] and words[end - 1] are guaranteed non-echo (real speech).
    let first = words[start];
    let last = words[end - 1];
    let begin = first.as_ptr() as usize - trimmed.as_ptr() as usize;
    let finish = last.as_ptr() as usize - trimmed.as_ptr() as usize + last.len();
    let result = trimmed[begin..finish].trim();
    if result.is_empty() {
        return trimmed.to_string();
    }
    log::info!(
        "[Whisperi] Stripped dictionary edge echo (leading: \"{}\", trailing: \"{}\")",
        trimmed[..begin].trim(),
        trimmed[finish..].trim()
    );
    result.to_string()
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
///
/// Uses `response_format=verbose_json` so the response includes the
/// detected language. The `gpt-4o-*-transcribe` models reject verbose_json
/// (they only support `json` or `text`), so we drop the parameter for them
/// and accept that detection is unavailable on that path.
pub async fn transcribe_openai(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
    base_url: Option<&str>,
) -> Result<CloudTranscription> {
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

    // verbose_json gives us `language` (and `duration`/`segments` which we
    // ignore). Only `whisper-1` and Groq's whisper variants support it; the
    // newer GPT-4o transcription models do not.
    let supports_verbose = !model.starts_with("gpt-4o");
    if supports_verbose {
        form = form.text("response_format", "verbose_json".to_string());
    }

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
    if let Some(ref code) = result.language {
        log::info!("[Whisperi] Cloud detected language: {}", code);
    }
    Ok(CloudTranscription {
        text: result.text,
        detected_language: normalize_provider_language(result.language.as_deref()),
    })
}

/// OpenAI's verbose_json returns full English names ("english", "chinese") for
/// some models and ISO 639-1 codes for others. Normalize to ISO codes since
/// that's what the rest of the pipeline expects.
fn normalize_provider_language(lang: Option<&str>) -> Option<String> {
    let raw = lang?.trim();
    if raw.is_empty() {
        return None;
    }
    // If it already looks like a 2-3 letter code, keep it.
    if raw.len() <= 3 && raw.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some(raw.to_lowercase());
    }
    // Common full-name → code mappings produced by OpenAI's verbose_json.
    let code = match raw.to_lowercase().as_str() {
        "chinese" | "mandarin" => "zh",
        "english" => "en",
        "japanese" => "ja",
        "korean" => "ko",
        "french" => "fr",
        "german" => "de",
        "spanish" => "es",
        "portuguese" => "pt",
        "russian" => "ru",
        "italian" => "it",
        "dutch" => "nl",
        "polish" => "pl",
        "turkish" => "tr",
        "arabic" => "ar",
        "hindi" => "hi",
        "vietnamese" => "vi",
        "thai" => "th",
        "indonesian" => "id",
        "ukrainian" => "uk",
        _ => return Some(raw.to_lowercase()),
    };
    Some(code.to_string())
}

/// Transcribe audio via Groq Whisper API
pub async fn transcribe_groq(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<CloudTranscription> {
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
) -> Result<CloudTranscription> {
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
    Ok(CloudTranscription {
        text,
        detected_language: None,
    })
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
) -> Result<CloudTranscription> {
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
    Ok(CloudTranscription {
        text,
        detected_language: None,
    })
}

/// Transcribe audio via Mistral Voxtral API
pub async fn transcribe_mistral(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Result<CloudTranscription> {
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
    fn normalize_provider_language_iso_codes_pass_through() {
        assert_eq!(normalize_provider_language(Some("zh")), Some("zh".into()));
        assert_eq!(normalize_provider_language(Some("EN")), Some("en".into()));
        assert_eq!(normalize_provider_language(Some("ja")), Some("ja".into()));
    }

    #[test]
    fn normalize_provider_language_full_names_to_codes() {
        assert_eq!(normalize_provider_language(Some("chinese")), Some("zh".into()));
        assert_eq!(normalize_provider_language(Some("Chinese")), Some("zh".into()));
        assert_eq!(normalize_provider_language(Some("English")), Some("en".into()));
        assert_eq!(normalize_provider_language(Some("Japanese")), Some("ja".into()));
        assert_eq!(normalize_provider_language(Some("mandarin")), Some("zh".into()));
    }

    #[test]
    fn normalize_provider_language_unknown_full_name_falls_through() {
        // Unknown full name → lowercased verbatim (caller's heuristic still works
        // for the Chinese case since downstream gating doesn't recognise it).
        assert_eq!(
            normalize_provider_language(Some("Klingon")),
            Some("klingon".into())
        );
    }

    #[test]
    fn normalize_provider_language_none_and_empty() {
        assert_eq!(normalize_provider_language(None), None);
        assert_eq!(normalize_provider_language(Some("")), None);
        assert_eq!(normalize_provider_language(Some("   ")), None);
    }

    #[test]
    fn strip_prompt_echo_no_prompt() {
        assert_eq!(strip_prompt_echo("Hello world", None, &[]), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_empty_prompt() {
        assert_eq!(
            strip_prompt_echo("Hello world", Some(""), &[]),
            "Hello world"
        );
    }

    #[test]
    fn strip_prompt_echo_full_echo_detected() {
        // All words in text come from dictionary → silence
        assert_eq!(
            strip_prompt_echo("Whisperi Tauri", Some("Whisperi Tauri"), &[]),
            ""
        );
    }

    #[test]
    fn strip_prompt_echo_prefix_stripped() {
        // Text starts with all dictionary words in sequence → strip prefix
        assert_eq!(
            strip_prompt_echo(
                "Whisperi Tauri this is real speech",
                Some("Whisperi Tauri"),
                &[]
            ),
            "this is real speech"
        );
    }

    #[test]
    fn strip_prompt_echo_no_false_positive_on_common_words() {
        // Text contains common words that happen to be in conditioning text
        // but dictionary is different — should NOT be stripped
        assert_eq!(
            strip_prompt_echo("Hello, how are you today?", Some("Whisperi Tauri"), &[]),
            "Hello, how are you today?"
        );
    }

    #[test]
    fn strip_prompt_echo_single_word_dict_not_prefix_stripped() {
        // Single-word dictionary: prefix stripping disabled (too ambiguous)
        assert_eq!(
            strip_prompt_echo("Whisperi is great", Some("Whisperi"), &[]),
            "Whisperi is great"
        );
    }

    #[test]
    fn strip_prompt_echo_partial_prefix_not_stripped() {
        // Only some dictionary words match at start — don't strip
        assert_eq!(
            strip_prompt_echo(
                "Whisperi is great software by Tauri",
                Some("Whisperi Tauri"),
                &[]
            ),
            "Whisperi is great software by Tauri"
        );
    }

    #[test]
    fn strip_prompt_echo_trims_whitespace() {
        assert_eq!(
            strip_prompt_echo("  Hello world  ", None, &[]),
            "Hello world"
        );
    }

    #[test]
    fn strip_prompt_echo_empty_text() {
        assert_eq!(strip_prompt_echo("", Some("Whisperi"), &[]), "");
        assert_eq!(strip_prompt_echo("   ", Some("Whisperi"), &[]), "");
    }

    #[test]
    fn strip_prompt_echo_conditioning_echo_detected() {
        // When whisper echoes conditioning text during silence and the full
        // conditioning + dictionary prompt is passed, it should be detected
        let cond = crate::transcription::PUNCTUATION_PROMPT;
        let full_prompt = format!("{} Whisperi Tauri", cond);
        // Conditioning echo (all words from conditioning text) → silence
        assert_eq!(strip_prompt_echo(cond, Some(&full_prompt), &[]), "");
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
                &[],
            ),
            "Hello, I wanted to discuss the project timeline today."
        );
    }

    #[test]
    fn strip_prompt_echo_preserves_protected_dictionary_only_speech() {
        let protected = vec!["CLAUDE".to_string()];

        assert_eq!(
            strip_prompt_echo("CLAUDE", Some("Whisperi CLAUDE"), &protected),
            "CLAUDE"
        );
        assert_eq!(
            strip_prompt_echo(
                "Whisperi CLAUDE",
                Some("Whisperi CLAUDE"),
                &protected
            ),
            ""
        );
    }

    // --- strip_dictionary_edge_echo ---

    #[test]
    fn edge_echo_strips_leading_single_dict_word() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi can you summarize this", &dict, &[]),
            "can you summarize this"
        );
    }

    #[test]
    fn edge_echo_strips_trailing_dict_word() {
        let dict = vec!["Tauri".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("let us wrap up the demo Tauri", &dict, &[]),
            "let us wrap up the demo"
        );
    }

    #[test]
    fn edge_echo_strips_both_edges() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi the build passed Tauri", &dict, &[]),
            "the build passed"
        );
    }

    #[test]
    fn edge_echo_strips_multiple_leading_dict_words() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi Tauri the meeting is at three", &dict, &[]),
            "the meeting is at three"
        );
    }

    #[test]
    fn edge_echo_keeps_interior_dict_word() {
        // An interior dictionary word is likely genuine speech — never stripped.
        let dict = vec!["Whisperi".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("please open the Whisperi settings", &dict, &[]),
            "please open the Whisperi settings"
        );
    }

    #[test]
    fn edge_echo_never_strips_agent_name_at_edge() {
        // The agent name is a dictionary word but must survive at the edge so
        // chat-mode detection still fires; a non-agent dict word at the other
        // edge is still stripped.
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        let keep = vec!["Whisperi".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi the build passed Tauri", &dict, &keep),
            "Whisperi the build passed"
        );
    }

    #[test]
    fn edge_echo_all_dictionary_left_unchanged() {
        // Whole output is dictionary words → silence; left for the full-echo /
        // empty guards, not this edge stripper.
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi Tauri", &dict, &[]),
            "Whisperi Tauri"
        );
    }

    #[test]
    fn edge_echo_no_dict_words_unchanged() {
        let dict = vec!["Whisperi".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("hello world this is real", &dict, &[]),
            "hello world this is real"
        );
    }

    #[test]
    fn edge_echo_empty_dictionary_unchanged() {
        assert_eq!(
            strip_dictionary_edge_echo("anything goes here", &[], &[]),
            "anything goes here"
        );
    }

    #[test]
    fn edge_echo_strips_dict_word_with_trailing_punctuation() {
        let dict = vec!["Whisperi".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi. this is the real text", &dict, &[]),
            "this is the real text"
        );
    }

    #[test]
    fn edge_echo_cjk_leading_english_dict_word() {
        // The reported case: an echoed English dict word in front of Chinese
        // speech, which strip_prompt_echo's tokeniser cannot catch.
        let dict = vec!["Whisperi".to_string()];
        assert_eq!(
            strip_dictionary_edge_echo("Whisperi 我今天去商店买牛奶", &dict, &[]),
            "我今天去商店买牛奶"
        );
    }

    #[test]
    fn edge_echo_empty_text() {
        let dict = vec!["Whisperi".to_string()];
        assert_eq!(strip_dictionary_edge_echo("   ", &dict, &[]), "");
    }
}
