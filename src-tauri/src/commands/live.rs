//! Tauri commands for Live dictation mode.

use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

use crate::audio::recorder::RecordingState;
use crate::clipboard::{
    ClipError, SwapResult, current_foreground_hwnd, current_foreground_window_class,
    send_text_keystrokes, swap_typed_text,
};
use crate::transcription::streaming::{
    LiveSessionHandle, LiveSessionState, SessionConfig, StreamingEvent, StreamingTranscriber,
    audio_pump::{OnlineResampler, f32_to_pcm16},
    providers,
    realtime_openai_compatible::RealtimeOpenAiCompatibleClient,
};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum TypeChunkResult {
    Typed(usize),
    SkippedTerminalFocus,
}

#[tauri::command]
pub async fn type_text_chunk(text: String) -> Result<TypeChunkResult, String> {
    match send_text_keystrokes(&text) {
        Ok(n) => Ok(TypeChunkResult::Typed(n)),
        Err(ClipError::TerminalFocusGuard) => Ok(TypeChunkResult::SkippedTerminalFocus),
    }
}

#[tauri::command]
pub async fn swap_typed_text_cmd(
    backspace_count: usize,
    new_text: String,
    expected_hwnd: Option<isize>,
) -> Result<SwapResult, String> {
    swap_typed_text(backspace_count, &new_text, expected_hwnd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_foreground_window() -> Result<isize, String> {
    Ok(current_foreground_hwnd())
}

#[tauri::command]
pub fn get_foreground_window_class() -> Result<Option<String>, String> {
    Ok(current_foreground_window_class())
}

#[tauri::command]
pub async fn start_live_session(
    app: AppHandle,
    sessions: State<'_, LiveSessionState>,
    rec_state: State<'_, RecordingState>,
    provider_id: String,
    model: String,
    language: Option<String>,
    api_key: String,
    expected_hwnd: Option<isize>,
) -> Result<u64, String> {
    // Validate provider exists
    let provider = providers::lookup(&provider_id)
        .ok_or_else(|| format!("Unknown Live provider: {}", provider_id))?;

    // Validate language is not "auto"
    if matches!(language.as_deref(), Some("auto")) {
        return Err("Live mode requires an explicit language (not 'auto').".into());
    }

    let session_id = sessions.new_id();
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

    // Clone the buffer Arc and sample rate before moving into the task.
    // samples_buf() returns Arc<Mutex<Vec<f32>>> which is Send + Clone.
    let samples_buf: Arc<Mutex<Vec<f32>>> = rec_state.samples_buf();
    let device_sample_rate = rec_state.current_sample_rate();

    let app_for_task = app.clone();

    // Create the client and open the WS connection up-front so that any
    // auth/network error fails the command synchronously (frontend can show a
    // toast immediately rather than racing on a background task error).
    let mut client = RealtimeOpenAiCompatibleClient::new(provider);
    client
        .open(SessionConfig {
            provider_id: provider.id,
            model: model.clone(),
            language: language.clone(),
            api_key: api_key.clone(),
        })
        .await
        .map_err(|e| format!("Failed to open Live session: {}", e))?;

    let target_sample_rate = client.sample_rate();

    // Spawn the audio-pump + event-reader task
    let task = tokio::spawn(async move {
        let mut resampler = OnlineResampler::new(device_sample_rate, target_sample_rate);
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        // Keep ~5 samples of trailing context in the cpal buffer for resampler continuity
        const TAIL_KEEP: usize = 5;
        let mut last_pulled = 0usize;

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let chunk = {
                        let mut buf = samples_buf.lock().unwrap();
                        if buf.len() > last_pulled + TAIL_KEEP {
                            let upto = buf.len() - TAIL_KEEP;
                            let v: Vec<f32> = buf[last_pulled..upto].to_vec();
                            // Live-mode-only drain to keep buffer bounded
                            buf.drain(..upto);
                            last_pulled = 0;
                            v
                        } else { Vec::new() }
                    };
                    if !chunk.is_empty() {
                        let resampled = resampler.process(&chunk);
                        let pcm = f32_to_pcm16(&resampled);
                        if let Err(e) = client.push_pcm16(&pcm).await {
                            emit_error(&app_for_task, format!("audio push failed: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                }
                evt = client.poll_event() => {
                    match evt {
                        Ok(Some(StreamingEvent::UtteranceCompleted { text, utterance_seq })) => {
                            let payload = serde_json::json!({
                                "text": text,
                                "utterance_seq": utterance_seq,
                            });
                            let _ = app_for_task.emit("live-utterance", payload);
                        }
                        Ok(Some(StreamingEvent::Error { message, kind })) => {
                            emit_error(&app_for_task, message, kind);
                            break;
                        }
                        Ok(Some(StreamingEvent::SessionClosed)) | Ok(None) => {}
                        Err(e) => {
                            emit_error(&app_for_task, format!("websocket: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() { break; }
                }
            }
        }

        // Soft flush: commit any pending utterance, drain final events for up to 800ms
        let _ = client.commit_utterance().await;
        let flush_deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        loop {
            let remaining = flush_deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() { break; }
            tokio::select! {
                _ = tokio::time::sleep(remaining) => break,
                evt = client.poll_event() => {
                    match evt {
                        Ok(Some(StreamingEvent::UtteranceCompleted { text, utterance_seq })) => {
                            let _ = app_for_task.emit("live-utterance", serde_json::json!({
                                "text": text,
                                "utterance_seq": utterance_seq,
                            }));
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }

        let _ = client.close().await;
        let _ = app_for_task.emit("live-session-closed", session_id);
    });

    sessions.insert(session_id, LiveSessionHandle {
        task,
        cancel_tx,
        expected_hwnd,
    });

    Ok(session_id)
}

#[tauri::command]
pub async fn stop_live_session(
    sessions: State<'_, LiveSessionState>,
    session_id: u64,
) -> Result<(), String> {
    // Find and remove the handle, claiming ownership
    let _handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;

    // Signal cancel. The spawned task's select! will pick this up on the next
    // iteration, break the main loop, and enter the soft-flush phase (commit_utterance,
    // drain events for 800ms, close).
    let _ = _handle.cancel_tx.send(true);

    // Soft-flush outer bound: give the task up to 1.5s to finish its cancel sequence
    // (commit, drain trailing .completed events, and close) before we return.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    Ok(())
}

#[tauri::command]
pub async fn cancel_live_session(
    sessions: State<'_, LiveSessionState>,
    session_id: u64,
) -> Result<(), String> {
    let _handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;
    // Hard cancel — no soft flush, no waiting. Task picks up the signal on next tick.
    let _ = _handle.cancel_tx.send(true);
    Ok(())
}

fn emit_error(app: &AppHandle, message: String, kind: crate::transcription::streaming::ErrorKind) {
    let _ = app.emit("live-error", serde_json::json!({
        "message": message,
        "kind": kind,
    }));
}
