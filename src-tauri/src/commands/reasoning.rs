use crate::reasoning::{self, ReasoningRequest};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn process_reasoning(
    text: String,
    model: String,
    provider: String,
    system_prompt: String,
    api_key: String,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    language: Option<String>,
    language_pair: Option<Vec<String>>,
) -> Result<String, String> {
    // Never log any part of the API key (see transcribe_cloud). Presence only.
    log::info!(
        "[Whisperi] Enhancing: provider={}, model={}, has_key={}",
        provider,
        model,
        !api_key.is_empty()
    );

    let req = ReasoningRequest {
        text,
        model,
        provider,
        system_prompt,
        api_key,
        max_tokens,
        temperature,
    };

    match reasoning::process(&req).await {
        Ok(response) => {
            log::info!("[Whisperi] Enhancement complete ({} chars)", response.text.len());
            // Run the full Chinese post-processing pipeline (punctuation + T→S
            // when the configured language is Chinese). Acts as a deterministic
            // safety net for models that occasionally slip into Traditional
            // characters or half-width punctuation despite prompt instructions.
            // The bilingual pair decides whether Han text under an English
            // resolution is a Chinese clause (zh pair) or kanji (ja pair).
            let pair: Vec<&str> = language_pair.iter().flatten().map(String::as_str).collect();
            Ok(crate::transcription::finalize_for_languages(
                &response.text,
                language.as_deref(),
                &pair,
            ))
        }
        Err(e) => {
            log::error!("[Whisperi] Enhancement failed: {}", e);
            Err(e.to_string())
        }
    }
}
