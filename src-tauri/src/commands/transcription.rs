use super::ResultExt;
use crate::transcription;
use serde::Serialize;

/// Normalize locale codes like "en-US" to ISO 639-1 "en" for transcription APIs.
fn normalize_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.split('-').next().unwrap_or(&l).to_string())
}

/// Result of a transcription Tauri command. `text` is the post-processed
/// (Simplified Chinese, full-width punctuation) output. `detected_language` is
/// the resolved language output by `resolve_language` — in bilingual mode it's
/// the snapped-to-pair language, in single/auto mode it's the chosen/detected
/// language. The frontend forwards this into the subsequent reasoning call so
/// AI enhancement runs with the resolved language instead of "auto".
#[derive(Debug, Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub detected_language: Option<String>,
}

/// Resolve the effective language used for post-processing (T→S) and reported
/// back to the frontend (which forwards it to AI enhancement).
///
/// - **Bilingual** (`secondary` is `Some`): keep the detected language when it
///   is one of the two chosen languages; a third-language detection or no
///   detection snaps to `primary`.
/// - **Auto / Single** (`secondary` is `None`): unchanged — auto/empty prefers
///   the detected language, an explicit language wins outright.
fn resolve_language(
    primary: Option<&str>,
    secondary: Option<&str>,
    detected: Option<&str>,
) -> Option<String> {
    match secondary {
        Some(sec) => {
            let p = primary.unwrap_or("");
            match detected {
                Some(d) if d == p || d == sec => Some(d.to_string()),
                _ => primary.map(str::to_string),
            }
        }
        None => match primary {
            None | Some("auto") => detected.map(str::to_string),
            Some(other) => Some(other.to_string()),
        },
    }
}

/// Per-request language parameters shared by both buffered transcription
/// commands: the normalized primary/secondary codes, the conditioning prompt
/// (bilingual when both are set, else single/auto), and the `engine_lang` handed
/// to the model — `None` in bilingual mode so it auto-detects within the pair,
/// otherwise the user's choice.
fn prepare_language_params(
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: &[String],
) -> (Option<String>, Option<String>, String, Option<String>) {
    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);
    // Bilingual: prime BOTH languages and let the model detect within the pair
    // (no forced -l). Otherwise keep the single/auto prompt.
    let prompt = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => crate::transcription::build_bilingual_prompt(dictionary, p, s),
        _ => crate::transcription::build_prompt(dictionary, primary.as_deref()),
    };
    // In bilingual mode never force a language; auto/single pass the choice through.
    let engine_lang = match secondary.as_deref() {
        Some(_) => None,
        None => primary.clone(),
    };
    (primary, secondary, prompt, engine_lang)
}

#[tauri::command]
pub async fn transcribe_cloud(
    audio_data: Vec<u8>,
    provider: String,
    api_key: String,
    model: String,
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: Vec<String>,
    agent_terms: Vec<String>,
) -> Result<TranscriptionResult, String> {
    // Never log any part of the API key — even a 4-char prefix/suffix can leak
    // into shipped logs or bug reports. Log only whether a key is present.
    log::info!(
        "[Whisperi] Transcribing: provider={}, model={}, has_key={}",
        provider,
        model,
        !api_key.is_empty()
    );

    let (primary, secondary, prompt, engine_lang) =
        prepare_language_params(language, secondary_language, &dictionary);

    let output = match provider.as_str() {
        "openai" => transcription::cloud::transcribe_openai(
            audio_data,
            &api_key,
            &model,
            engine_lang.as_deref(),
            Some(prompt.as_str()),
            None,
        )
        .await
        .str_err()?,

        "groq" => transcription::cloud::transcribe_groq(
            audio_data,
            &api_key,
            &model,
            engine_lang.as_deref(),
            Some(prompt.as_str()),
        )
        .await
        .str_err()?,

        "qwen" => transcription::cloud::transcribe_qwen(audio_data, &api_key, &model)
            .await
            .str_err()?,

        "mistral" => transcription::cloud::transcribe_mistral(
            audio_data,
            &api_key,
            &model,
            engine_lang.as_deref(),
            Some(prompt.as_str()),
        )
        .await
        .str_err()?,

        "openrouter" => transcription::cloud::transcribe_openrouter(
            audio_data,
            &api_key,
            &model,
            engine_lang.as_deref(),
            Some(prompt.as_str()),
        )
        .await
        .str_err()?,

        other => return Err(format!("Unknown transcription provider: {}", other)),
    };

    let stripped = transcription::cloud::strip_prompt_echo(&output.text, Some(prompt.as_str()));
    let stripped =
        transcription::cloud::strip_dictionary_edge_echo(&stripped, &dictionary, &agent_terms);
    let resolved = resolve_language(
        primary.as_deref(),
        secondary.as_deref(),
        output.detected_language.as_deref(),
    );
    let text = transcription::finalize_chinese_text(&stripped, resolved.as_deref());
    Ok(TranscriptionResult {
        text,
        detected_language: resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_keeps_in_set_detection() {
        // Bilingual: a detected language inside {primary, secondary} is kept.
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("en")),
            Some("en".to_string())
        );
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("zh")),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_snaps_out_of_set_to_primary() {
        // Bilingual: a third-language detection snaps to primary.
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("ko")),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_no_detection_uses_primary_in_bilingual() {
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), None),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_non_bilingual_matches_legacy_behavior() {
        // secondary = None → auto/empty prefers detection, explicit wins outright.
        assert_eq!(
            resolve_language(Some("auto"), None, Some("zh")),
            Some("zh".to_string())
        );
        assert_eq!(
            resolve_language(None, None, Some("ja")),
            Some("ja".to_string())
        );
        assert_eq!(
            resolve_language(Some("ja"), None, Some("zh")),
            Some("ja".to_string())
        );
        assert_eq!(resolve_language(Some("auto"), None, None), None);
    }
}
