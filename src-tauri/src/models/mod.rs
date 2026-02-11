use anyhow::Result;
use futures_util::StreamExt;
use std::path::PathBuf;

/// Download a file with streaming progress reporting.
/// Calls `on_progress(downloaded_bytes, total_bytes)` periodically during download.
pub async fn download_file(
    url: &str,
    dest: &PathBuf,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::HTTP_CLIENT.get(url).send().await?;

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
