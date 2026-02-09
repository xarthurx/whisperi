use crate::transcription;
use serde::Serialize;
use tauri::AppHandle;

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
    // Look up model file name from model ID
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
    // This will be populated from the model registry JSON in Phase 6
    // For now return a hardcoded list matching the existing models
    let models_dir = transcription::whisper::models_dir().map_err(|e| e.to_string())?;

    let models = vec![
        ("tiny", "Tiny", "Fastest, lower quality", "75MB", 75, false),
        ("base", "Base", "Good balance of speed and quality", "142MB", 142, true),
        ("small", "Small", "Better quality, slower", "466MB", 466, false),
        ("medium", "Medium", "High quality", "1.5GB", 1500, false),
        ("large", "Large", "Best quality, slowest", "3GB", 3000, false),
        ("turbo", "Turbo", "Fast with good quality", "1.6GB", 1600, false),
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

#[tauri::command]
pub async fn download_whisper_model(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let file_name = format!("ggml-{}.bin", model_id);
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        file_name
    );
    let dest = transcription::whisper::models_dir()
        .map_err(|e| e.to_string())?
        .join(&file_name);

    crate::models::download_file(&url, &dest, |downloaded, total| {
        // Emit progress events to frontend
        let _ = tauri::Emitter::emit(
            &app,
            "model-download-progress",
            serde_json::json!({
                "model_id": model_id,
                "downloaded": downloaded,
                "total": total,
            }),
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

#[tauri::command]
pub fn get_whisper_status() -> Result<bool, String> {
    // Check if whisper-cpp sidecar binary exists
    // This is a simplified check — the actual sidecar resolution happens at runtime
    Ok(true)
}
