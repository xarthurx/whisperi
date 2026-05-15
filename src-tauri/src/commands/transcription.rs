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

/// Pick the language to use for the post-processing (T→S) decision. If the
/// user requested auto-detect, prefer what the model reported; otherwise use
/// what the user explicitly chose.
fn effective_language(
    requested: Option<&str>,
    detected: Option<&str>,
) -> Option<String> {
    match requested {
        None | Some("auto") => detected.map(str::to_string),
        Some(other) => Some(other.to_string()),
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
    dictionary: Vec<String>,
) -> Result<TranscriptionResult, String> {
    let file_name = format!("ggml-{}.bin", model);
    let language = normalize_language(language);
    let full_prompt = crate::transcription::build_prompt(&dictionary, language.as_deref());

    let output = transcription::whisper::transcribe(
        &app,
        &audio_data,
        &file_name,
        language.as_deref(),
        &full_prompt,
    )
    .await
    .str_err()?;

    let stripped = transcription::cloud::strip_prompt_echo(&output.text, Some(&full_prompt));
    let effective = effective_language(language.as_deref(), output.detected_language.as_deref());
    let text = transcription::finalize_chinese_text(&stripped, effective.as_deref());
    Ok(TranscriptionResult {
        text,
        detected_language: output.detected_language,
    })
}

#[tauri::command]
pub async fn transcribe_cloud(
    audio_data: Vec<u8>,
    provider: String,
    api_key: String,
    model: String,
    language: Option<String>,
    dictionary: Vec<String>,
) -> Result<TranscriptionResult, String> {
    let key_preview = if api_key.len() > 8 {
        format!("{}...{}", &api_key[..4], &api_key[api_key.len()-4..])
    } else {
        "(too short)".to_string()
    };
    log::info!("[Whisperi] Transcribing: provider={}, model={}, key={}", provider, model, key_preview);

    let language = normalize_language(language);
    let prompt = crate::transcription::build_prompt(&dictionary, language.as_deref());

    let output = match provider.as_str() {
        "openai" => transcription::cloud::transcribe_openai(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
            Some(prompt.as_str()),
            None,
        )
        .await
        .str_err()?,

        "groq" => transcription::cloud::transcribe_groq(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
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
            language.as_deref(),
            Some(prompt.as_str()),
        )
        .await
        .str_err()?,

        "openrouter" => transcription::cloud::transcribe_openrouter(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
            Some(prompt.as_str()),
        )
        .await
        .str_err()?,

        other => return Err(format!("Unknown transcription provider: {}", other)),
    };

    let stripped = transcription::cloud::strip_prompt_echo(&output.text, Some(prompt.as_str()));
    let effective = effective_language(language.as_deref(), output.detected_language.as_deref());
    let text = transcription::finalize_chinese_text(&stripped, effective.as_deref());
    Ok(TranscriptionResult {
        text,
        detected_language: output.detected_language,
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
    fn effective_language_prefers_user_choice() {
        // User explicitly chose Japanese — detection result is ignored.
        assert_eq!(
            effective_language(Some("ja"), Some("zh")),
            Some("ja".to_string())
        );
    }

    #[test]
    fn effective_language_uses_detection_in_auto_mode() {
        assert_eq!(
            effective_language(Some("auto"), Some("zh")),
            Some("zh".to_string())
        );
        assert_eq!(
            effective_language(None, Some("ja")),
            Some("ja".to_string())
        );
    }

    #[test]
    fn effective_language_falls_back_to_none_when_no_detection() {
        // Auto mode + no detection → None. finalize_chinese_text will then
        // use its own kana heuristic.
        assert_eq!(effective_language(Some("auto"), None), None);
        assert_eq!(effective_language(None, None), None);
    }
}
