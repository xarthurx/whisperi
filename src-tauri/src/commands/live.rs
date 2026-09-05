//! Tauri commands for Live dictation mode.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

use crate::audio::recorder::RecordingState;
use crate::clipboard::{
    FocusTarget, SwapResult, current_focus_target, current_foreground_hwnd,
    current_foreground_window_class, send_text_keystrokes, swap_typed_text,
};
use crate::transcription::streaming::{
    LiveSessionHandle, LiveSessionState, SessionConfig, StreamingEvent, StreamingTranscriber,
    audio_pump::{OnlineResampler, f32_to_pcm16},
    providers,
    realtime_openai_compatible::RealtimeOpenAiCompatibleClient,
};

/// Result of typing one Live utterance chunk: how many UTF-16 units were
/// actually sent, plus the focus target they landed in (so the frontend can
/// group chunks per box and scope the post-stop swap to a single box).
#[derive(serde::Serialize)]
pub struct TypedChunk {
    pub chars: usize,
    pub window: isize,
    pub control: isize,
    pub scopable: bool,
}

#[tauri::command]
pub async fn type_text_chunk(text: String) -> Result<TypedChunk, String> {
    // Read where focus is right before typing so the chunk is attributed to the
    // box it actually lands in.
    let target = current_focus_target();
    let chars = send_text_keystrokes(&text).map_err(|e| e.to_string())?;
    Ok(TypedChunk {
        chars,
        window: target.window,
        control: target.control,
        scopable: target.scopable,
    })
}

#[tauri::command]
pub async fn swap_typed_text_cmd(
    backspace_count: usize,
    new_text: String,
    expected_hwnd: Option<isize>,
    expected_control: Option<isize>,
) -> Result<SwapResult, String> {
    swap_typed_text(backspace_count, &new_text, expected_hwnd, expected_control)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_focus_target() -> Result<FocusTarget, String> {
    Ok(current_focus_target())
}

#[tauri::command]
pub fn get_foreground_window() -> Result<isize, String> {
    Ok(current_foreground_hwnd())
}

#[tauri::command]
pub fn get_foreground_window_class() -> Result<Option<String>, String> {
    Ok(current_foreground_window_class())
}

/// Resolve the language the frontend hands over into what the provider's
/// `session.update` expects: `None` for auto-detect ("auto", empty, or absent),
/// otherwise a lower-case ISO 639-1 code. The stored `preferredLanguage` is a
/// locale for English (`en-US` / `en-GB`); providers reject malformed codes, so
/// the region suffix is dropped exactly as Standard mode's `normalize_language`
/// does.
fn session_language(language: Option<&str>) -> Option<String> {
    let lang = language?.trim();
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
        return None;
    }
    let base = lang.split('-').next().unwrap_or(lang);
    Some(base.to_ascii_lowercase())
}

#[tauri::command]
pub async fn start_live_session(
    app: AppHandle,
    sessions: State<'_, Arc<LiveSessionState>>,
    rec_state: State<'_, RecordingState>,
    provider_id: String,
    model: String,
    language: Option<String>,
    dictionary: Vec<String>,
    api_key: String,
    expected_hwnd: Option<isize>,
) -> Result<u64, String> {
    log::info!(
        "[Live] start_live_session: provider_id={} model={} language={:?} expected_hwnd={:?} key_len={}",
        provider_id,
        model,
        language,
        expected_hwnd,
        api_key.len()
    );

    // Validate provider exists
    let provider = providers::lookup(&provider_id)
        .ok_or_else(|| format!("Unknown Live provider: {}", provider_id))?;

    // Language is optional (matches Standard mode's auto-detect). The adapter
    // omits the field from session.update when this is None, letting the
    // provider auto-detect from audio.
    let language_for_session = session_language(language.as_deref());

    // Defensive fallback: if the frontend somehow passes an empty model
    // (stale/empty setting), fall back to the provider's documented default
    // streaming model rather than letting the server reject with
    // "missing_model".
    let effective_model = if model.trim().is_empty() {
        log::warn!(
            "[Live] start_live_session: empty model — falling back to provider default {}",
            provider.default_model
        );
        provider.default_model.to_string()
    } else {
        model.clone()
    };

    let session_id = sessions.new_id();
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

    // Clone the buffer Arc and sample rate before moving into the task.
    // samples_buf() returns Arc<Mutex<Vec<f32>>> which is Send + Clone.
    let samples_buf: Arc<Mutex<Vec<f32>>> = rec_state.samples_buf();
    let device_sample_rate = rec_state.current_sample_rate();

    let app_for_task = app.clone();
    // Clone the Arc so the task can self-remove from LiveSessionState on exit,
    // preventing HashMap leaks when the WS dies without an explicit stop/cancel.
    let sessions_for_task: Arc<LiveSessionState> = Arc::clone(&*sessions);

    // Create the client and open the WS connection up-front so that any
    // auth/network error fails the command synchronously (frontend can show a
    // toast immediately rather than racing on a background task error).
    let mut client = RealtimeOpenAiCompatibleClient::new(provider);
    client
        .open(SessionConfig {
            provider_id: provider.id,
            model: effective_model.clone(),
            language: language_for_session.clone(),
            dictionary,
            api_key: api_key.clone(),
        })
        .await
        .map_err(|e| {
            log::error!("[Live] start_live_session: open() failed: {:#}", e);
            format!("Failed to open Live session: {}", e)
        })?;
    log::info!(
        "[Live] start_live_session: open() succeeded (effective_model={})",
        effective_model
    );

    let target_sample_rate = client.sample_rate();

    // Spawn the audio-pump + event-reader task
    let task = tokio::spawn(async move {
        let mut resampler = OnlineResampler::new(device_sample_rate, target_sample_rate);
        let mut tick = tokio::time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    // Drain the full shared buffer each tick. OnlineResampler holds its
                    // own trailing-sample / src_offset state for cross-chunk continuity,
                    // so we don't need to retain any samples in the cpal buffer.
                    let chunk: Vec<f32> = {
                        let mut buf = samples_buf.lock().unwrap();
                        if buf.is_empty() { Vec::new() } else { buf.drain(..).collect() }
                    };
                    if !chunk.is_empty() {
                        let resampled = resampler.process(&chunk);
                        let pcm = f32_to_pcm16(&resampled);
                        if let Err(e) = client.push_pcm16(&pcm).await {
                            emit_error(&app_for_task, session_id, format!("audio push failed: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                    // Release any utterance the reorder buffer has held past its
                    // head-of-line timeout (a dropped earlier utterance that never
                    // arrives); the client owns the timer, the pump just ticks it.
                    for evt in client.check_reorder_timeout(tokio::time::Instant::now()) {
                        emit_utterance(&app_for_task, session_id, evt);
                    }
                }
                evt = client.poll_event() => {
                    match evt {
                        Ok(Some(evt @ StreamingEvent::UtteranceCompleted { .. })) => {
                            emit_utterance(&app_for_task, session_id, evt);
                        }
                        Ok(Some(StreamingEvent::Error { message, kind })) => {
                            emit_error(&app_for_task, session_id, message, kind);
                            break;
                        }
                        Ok(Some(StreamingEvent::SessionClosed)) => {
                            // The provider ended the session on its own (we have
                            // not sent end-of-audio yet); nothing more will be
                            // transcribed, so surface it instead of pumping audio
                            // into a finished session.
                            emit_error(&app_for_task, session_id, "provider ended the session".to_string(), crate::transcription::streaming::ErrorKind::ServerError);
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            emit_error(&app_for_task, session_id, format!("websocket: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        log::info!("[Live] audio pump received cancel signal, entering soft-flush");
                        break;
                    }
                }
            }
        }

        // Soft flush: tell the provider no more audio is coming (OpenAI: commit
        // the buffer; DashScope: `session.finish`), then drain final events for
        // up to 800ms or until the provider says the session is finished.
        let _ = client.end_of_audio().await;
        let flush_deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        loop {
            let remaining = flush_deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(remaining) => break,
                evt = client.poll_event() => {
                    match evt {
                        Ok(Some(evt @ StreamingEvent::UtteranceCompleted { .. })) => {
                            emit_utterance(&app_for_task, session_id, evt);
                        }
                        Ok(Some(StreamingEvent::SessionClosed)) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }

        // Release any completion still held by the reorder buffer (a trailing
        // utterance whose earlier sibling never arrived) so the tail isn't lost.
        for evt in client.flush_reorder() {
            emit_utterance(&app_for_task, session_id, evt);
        }

        let _ = client.close().await;
        // Self-remove from state in case the loop exited without stop/cancel
        // being called (e.g. server-side WS close, push_pcm16 error). If the
        // command path already removed it, this is a no-op.
        let _ = sessions_for_task.remove(session_id);
        log::info!("[Live] audio pump task exiting, session_id={}", session_id);
        let _ = app_for_task.emit("live-session-closed", session_id);
    });

    sessions.insert(
        session_id,
        LiveSessionHandle {
            task,
            cancel_tx,
            expected_hwnd,
        },
    );

    Ok(session_id)
}

#[tauri::command]
pub async fn stop_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String> {
    log::info!("[Live] stop_live_session: session_id={}", session_id);
    // Find and remove the handle, claiming ownership
    let handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;

    // Signal cancel. The spawned task's select! will pick this up on the next
    // iteration, break the main loop, and enter the soft-flush phase (commit_utterance,
    // drain events for 800ms, close).
    let _ = handle.cancel_tx.send(true);

    // Wait for the task to finish its cancel sequence, up to a 1.5s bound. Returns
    // immediately on short utterances (typical case) rather than blocking the command
    // for the full 1.5s.
    //
    // Capture an AbortHandle BEFORE moving the JoinHandle into `timeout`. If timeout
    // elapses, the JoinHandle is dropped, which by itself only detaches the task and
    // leaves the audio pump / WS loop running. Call abort() on the surviving handle
    // so the task is actually cancelled.
    let abort_handle = handle.task.abort_handle();
    let result = tokio::time::timeout(Duration::from_millis(1500), handle.task).await;
    match result {
        Ok(_) => log::info!("[Live] stop_live_session: task joined cleanly"),
        Err(_) => {
            log::warn!("[Live] stop_live_session: task timed out after 1500ms; aborting");
            abort_handle.abort();
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn cancel_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String> {
    log::info!("[Live] cancel_live_session: session_id={}", session_id);
    let handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;
    let _ = handle.cancel_tx.send(true);
    // Await the task with a bound so the user can't rapid-fire start/cancel
    // and produce two concurrent audio pumps racing on the shared samples_buf.
    // The task's main loop polls cancel_rx on every ~100ms tick, then runs the
    // 800ms soft-flush; ~1000ms is enough headroom. Capture an AbortHandle BEFORE
    // moving the JoinHandle so we can actually cancel the task on timeout — a
    // dropped JoinHandle alone just detaches.
    let abort_handle = handle.task.abort_handle();
    if tokio::time::timeout(Duration::from_millis(1000), handle.task)
        .await
        .is_err()
    {
        log::warn!("[Live] cancel_live_session: task timed out after 1000ms; aborting");
        abort_handle.abort();
    }
    Ok(())
}

/// Emit a `live-utterance` event for a completion the reorder buffer released.
/// A no-op for any other event variant, so callers can hand it whatever the
/// reorder helpers return without matching first.
fn emit_utterance(app: &AppHandle, session_id: u64, evt: StreamingEvent) {
    if let StreamingEvent::UtteranceCompleted {
        text,
        utterance_seq,
    } = evt
    {
        let _ = app.emit(
            "live-utterance",
            serde_json::json!({
                "session_id": session_id,
                "text": text,
                "utterance_seq": utterance_seq,
            }),
        );
    }
}

fn emit_error(
    app: &AppHandle,
    session_id: u64,
    message: String,
    kind: crate::transcription::streaming::ErrorKind,
) {
    let _ = app.emit(
        "live-error",
        serde_json::json!({
            "session_id": session_id,
            "message": message,
            "kind": kind,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend hands over the stored `preferredLanguage`, which for English
    /// is a locale (`en-US` / `en-GB`). Providers take ISO 639-1 codes and reject
    /// malformed ones, so the Live path must normalize exactly like Standard mode.
    #[test]
    fn session_language_normalizes_locales_to_iso_639_1() {
        assert_eq!(session_language(Some("en-US")), Some("en".to_string()));
        assert_eq!(session_language(Some("en-GB")), Some("en".to_string()));
        assert_eq!(session_language(Some("zh")), Some("zh".to_string()));
        assert_eq!(session_language(Some("JA")), Some("ja".to_string()));
    }

    #[test]
    fn session_language_auto_and_empty_mean_auto_detect() {
        assert_eq!(session_language(None), None);
        assert_eq!(session_language(Some("auto")), None);
        assert_eq!(session_language(Some("")), None);
        assert_eq!(session_language(Some("  ")), None);
    }
}
