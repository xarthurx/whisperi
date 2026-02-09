use crate::transcription;
use serde::Serialize;
use tauri::{AppHandle, Manager};

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
) -> Result<String, String> {
    let file_name = format!("ggml-{}.bin", model);

    transcription::whisper::transcribe(
        &app,
        &audio_data,
        &file_name,
        language.as_deref(),
        &dictionary,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transcribe_cloud(
    audio_data: Vec<u8>,
    provider: String,
    api_key: String,
    model: String,
    language: Option<String>,
    dictionary: Vec<String>,
) -> Result<String, String> {
    let prompt = if dictionary.is_empty() {
        None
    } else {
        Some(dictionary.join(" "))
    };

    match provider.as_str() {
        "openai" => transcription::cloud::transcribe_openai(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
            prompt.as_deref(),
            None,
        )
        .await
        .map_err(|e| e.to_string()),

        "groq" => transcription::cloud::transcribe_groq(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
            prompt.as_deref(),
        )
        .await
        .map_err(|e| e.to_string()),

        "mistral" => transcription::cloud::transcribe_mistral(
            audio_data,
            &api_key,
            &model,
            language.as_deref(),
            prompt.as_deref(),
        )
        .await
        .map_err(|e| e.to_string()),

        other => Err(format!("Unknown transcription provider: {}", other)),
    }
}

#[tauri::command]
pub fn list_whisper_models() -> Result<Vec<WhisperModelStatus>, String> {
    let models_dir = transcription::whisper::models_dir().map_err(|e| e.to_string())?;

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
        .map_err(|e| e.to_string())?
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
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_whisper_model(model_id: String) -> Result<(), String> {
    let file_name = format!("ggml-{}.bin", model_id);
    transcription::whisper::delete_model(&file_name).map_err(|e| e.to_string())
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
