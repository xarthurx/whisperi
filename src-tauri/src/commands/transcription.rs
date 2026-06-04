use super::ResultExt;
use crate::transcription;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Normalize locale codes like "en-US" to ISO 639-1 "en" for transcription APIs.
fn normalize_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.split('-').next().unwrap_or(&l).to_string())
}

/// Result of a transcription Tauri command. `text` is the post-processed
/// (Simplified Chinese, full-width punctuation) output. `detected_language` is
/// what whisper/cloud reported during language ID — present only when the user
/// requested auto-detect AND the provider supports it. The frontend forwards
/// this into the subsequent reasoning call so AI enhancement runs with the
/// resolved language instead of "auto".
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

#[derive(Debug, Serialize)]
pub struct WhisperModelStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size: String,
    pub size_mb: u64,
    pub downloaded: bool,
    pub recommended: bool,
}

#[tauri::command]
pub async fn transcribe_local(
    app: AppHandle,
    audio_data: Vec<u8>,
    model: String,
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: Vec<String>,
    agent_terms: Vec<String>,
) -> Result<TranscriptionResult, String> {
    let file_name = format!("ggml-{}.bin", model);
    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);

    // Bilingual: prime BOTH languages and let the model detect within the pair
    // (no forced -l). Otherwise keep today's single/auto prompt.
    let full_prompt = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => crate::transcription::build_bilingual_prompt(&dictionary, p, s),
        _ => crate::transcription::build_prompt(&dictionary, primary.as_deref()),
    };
    // In bilingual mode never force a language; auto/single pass the choice through.
    let engine_lang: Option<String> = match secondary.as_deref() {
        Some(_) => None,
        None => primary.clone(),
    };

    let output = transcription::whisper::transcribe(
        &app,
        &audio_data,
        &file_name,
        engine_lang.as_deref(),
        &full_prompt,
    )
    .await
    .str_err()?;

    let stripped = transcription::cloud::strip_prompt_echo(&output.text, Some(&full_prompt));
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

    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);
    let prompt = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => crate::transcription::build_bilingual_prompt(&dictionary, p, s),
        _ => crate::transcription::build_prompt(&dictionary, primary.as_deref()),
    };
    // Bilingual → auto-detect within the pair; auto/single pass the choice through.
    let engine_lang: Option<String> = match secondary.as_deref() {
        Some(_) => None,
        None => primary.clone(),
    };

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

        "qwen" => transcription::cloud::transcribe_qwen(
            audio_data,
            &api_key,
            &model,
        )
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

#[tauri::command]
pub fn list_whisper_models() -> Result<Vec<WhisperModelStatus>, String> {
    let models_dir = transcription::whisper::models_dir().str_err()?;

    let models = vec![
        ("tiny", "Tiny", "Fastest, lower quality", "75MB", 75, false),
        (
            "base",
            "Base",
            "Good balance of speed and quality",
            "142MB",
            142,
            true,
        ),
        (
            "small",
            "Small",
            "Better quality, slower",
            "466MB",
            466,
            false,
        ),
        ("medium", "Medium", "High quality", "1.5GB", 1500, false),
        (
            "large",
            "Large",
            "Best quality, slowest",
            "3GB",
            3000,
            false,
        ),
        (
            "turbo",
            "Turbo",
            "Fast with good quality",
            "1.6GB",
            1600,
            false,
        ),
    ];

    Ok(models
        .into_iter()
        .map(|(id, name, desc, size, size_mb, recommended)| {
            let file_name = format!("ggml-{}.bin", id);
            let downloaded = models_dir.join(&file_name).exists();
            WhisperModelStatus {
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                size: size.to_string(),
                size_mb,
                downloaded,
                recommended,
            }
        })
        .collect())
}

#[derive(Clone, Serialize)]
struct ModelDownloadProgress {
    model_id: String,
    downloaded: u64,
    total: u64,
    percentage: u8,
}

#[tauri::command]
pub async fn download_whisper_model(app: AppHandle, model_id: String) -> Result<(), String> {
    let file_name = format!("ggml-{}.bin", model_id);
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        file_name
    );
    let dest = transcription::whisper::models_dir()
        .str_err()?
        .join(&file_name);

    // Skip if already downloaded
    if dest.exists() {
        let _ = tauri::Emitter::emit(
            &app,
            "model-download-progress",
            ModelDownloadProgress {
                model_id,
                downloaded: 0,
                total: 0,
                percentage: 100,
            },
        );
        return Ok(());
    }

    let model_id_clone = model_id.clone();
    crate::models::download_file(&url, &dest, move |downloaded, total| {
        let percentage = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };
        let _ = tauri::Emitter::emit(
            &app,
            "model-download-progress",
            ModelDownloadProgress {
                model_id: model_id_clone.clone(),
                downloaded,
                total,
                percentage,
            },
        );
    })
    .await
    .str_err()
}

#[tauri::command]
pub fn delete_whisper_model(model_id: String) -> Result<(), String> {
    let file_name = format!("ggml-{}.bin", model_id);
    transcription::whisper::delete_model(&file_name).str_err()
}

/// Get the sidecar binary filename for the current platform.
fn sidecar_binary_name() -> String {
    let target = env!("TARGET");
    if cfg!(target_os = "windows") {
        format!("whisper-cpp-{}.exe", target)
    } else {
        format!("whisper-cpp-{}", target)
    }
}

#[tauri::command]
pub fn get_whisper_status(app: AppHandle) -> Result<bool, String> {
    let binary_name = sidecar_binary_name();

    // In dev mode, check src-tauri/binaries/
    let dev_binary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(&binary_name);

    if dev_binary.exists() {
        return Ok(true);
    }

    // In production, check the resource directory
    if let Ok(resource_dir) = app.path().resource_dir() {
        let prod_binary = resource_dir.join(&binary_name);
        if prod_binary.exists() {
            return Ok(true);
        }
    }

    Ok(false)
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
        assert_eq!(resolve_language(Some("auto"), None, Some("zh")), Some("zh".to_string()));
        assert_eq!(resolve_language(None, None, Some("ja")), Some("ja".to_string()));
        assert_eq!(resolve_language(Some("ja"), None, Some("zh")), Some("ja".to_string()));
        assert_eq!(resolve_language(Some("auto"), None, None), None);
    }
}
