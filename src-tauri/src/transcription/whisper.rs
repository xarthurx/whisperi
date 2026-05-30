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

/// Result of a whisper.cpp transcription: the text and (when auto-detected) the
/// language code that whisper itself identified.
#[derive(Debug, Clone)]
pub struct WhisperOutput {
    pub text: String,
    /// ISO 639-1 code reported by whisper.cpp's language ID step. `None` when
    /// the user forced a language (no detection runs) or when the stderr stream
    /// didn't include the expected marker.
    pub detected_language: Option<String>,
}

/// Parse whisper.cpp's stderr for the auto-detected language token.
/// whisper.cpp emits a line like:
///   `whisper_full_with_state: auto-detected language: zh (p = 0.998765)`
/// Returns the language code (`zh` here) if found, otherwise None.
pub fn parse_detected_language(stderr: &str) -> Option<String> {
    // Use a state-machine scan rather than regex to avoid pulling in the crate.
    const MARKER: &str = "auto-detected language: ";
    let pos = stderr.find(MARKER)?;
    let rest = &stderr[pos + MARKER.len()..];
    // Take ASCII letters/digits until whitespace, paren, or comma.
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        .unwrap_or(rest.len());
    let code = &rest[..end];
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

/// Transcribe audio using whisper.cpp sidecar.
/// `prompt` is the full conditioning + dictionary prompt (built by `build_prompt`).
///
/// Returns the transcribed text plus, when the request used auto-detection,
/// the language code whisper itself identified.
pub async fn transcribe(
    app: &AppHandle,
    audio_data: &[u8],
    model_file: &str,
    language: Option<&str>,
    prompt: &str,
) -> Result<WhisperOutput> {
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

    // Omit --prompt entirely when empty (auto-detect with no dictionary): an
    // empty or English conditioning prompt would bias whisper.cpp's output
    // language. See `build_prompt` / `punctuation_prompt`.
    if !prompt.is_empty() {
        args.push("--prompt".into());
        args.push(prompt.to_string());
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
    // Only parse the auto-detected language when the user actually asked for
    // auto-detect — when they forced `-l <code>`, whisper.cpp skips the ID step
    // and the marker doesn't appear (or echoes the forced value uselessly).
    let detected_language = match language {
        None | Some("auto") => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let parsed = parse_detected_language(&stderr);
            if let Some(ref code) = parsed {
                log::info!("[Whisperi] Whisper detected language: {}", code);
            }
            parsed
        }
        _ => None,
    };
    super::cloud::log_transcription_result("Local", &text, Some(prompt));
    Ok(WhisperOutput {
        text,
        detected_language,
    })
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
        * 3
        / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chinese_detection() {
        let stderr = "whisper_init_state: kv self size  =  None\n\
                      whisper_full_with_state: auto-detected language: zh (p = 0.998765)\n\
                      whisper_print_timings: total time =  450.00 ms\n";
        assert_eq!(parse_detected_language(stderr), Some("zh".to_string()));
    }

    #[test]
    fn parses_english_detection() {
        let stderr = "whisper_full_with_state: auto-detected language: en (p = 0.989)\n";
        assert_eq!(parse_detected_language(stderr), Some("en".to_string()));
    }

    #[test]
    fn parses_locale_subtag() {
        // Some builds may emit subtagged codes — accept hyphen/underscore.
        let stderr = "auto-detected language: zh-CN (p = 0.97)\n";
        assert_eq!(parse_detected_language(stderr), Some("zh-CN".to_string()));
    }

    #[test]
    fn returns_none_when_marker_absent() {
        let stderr = "whisper_init: loading model from /path\n\
                      whisper_print_timings: total time =  120.00 ms\n";
        assert_eq!(parse_detected_language(stderr), None);
    }

    #[test]
    fn returns_none_on_empty() {
        assert_eq!(parse_detected_language(""), None);
    }

    #[test]
    fn first_match_wins_on_repeated_marker() {
        // If for any reason the marker appears twice, take the first occurrence.
        let stderr = "auto-detected language: ja (p = 0.7)\n\
                      auto-detected language: zh (p = 0.6)\n";
        assert_eq!(parse_detected_language(stderr), Some("ja".to_string()));
    }

    #[test]
    fn handles_trailing_text_without_paren() {
        // Defensive: if whisper changes format and omits the (p = …) suffix.
        let stderr = "auto-detected language: fr";
        assert_eq!(parse_detected_language(stderr), Some("fr".to_string()));
    }
}
