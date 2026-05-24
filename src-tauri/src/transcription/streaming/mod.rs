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
}
