use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    /// Detected language from Whisper `verbose_json` (OpenAI/Groq) and from
    /// Mistral, which always returns it.
    #[serde(default)]
    language: Option<String>,
    /// `gpt-transcribe` reports detection as `[{"code": "fr"}]`, or `[]`
    /// when it cannot decide. Kept as raw JSON: detection is optional
    /// metadata, so an unexpected shape must lose only the detection, never
    /// fail the whole transcription.
    #[serde(default)]
    languages: Option<serde_json::Value>,
}

impl TranscriptionResponse {
    fn detected_language(&self) -> Option<String> {
        let first = self.languages.as_ref().and_then(|l| l.get(0)).and_then(|entry| {
            entry
                .as_str()
                .or_else(|| entry.get("code").and_then(serde_json::Value::as_str))
        });
        normalize_provider_language(self.language.as_deref())
            .or_else(|| normalize_provider_language(first))
    }
}

/// Result of a cloud transcription: the text plus, when the provider supports
/// detection (e.g. OpenAI/Groq with verbose_json), the language code.
#[derive(Debug, Clone)]
pub struct CloudTranscription {
    pub text: String,
    pub detected_language: Option<String>,
}

/// What a transcription request may send besides audio: the free-text
/// `prompt` (conditioning + vocabulary, for Whisper-style models) and the
/// vocabulary as a list for providers with a dedicated biasing field
/// (OpenAI `keywords[]`, Mistral `context_bias`). The dictionary-echo retry
/// sends neither vocabulary form.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RequestHints {
    pub prompt: String,
    pub vocabulary: Vec<String>,
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
    // A complete protected phrase anywhere in the output may be spoken
    // vocabulary, including a pair of terms or words in language conditioning.
    // Prompt membership alone cannot distinguish that from a hallucinated echo.
    let text_tokens = tokenize(trimmed);
    let protected_tokens: Vec<Vec<String>> = protected_terms
        .iter()
        .map(|term| tokenize(term))
        .filter(|term| !term.is_empty())
        .collect();
    let contains_protected = |tokens: &[String]| {
        protected_tokens
            .iter()
            .any(|term| tokens.windows(term.len()).any(|span| span == term))
    };
    if is_dictionary_echo(trimmed, p) && !contains_protected(&text_tokens) {
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
        if contains_protected(&tokenize(&trimmed[..byte_offset])) {
            return trimmed.to_string();
        }
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

/// Vocabulary entries sent as a provider biasing list. Mistral documents a
/// maximum of 100 `context_bias` entries; OpenAI documents no limit.
const MAX_VOCABULARY: usize = 100;

/// Vocabulary safe to send as a list field: trimmed, de-duplicated
/// case-insensitively, capped at [`MAX_VOCABULARY`], and without entries
/// containing `<`, `>`, CR or LF — OpenAI rejects the whole request for any
/// of those characters in `keywords[]`.
fn sanitize_vocabulary(vocabulary: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    vocabulary
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty() && !v.contains(['<', '>', '\r', '\n']))
        .filter(|v| seen.insert(v.to_lowercase()))
        .take(MAX_VOCABULARY)
        .map(String::from)
        .collect()
}

/// Text fields for an OpenAI-compatible multipart transcription request
/// (OpenAI and Groq), by model family:
/// - Whisper (`whisper-1`, Groq `whisper-large-v3*`) is the only family that
///   accepts `verbose_json`, which carries the detected `language`.
/// - `gpt-transcribe` takes `languages[]` (never alongside `language`; the
///   provider's code-switching hint) and the vocabulary as `keywords[]`, and
///   reports detection in plain `json`.
/// - Other models take at most one `language`; a bilingual pair sends none so
///   the model detects within it.
/// - Diarization models do not accept `prompt`.
pub(crate) fn openai_form_fields(
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", model.to_string())];
    if model.starts_with("whisper") {
        fields.push(("response_format", "verbose_json".to_string()));
    }
    if model.starts_with("gpt-transcribe") {
        fields.extend(languages.iter().map(|l| ("languages[]", l.clone())));
        fields.extend(
            sanitize_vocabulary(&hints.vocabulary)
                .into_iter()
                .map(|k| ("keywords[]", k)),
        );
    } else if let [only] = languages {
        fields.push(("language", only.clone()));
    }
    if !hints.prompt.is_empty() && !model.contains("diarize") {
        fields.push(("prompt", hints.prompt.clone()));
    }
    fields
}

/// Text fields for Mistral's transcription endpoint. Its schema has no
/// `prompt` or `response_format`: vocabulary goes in `context_bias` (a
/// repeated field, as Mistral's own SDK encodes lists) and the detected
/// `language` is always returned. A bilingual pair sends no `language`.
pub(crate) fn mistral_form_fields(
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", model.to_string())];
    if let [only] = languages {
        fields.push(("language", only.clone()));
    }
    fields.extend(
        sanitize_vocabulary(&hints.vocabulary)
            .into_iter()
            .map(|t| ("context_bias", t)),
    );
    fields
}

/// POST a multipart transcription request (audio as `file` plus `fields`)
/// and parse the JSON response.
async fn post_transcription(
    url: &str,
    api_key: &str,
    audio_data: Vec<u8>,
    fields: Vec<(&'static str, String)>,
) -> Result<TranscriptionResponse> {
    let file_part = multipart::Part::bytes(audio_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = multipart::Form::new().part("file", file_part);
    for (name, value) in fields {
        form = form.text(name, value);
    }
    log::info!("[Whisperi] POST {}", url);
    let response = crate::http::HTTP_CLIENT
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;
    let response = crate::http::check_response(response, "Transcription API error").await?;
    Ok(response.json().await?)
}

/// Transcribe audio via an OpenAI-compatible `/audio/transcriptions`
/// endpoint (OpenAI, Groq). Which fields each model family receives lives in
/// [`openai_form_fields`].
pub async fn transcribe_openai(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    languages: &[String],
    hints: &RequestHints,
    base_url: Option<&str>,
) -> Result<CloudTranscription> {
    let url = format!(
        "{}/audio/transcriptions",
        base_url.unwrap_or("https://api.openai.com/v1")
    );
    let fields = openai_form_fields(model, languages, hints);
    let result = post_transcription(&url, api_key, audio_data, fields).await?;
    log_transcription_result("Cloud", &result.text, Some(hints.prompt.as_str()));
    let detected_language = result.detected_language();
    if let Some(ref code) = detected_language {
        log::info!("[Whisperi] Cloud detected language: {}", code);
    }
    Ok(CloudTranscription {
        text: result.text,
        detected_language,
    })
}

/// OpenAI's verbose_json returns full English names ("english", "chinese") for
/// some models and ISO 639-1 codes for others. Normalize to ISO codes since
/// that's what the rest of the pipeline expects.
fn normalize_provider_language(lang: Option<&str>) -> Option<String> {
    let raw = lang?.trim();
    let raw = raw.split(['-', '_']).next().unwrap_or(raw);
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
    languages: &[String],
    hints: &RequestHints,
) -> Result<CloudTranscription> {
    transcribe_openai(
        audio_data,
        api_key,
        model,
        languages,
        hints,
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
    languages: &[String],
    hints: &RequestHints,
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
    if let [only] = languages {
        instruction.push_str(&format!(" Output language: {}.", only));
    }
    if !hints.prompt.is_empty() {
        instruction.push_str(&format!(" Context/vocabulary hints: {}", hints.prompt));
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

    log_transcription_result("OpenRouter", &text, Some(hints.prompt.as_str()));
    Ok(CloudTranscription {
        text,
        detected_language: None,
    })
}

/// Transcribe audio via Mistral Voxtral (`/v1/audio/transcriptions`).
pub async fn transcribe_mistral(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Result<CloudTranscription> {
    let result = post_transcription(
        "https://api.mistral.ai/v1/audio/transcriptions",
        api_key,
        audio_data,
        mistral_form_fields(model, languages, hints),
    )
    .await?;
    log_transcription_result("Mistral", &result.text, None);
    Ok(CloudTranscription {
        detected_language: result.detected_language(),
        text: result.text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(prompt: &str, vocabulary: &[&str]) -> RequestHints {
        RequestHints {
            prompt: prompt.into(),
            vocabulary: vocabulary.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn only_whisper_models_request_verbose_json() {
        for model in ["whisper-1", "whisper-large-v3", "whisper-large-v3-turbo"] {
            let fields = openai_form_fields(model, &[], &hints("", &[]));
            assert!(fields.contains(&("response_format", "verbose_json".into())), "{model}");
        }
        for model in ["gpt-transcribe", "gpt-4o-mini-transcribe", "gpt-4o-transcribe"] {
            let fields = openai_form_fields(model, &[], &hints("", &[]));
            assert!(fields.iter().all(|(k, _)| *k != "response_format"), "{model}");
        }
    }

    #[test]
    fn gpt_transcribe_gets_language_list_and_keywords_never_singular_language() {
        let fields = openai_form_fields(
            "gpt-transcribe",
            &langs(&["en", "zh"]),
            &hints("prompt text", &["CLAUDE", "Tauri"]),
        );
        assert!(fields.contains(&("languages[]", "en".into())));
        assert!(fields.contains(&("languages[]", "zh".into())));
        assert!(fields.contains(&("keywords[]", "CLAUDE".into())));
        assert!(fields.contains(&("keywords[]", "Tauri".into())));
        assert!(fields.contains(&("prompt", "prompt text".into())));
        assert!(fields.iter().all(|(k, _)| *k != "language"));
    }

    #[test]
    fn legacy_models_send_one_language_and_none_for_a_pair() {
        let single = openai_form_fields("gpt-4o-mini-transcribe", &langs(&["zh"]), &hints("", &[]));
        assert!(single.contains(&("language", "zh".into())));
        let pair = openai_form_fields("whisper-large-v3", &langs(&["en", "zh"]), &hints("", &[]));
        assert!(pair.iter().all(|(k, _)| *k != "language" && *k != "languages[]"));
        assert!(pair.iter().all(|(k, _)| *k != "keywords[]"));
    }

    #[test]
    fn diarize_model_never_gets_a_prompt_and_empty_prompts_are_omitted() {
        let diarize = openai_form_fields("gpt-4o-transcribe-diarize", &[], &hints("ctx", &[]));
        assert!(diarize.iter().all(|(k, _)| *k != "prompt"));
        let empty = openai_form_fields("whisper-1", &[], &hints("", &[]));
        assert!(empty.iter().all(|(k, _)| *k != "prompt"));
    }

    #[test]
    fn vocabulary_drops_request_breaking_entries_and_duplicates() {
        let vocab: Vec<String> = ["CLAUDE", " claude ", "a<b", "x>y", "line\nbreak", "cr\rx", "", "Tauri"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(sanitize_vocabulary(&vocab), vec!["CLAUDE".to_string(), "Tauri".to_string()]);
        let many: Vec<String> = (0..150).map(|i| format!("term{i}")).collect();
        assert_eq!(sanitize_vocabulary(&many).len(), 100);
    }

    #[test]
    fn detected_language_reads_whisper_and_gpt_transcribe_shapes() {
        let whisper: TranscriptionResponse =
            serde_json::from_str(r#"{"text":"hi","language":"english"}"#).unwrap();
        assert_eq!(whisper.detected_language().as_deref(), Some("en"));
        let gpt: TranscriptionResponse =
            serde_json::from_str(r#"{"text":"hi","languages":[{"code":"zh-cn"}]}"#).unwrap();
        assert_eq!(gpt.detected_language().as_deref(), Some("zh"));
        for body in [r#"{"text":"hi","languages":[]}"#, r#"{"text":"hi","languages":null}"#, r#"{"text":"hi"}"#] {
            let r: TranscriptionResponse = serde_json::from_str(body).unwrap();
            assert_eq!(r.detected_language(), None, "{body}");
        }
    }

    #[test]
    fn mistral_gets_context_bias_and_never_prompt_or_response_format() {
        let fields = mistral_form_fields(
            "voxtral-mini-latest",
            &langs(&["zh"]),
            &hints("conditioning sentence CLAUDE", &["CLAUDE", "Tauri"]),
        );
        assert!(fields.contains(&("language", "zh".into())));
        assert!(fields.contains(&("context_bias", "CLAUDE".into())));
        assert!(fields.contains(&("context_bias", "Tauri".into())));
        assert!(fields.iter().all(|(k, _)| *k != "prompt" && *k != "response_format"));
        let pair = mistral_form_fields("voxtral-mini-latest", &langs(&["en", "zh"]), &hints("", &[]));
        assert!(pair.iter().all(|(k, _)| *k != "language"));
    }

    #[test]
    fn unexpected_detection_shapes_lose_detection_not_the_transcript() {
        // Detection is optional metadata; a surprise shape must never fail
        // the whole transcription.
        for (body, expected) in [
            (r#"{"text":"hi","languages":["zh"]}"#, Some("zh")),
            (r#"{"text":"hi","languages":[{"code":null}]}"#, None),
            (r#"{"text":"hi","languages":[{"lang":"zh"}]}"#, None),
            (r#"{"text":"hi","languages":{"code":"zh"}}"#, None),
            (r#"{"text":"hi","language":"","languages":[{"code":"zh"}]}"#, Some("zh")),
        ] {
            let r: TranscriptionResponse = serde_json::from_str(body).expect(body);
            assert_eq!(r.detected_language().as_deref(), expected, "{body}");
        }
    }

    #[test]
    fn normalize_provider_language_strips_region() {
        assert_eq!(normalize_provider_language(Some("zh-CN")), Some("zh".into()));
        assert_eq!(normalize_provider_language(Some("en-us")), Some("en".into()));
    }

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
            "Whisperi CLAUDE"
        );
        assert_eq!(
            strip_prompt_echo(
                "Whisperi CLAUDE this is real speech",
                Some("Whisperi CLAUDE"),
                &protected,
            ),
            "Whisperi CLAUDE this is real speech"
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
