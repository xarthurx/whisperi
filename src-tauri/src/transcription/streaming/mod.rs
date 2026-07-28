//! Streaming transcription — Live mode backend.
//!
//! `StreamingTranscriber` is the trait every realtime ASR backend implements.
//! Two providers ship MVP: OpenAI Realtime (`gpt-4o-mini-transcribe` /
//! `gpt-4o-transcribe`) and Alibaba Qwen3-ASR-Flash-Realtime. Both speak the
//! OpenAI Realtime API wire protocol so they share a single concrete
//! implementation, parameterized by `ProviderConfig`.

pub mod audio_pump;
pub mod providers;
pub mod realtime_openai_compatible;
pub mod reorder;

use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub provider_id: &'static str,
    pub model: String,
    /// ISO 639-1 language code, or `None` to auto-detect. When `None`, the
    /// adapter omits the `language` field from `session.update` so the provider
    /// detects the language itself (matches Standard mode's auto-detect
    /// behaviour). `start_live_session` normalizes "auto"/empty to `None`
    /// before constructing this struct.
    pub language: Option<String>,
    /// Canonical names and specialized terms used to bias providers that
    /// support transcription context. Unsupported providers omit the field.
    pub dictionary: Vec<String>,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum StreamingEvent {
    UtteranceCompleted { text: String, utterance_seq: u32 },
    Error { message: String, kind: ErrorKind },
    SessionClosed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ErrorKind {
    AuthFailed,
    RateLimited,
    NetworkDrop,
    ServerError,
    MaxMessageExceeded,
    BadResponse,
}

#[async_trait]
pub trait StreamingTranscriber: Send {
    /// Open the WebSocket and send the initial `session.update`.
    async fn open(&mut self, cfg: SessionConfig) -> anyhow::Result<()>;

    /// Push 16-bit signed PCM at the provider's target sample rate.
    async fn push_pcm16(&mut self, samples: &[i16]) -> anyhow::Result<()>;

    /// Send `input_audio_buffer.commit` to flush any pending utterance.
    /// Used for manual-commit providers (OpenAI gpt-realtime-whisper) and as
    /// the soft-flush trigger on stop for all providers.
    async fn commit_utterance(&mut self) -> anyhow::Result<()>;

    /// Close the WebSocket cleanly.
    async fn close(&mut self) -> anyhow::Result<()>;
}

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Tauri-managed state for active Live sessions.
/// Keyed by session_id; single-session in practice but use a map for forward compatibility.
pub struct LiveSessionState {
    sessions: Mutex<HashMap<u64, LiveSessionHandle>>,
    next_id: AtomicU64,
}

impl Default for LiveSessionState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

pub struct LiveSessionHandle {
    pub task: tokio::task::JoinHandle<()>,
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    pub expected_hwnd: Option<isize>,
}

impl LiveSessionState {
    pub fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert(&self, id: u64, handle: LiveSessionHandle) {
        self.sessions.lock().unwrap().insert(id, handle);
    }

    pub fn remove(&self, id: u64) -> Option<LiveSessionHandle> {
        self.sessions.lock().unwrap().remove(&id)
    }

    pub fn expected_hwnd(&self, id: u64) -> Option<isize> {
        self.sessions.lock().unwrap().get(&id).and_then(|h| h.expected_hwnd)
    }

    /// Best-effort graceful shutdown: signal every active session to cancel and
    /// await its soft-flush (clean WebSocket close) within a bounded per-session
    /// timeout, aborting any task that overruns. Called from the app's
    /// `RunEvent::ExitRequested` handler so quitting mid-Live-session flushes the
    /// trailing utterance and closes the provider socket cleanly instead of
    /// abandoning a detached task. Returns immediately when no session is active.
    pub async fn shutdown(&self, per_task_timeout: std::time::Duration) {
        // Drain the handles out from under the lock, then release it before any
        // `.await` — the sessions map is a std::sync::Mutex and must never be
        // held across an await point.
        let handles: Vec<LiveSessionHandle> = {
            let mut map = self.sessions.lock().unwrap();
            map.drain().map(|(_, handle)| handle).collect()
        };
        for handle in handles {
            let _ = handle.cancel_tx.send(true);
            let abort = handle.task.abort_handle();
            if tokio::time::timeout(per_task_timeout, handle.task)
                .await
                .is_err()
            {
                // Task didn't finish its soft-flush in time; abort so we don't
                // block process exit on a wedged WebSocket.
                abort.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// shutdown() must signal cancel to an active session, await its clean exit,
    /// and drain it from the registry.
    #[tokio::test]
    async fn shutdown_signals_cancel_and_drains() {
        let state = LiveSessionState::default();
        let id = state.new_id();
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let saw_cancel = Arc::new(AtomicBool::new(false));
        let saw_cancel_task = saw_cancel.clone();
        // Mimics the pump loop: runs until cancel is signalled, then exits.
        let task = tokio::spawn(async move {
            let _ = cancel_rx.changed().await;
            if *cancel_rx.borrow() {
                saw_cancel_task.store(true, Ordering::SeqCst);
            }
        });
        state.insert(
            id,
            LiveSessionHandle {
                task,
                cancel_tx,
                expected_hwnd: Some(42),
            },
        );

        state.shutdown(Duration::from_millis(1000)).await;

        assert!(
            saw_cancel.load(Ordering::SeqCst),
            "task should have observed the cancel signal"
        );
        assert!(
            state.remove(id).is_none(),
            "session map should be drained after shutdown"
        );
    }

    /// shutdown() must be bounded: a task that ignores cancel is aborted once the
    /// per-task timeout elapses, so quit never hangs.
    #[tokio::test]
    async fn shutdown_aborts_unresponsive_session_within_timeout() {
        let state = LiveSessionState::default();
        let id = state.new_id();
        // Keep the receiver alive but never react to cancel; the task hangs.
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(std::future::pending::<()>());
        state.insert(
            id,
            LiveSessionHandle {
                task,
                cancel_tx,
                expected_hwnd: None,
            },
        );

        let start = tokio::time::Instant::now();
        state.shutdown(Duration::from_millis(150)).await;

        assert!(
            start.elapsed() < Duration::from_millis(1200),
            "shutdown must be bounded by the per-task timeout, took {:?}",
            start.elapsed()
        );
        assert!(
            state.remove(id).is_none(),
            "session map should be drained even when the task is aborted"
        );
    }

    /// shutdown() on an idle registry is an immediate no-op (the common quit path).
    #[tokio::test]
    async fn shutdown_is_noop_when_no_sessions() {
        let state = LiveSessionState::default();
        let start = tokio::time::Instant::now();
        state.shutdown(Duration::from_millis(1000)).await;
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "no-session shutdown should return immediately"
        );
    }
}
