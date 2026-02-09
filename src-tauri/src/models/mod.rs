use anyhow::Result;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size: String,
    pub size_mb: u64,
    pub file_name: String,
    pub download_url: String,
    pub downloaded: bool,
    #[serde(default)]
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub disable_thinking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudProvider {
    pub id: String,
    pub name: String,
    pub models: Vec<CloudModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionModel {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<TranscriptionModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistry {
    pub whisper_models: Vec<WhisperModelInfo>,
    pub cloud_providers: Vec<CloudProvider>,
    pub transcription_providers: Vec<TranscriptionProvider>,
}

/// Get the whisper models directory
pub fn whisper_models_dir() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir().ok_or_else(|| anyhow::anyhow!("No cache directory"))?;
    let path = cache_dir.join("whisperi").join("whisper-models");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Download a file with streaming progress reporting.
/// Calls `on_progress(downloaded_bytes, total_bytes)` periodically during download.
pub async fn download_file(
    url: &str,
    dest: &PathBuf,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);

    // Download to a temp file first, then rename on success
    let temp_dest = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&temp_dest).await?;
    let mut downloaded: u64 = 0;
    let mut last_progress = std::time::Instant::now();

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        downloaded += chunk.len() as u64;

        // Throttle progress callbacks to ~100ms
        let now = std::time::Instant::now();
        if now.duration_since(last_progress).as_millis() >= 100 || downloaded >= total_size {
            on_progress(downloaded, total_size);
            last_progress = now;
        }
    }

    // Flush and close the file
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);

    // Rename .part to final destination
    tokio::fs::rename(&temp_dest, dest).await?;

    // Final progress callback
    on_progress(downloaded, total_size);

    Ok(())
}
