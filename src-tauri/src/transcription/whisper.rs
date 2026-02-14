use anyhow::{Context, Result};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// Get the directory where whisper models are stored
pub fn models_dir() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir().context("Failed to find cache directory")?;
    let models_path = cache_dir.join("whisperi").join("whisper-models");
    std::fs::create_dir_all(&models_path)?;
    Ok(models_path)
}

/// Delete a downloaded whisper model
pub fn delete_model(file_name: &str) -> Result<()> {
    let path = models_dir()?.join(file_name);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Transcribe audio using whisper.cpp sidecar
pub async fn transcribe(
    app: &AppHandle,
    audio_data: &[u8],
    model_file: &str,
    language: Option<&str>,
    dictionary: &[String],
) -> Result<String> {
    let models_path = models_dir()?;
    let model_path = models_path.join(model_file);

    if !model_path.exists() {
        anyhow::bail!("Model file not found: {}", model_path.display());
    }

    // Write audio data to a temporary file
    let temp_dir = std::env::temp_dir();
    let temp_audio = temp_dir.join(format!("whisperi-audio-{}.wav", std::process::id()));
    std::fs::write(&temp_audio, audio_data)?;

    // Build whisper.cpp arguments
    let mut args: Vec<String> = vec![
        "-m".into(),
        model_path.to_string_lossy().to_string(),
        "-f".into(),
        temp_audio.to_string_lossy().to_string(),
        "--no-timestamps".into(),
        "-t".into(),
        num_cpus().to_string(),
    ];

    if let Some(lang) = language {
        if lang != "auto" {
            args.push("-l".into());
            args.push(lang.to_string());
        }
    }

    if !dictionary.is_empty() {
        args.push("--prompt".into());
        args.push(dictionary.join(" "));
    }

    // Execute whisper.cpp sidecar
    let output = app
        .shell()
        .sidecar("whisper-cpp")
        .map_err(|e| anyhow::anyhow!("Failed to create sidecar: {}", e))?
        .args(&args)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run whisper-cpp: {}", e))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_audio);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("whisper-cpp failed: {}", stderr);
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        log::warn!("[Whisperi] Local transcription result: empty (no voice detected)");
    } else {
        log::info!("[Whisperi] Local transcription result: {} chars", text.len());
    }
    Ok(text)
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
        * 3
        / 4
}
