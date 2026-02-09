use crate::audio::{AudioDevice, AudioRecorder, RecordingState};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize)]
struct AudioLevelPayload {
    level: f32,
}

#[derive(Clone, Serialize)]
struct RecordingErrorPayload {
    error: String,
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    AudioRecorder::list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: State<'_, RecordingState>,
    device_id: Option<String>,
) -> Result<(), String> {
    AudioRecorder::start(&state, device_id).map_err(|e| e.to_string())?;

    // Clone the Arc handles we need for the level emitter
    let (is_recording, peak_level, recording_error) = state.level_emitter_handles();

    // Spawn a thread to emit audio level events while recording
    std::thread::Builder::new()
        .name("whisperi-audio-level".to_string())
        .spawn(move || {
            while is_recording.load(Ordering::SeqCst) {
                let level = *peak_level.lock().unwrap();
                let _ = app.emit("audio-level", AudioLevelPayload { level });

                // Check for recording errors
                if let Some(error) = recording_error.lock().unwrap().clone() {
                    let _ = app.emit("recording-error", RecordingErrorPayload { error });
                    break;
                }

                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Emit a final zero level when recording stops
            let _ = app.emit("audio-level", AudioLevelPayload { level: 0.0 });
        })
        .map_err(|e| format!("Failed to spawn level emitter: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, RecordingState>) -> Result<Vec<u8>, String> {
    AudioRecorder::stop(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_level(state: State<'_, RecordingState>) -> Result<f32, String> {
    Ok(state.get_level())
}
