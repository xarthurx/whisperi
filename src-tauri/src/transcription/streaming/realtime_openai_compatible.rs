//! OpenAI Realtime API wire-compatible WebSocket client.
//!
//! Both OpenAI's `/v1/realtime` and Alibaba DashScope's
//! `/api-ws/v1/realtime` accept the same wire protocol. This client is
//! parameterized by `ProviderConfig` so a single implementation serves both.

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

use super::providers::{AuthScheme, ProviderConfig, VadMode, VocabularyField};
use super::reorder::ReorderBuffer;
use super::{SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::Instant;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsSource = SplitStream<WsStream>;

/// Head-of-line bound for the reorder buffer: a completion held waiting for an
/// earlier utterance that never finishes is released after this. Comfortably
/// longer than realistic transcription latency, since a merely-slow utterance
/// fills the gap (via `poll_event`) and disarms the timer before it fires.
const REORDER_HEAD_TIMEOUT: Duration = Duration::from_millis(2500);

pub struct RealtimeOpenAiCompatibleClient {
    cfg: &'static ProviderConfig,
    sink: Option<WsSink>,
    source: Option<WsSource>,
    /// Monotonic counter stamped onto each emitted (post-reorder) utterance.
    emit_seq: u32,
    /// Restores spoken order when the provider finalises a short later utterance
    /// before a long earlier one (out-of-order `.completed` events).
    reorder: ReorderBuffer,
    /// Completions the reorder buffer has released, handed back one per
    /// `poll_event` call.
    ready: VecDeque<StreamingEvent>,
    /// When the reorder buffer first became head-of-line blocked, for the
    /// [`Self::check_reorder_timeout`] deadline. `None` while not blocked.
    blocked_since: Option<Instant>,
}

impl RealtimeOpenAiCompatibleClient {
    pub fn new(provider: &'static ProviderConfig) -> Self {
        Self {
            cfg: provider,
            sink: None,
            source: None,
            emit_seq: 0,
            reorder: ReorderBuffer::new(),
            ready: VecDeque::new(),
            blocked_since: None,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.cfg.audio_sample_rate
    }

    pub fn vad_mode(&self) -> &VadMode {
        &self.cfg.vad_mode
    }

    /// Read one streaming event. Returns `Ok(Some(event))` for a real event,
    /// `Ok(None)` for messages we don't surface (pings, deltas, capture-order
    /// ranking signals, or a completion the reorder buffer is still holding), and
    /// `Err` on transport failure or close.
    ///
    /// Completed transcripts pass through [`ReorderBuffer`] so they surface in
    /// spoken order even when the provider finalises a short later utterance
    /// before a long earlier one.
    pub async fn poll_event(&mut self) -> Result<Option<StreamingEvent>> {
        // Hand back any completion the reorder buffer already released before
        // reading more off the socket.
        if let Some(evt) = self.ready.pop_front() {
            return Ok(Some(evt));
        }
        let source = self.source.as_mut().ok_or_else(|| anyhow!("not connected"))?;
        let Some(msg) = source.next().await else {
            return Err(anyhow!("websocket closed"));
        };
        let text = match msg.context("ws read")? {
            Message::Text(text) => text,
            Message::Close(frame) => {
                log::warn!("[Live] WS close frame: {:?}", frame);
                return Err(anyhow!("websocket closed by server"));
            }
            _ => return Ok(None),
        };
        // Parse once and reuse for both logging and dispatch.
        // INFO: type + size only (no transcript content — that's user speech and
        // shouldn't land in production logs). DEBUG: full payload (truncated at
        // 600 bytes on a char boundary).
        let parsed = serde_json::from_str::<Value>(&text);
        let evt_type = parsed
            .as_ref()
            .ok()
            .and_then(|v| v.get("type").and_then(|t| t.as_str()))
            .unwrap_or("<unparseable>")
            .to_string();
        log::info!("[Live] WS recv type={} ({} bytes)", evt_type, text.len());
        log::debug!(
            "[Live] WS recv payload: {}",
            truncate_at_char_boundary(&text, 600)
        );
        let value = parsed.context("parse event json")?;

        match parse_event(&value)? {
            WireEvent::Committed { item_id } => {
                // Capture-order ranking signal — assign the rank now, emit nothing.
                self.reorder.observe(&item_id);
                Ok(None)
            }
            WireEvent::Completed { item_id, transcript } => {
                let released = self.ingest_completed(&item_id, transcript);
                let events = self.wrap_completed(released);
                self.ready.extend(events);
                Ok(self.ready.pop_front())
            }
            WireEvent::Error { message, kind } => {
                Ok(Some(StreamingEvent::Error { message, kind }))
            }
            WireEvent::Finished => Ok(Some(StreamingEvent::SessionClosed)),
            WireEvent::Ignored => Ok(None),
        }
    }

    /// Feed a completed transcript into the reorder buffer and re-arm the
    /// head-of-line timer when the head advances. Without this, a completion that
    /// advances the head onto a *new* still-missing rank (e.g. rank 0 lands while
    /// rank 1 is still absent) leaves `blocked_since` measuring from the previous
    /// block, so the new head could be skipped well before its full
    /// `REORDER_HEAD_TIMEOUT`. Keyed on the head moving — not on text being
    /// released — so advancing past a silent (empty) rank re-arms it too.
    fn ingest_completed(&mut self, item_id: &str, transcript: String) -> Vec<String> {
        let head_before = self.reorder.head();
        let released = self.reorder.complete(item_id, transcript);
        if self.reorder.head() != head_before {
            // Head advanced; clear the clock so the next `check_reorder_timeout`
            // tick re-arms it fresh for the newly-exposed head.
            self.blocked_since = None;
        }
        released
    }

    /// Release any completion held past the head-of-line timeout. The pump calls
    /// this once per tick with the current instant; the client owns the timer so
    /// the timeout policy stays with the buffer it guards rather than leaking into
    /// the audio pump. A merely-slow utterance fills the gap via `poll_event` and
    /// disarms the timer before it fires; only a dropped/never-arriving earlier
    /// utterance trips it.
    pub fn check_reorder_timeout(&mut self, now: Instant) -> Vec<StreamingEvent> {
        if !self.reorder.is_blocked() {
            self.blocked_since = None;
            return Vec::new();
        }
        let since = *self.blocked_since.get_or_insert(now);
        if now.duration_since(since) >= REORDER_HEAD_TIMEOUT {
            self.blocked_since = None;
            let released = self.reorder.skip_head();
            return self.wrap_completed(released);
        }
        Vec::new()
    }

    /// Release every still-buffered completion in spoken order. Called during the
    /// stop soft-flush so a held tail isn't lost on close.
    pub fn flush_reorder(&mut self) -> Vec<StreamingEvent> {
        let released = self.reorder.flush_all();
        self.wrap_completed(released)
    }

    /// Stamp a monotonic post-reorder sequence number onto each released text.
    fn wrap_completed(&mut self, texts: Vec<String>) -> Vec<StreamingEvent> {
        texts
            .into_iter()
            .map(|text| {
                self.emit_seq += 1;
                StreamingEvent::UtteranceCompleted {
                    text,
                    utterance_seq: self.emit_seq,
                }
            })
            .collect()
    }
}

/// A decoded wire message, before reordering.
#[derive(Debug)]
enum WireEvent {
    /// An utterance's audio segment was committed, or speech started — the
    /// capture-order ranking signal, carrying the provider's `item_id`.
    Committed { item_id: String },
    /// A transcription finished. `transcript` may be empty (silence/noise).
    Completed { item_id: String, transcript: String },
    Error { message: String, kind: ErrorKind },
    /// The server ended the session (DashScope's `session.finished`, sent after
    /// the trailing utterance in reply to `session.finish`). Nothing more will
    /// arrive; the client should close the socket.
    Finished,
    /// A message we don't act on (session.created, deltas, pings, …).
    Ignored,
}

fn item_id_of(v: &Value) -> Option<String> {
    v.get("item_id").and_then(|i| i.as_str()).map(str::to_string)
}

/// Decode a single (already-parsed) provider message into a [`WireEvent`]. Pure
/// — all ordering and sequence-number state lives in the client, not here.
fn parse_event(v: &Value) -> Result<WireEvent> {
    let evt_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match evt_type {
        // Capture-order signals: the provider emits these sequentially as the
        // user speaks, BEFORE transcription completes, so first-sight order is
        // spoken order. Both are observed because providers differ in which they
        // send; whichever names an item_id first sets its rank (idempotent).
        "input_audio_buffer.committed" | "input_audio_buffer.speech_started" => {
            match item_id_of(v) {
                Some(item_id) => Ok(WireEvent::Committed { item_id }),
                None => Ok(WireEvent::Ignored),
            }
        }
        "conversation.item.input_audio_transcription.completed"
        | "transcription_session.completed" /* fallback variant some providers emit */ => {
            let transcript = v
                .get("transcript")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            // Correlate by item_id; fall back to the unique event_id so a provider
            // that omits item_id still gets a distinct (unobserved) rank and emits
            // in arrival order rather than mis-correlating across utterances.
            let item_id = item_id_of(v)
                .or_else(|| v.get("event_id").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_default();
            Ok(WireEvent::Completed { item_id, transcript })
        }
        "error" => {
            let message = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            let code = v
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let kind = classify_error(code);
            Ok(WireEvent::Error { message, kind })
        }
        "session.finished" => Ok(WireEvent::Finished),
        _ => Ok(WireEvent::Ignored),
    }
}

/// Truncate `text` to at most `max_bytes`, snapping to a UTF-8 char boundary
/// so we never panic when the cut would land inside a multi-byte sequence
/// (e.g. a 3-byte CJK or 4-byte emoji codepoint straddling byte 600 in a
/// large server event).
fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn classify_error(code: &str) -> ErrorKind {
    match code {
        "invalid_api_key" | "unauthorized" | "auth_failed" => ErrorKind::AuthFailed,
        "rate_limit_exceeded" | "rate_limited" => ErrorKind::RateLimited,
        c if c.starts_with("server_") => ErrorKind::ServerError,
        _ => ErrorKind::BadResponse,
    }
}

/// Whether an OpenAI transcription model takes the plural `languages` array.
/// `gpt-transcribe` and `gpt-live-transcribe` (and their dated snapshots) do,
/// and OpenAI's docs say the singular `language` must not be sent alongside it.
/// The legacy `whisper-1` / `gpt-4o-*-transcribe` models take `language`.
fn uses_languages_array(model: &str) -> bool {
    model.starts_with("gpt-transcribe") || model.starts_with("gpt-live-transcribe")
}

/// Render the dictionary as one biasing sentence, deduplicated case-insensitively.
/// `None` when nothing usable remains.
fn vocabulary_hint(dictionary: &[String]) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    let terms: Vec<&str> = dictionary
        .iter()
        .map(|term| term.trim())
        .filter(|term| !term.is_empty())
        .filter(|term| seen.insert(term.to_lowercase()))
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(format!(
        "Expected names and specialized vocabulary: {}.",
        terms.join(", ")
    ))
}

/// Build the `session.update` JSON for a session. The template carries a
/// placeholder `{model}` and a literal `"language": "{language}"` field; this
/// function substitutes the model, replaces the language placeholder with the
/// field the model expects (singular `language`, or the `languages` array for
/// OpenAI's current models) or removes it entirely for auto-detect, writes the
/// dictionary into the provider's vocabulary field, and stamps an `event_id`
/// (optional for OpenAI, required by DashScope).
///
/// Both OpenAI Realtime and Qwen3-ASR-Flash-Realtime treat an absent language
/// as "auto-detect from audio" — same semantics as Standard mode's auto mode.
fn build_session_update(
    template: &str,
    model: &str,
    language: Option<&str>,
    dictionary: &[String],
    vocabulary_field: &VocabularyField,
) -> Result<String> {
    let with_model = template.replace("{model}", model);
    let mut value: serde_json::Value =
        serde_json::from_str(&with_model).context("parse session template")?;

    if let Some(root) = value.as_object_mut() {
        root.insert(
            "event_id".to_string(),
            serde_json::json!(uuid::Uuid::new_v4().to_string()),
        );
    }

    // The transcription config lives at one of two paths depending on provider:
    //   - OpenAI Realtime (GA shape): session.audio.input.transcription
    //   - Qwen3-ASR-Flash-Realtime:   session.input_audio_transcription
    // Both providers' templates put `"language": "{language}"` inside that
    // object; we navigate to it and either set the real value or drop the
    // field (auto-detect).
    let path = if value
        .pointer("/session/audio/input/transcription")
        .is_some()
    {
        "/session/audio/input/transcription"
    } else {
        "/session/input_audio_transcription"
    };
    let transcription = value
        .pointer_mut(path)
        .and_then(|t| t.as_object_mut())
        .ok_or_else(|| anyhow!("session template missing transcription config"))?;
    transcription.remove("language");
    transcription.remove("languages");
    if let Some(lang) = language.filter(|l| !l.is_empty() && *l != "auto") {
        if uses_languages_array(model) {
            transcription.insert("languages".to_string(), serde_json::json!([lang]));
        } else {
            transcription.insert("language".to_string(), serde_json::json!(lang));
        }
    }
    transcription.remove("prompt");
    transcription.remove("corpus");
    if let Some(hint) = vocabulary_hint(dictionary) {
        match vocabulary_field {
            VocabularyField::Prompt => {
                transcription.insert("prompt".to_string(), serde_json::json!(hint));
            }
            VocabularyField::CorpusText => {
                transcription.insert(
                    "corpus".to_string(),
                    serde_json::json!({ "text": hint }),
                );
            }
            VocabularyField::None => {}
        }
    }
    Ok(serde_json::to_string(&value).context("serialize session.update")?)
}

/// Build the JSON string for an `input_audio_buffer.append` event.
/// PCM16 samples are converted to little-endian bytes and base64-encoded.
fn build_audio_event(samples: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let encoded = BASE64.encode(&bytes);
    serde_json::json!({
        "event_id": uuid::Uuid::new_v4().to_string(),
        "type": "input_audio_buffer.append",
        "audio": encoded,
    })
    .to_string()
}

#[async_trait]
impl StreamingTranscriber for RealtimeOpenAiCompatibleClient {
    async fn open(&mut self, cfg: SessionConfig) -> Result<()> {
        // Build URL with model substitution
        let url_str = self.cfg.ws_url_template.replace("{model}", &cfg.model);
        log::info!(
            "[Live] WS connecting: provider={} url={} model={} language={:?}",
            self.cfg.id,
            url_str,
            cfg.model,
            cfg.language
        );
        let mut request = url_str
            .as_str()
            .into_client_request()
            .context("build ws request")?;

        // Add auth header (NEVER in URL query — security audit)
        let auth_value = match self.cfg.auth_scheme {
            AuthScheme::Bearer => format!("Bearer {}", cfg.api_key),
        };
        request
            .headers_mut()
            .insert("Authorization", auth_value.parse().context("auth header")?);

        // Extra headers (e.g., Qwen's OpenAI-Beta)
        for (name, value) in self.cfg.extra_headers {
            request
                .headers_mut()
                .insert(*name, value.parse().context("extra header")?);
        }

        // Cap message size at 1 MB to prevent OOM from malicious server (security audit)
        let mut ws_config = WebSocketConfig::default();
        ws_config.max_message_size = Some(1024 * 1024);
        ws_config.max_frame_size = Some(256 * 1024);
        ws_config.accept_unmasked_frames = false;

        let (stream, _resp): (WsStream, _) =
            connect_async_with_config(request, Some(ws_config), false)
                .await
                .context("ws connect")?;
        log::info!("[Live] WS handshake complete, sending session.update");

        let (sink, source) = stream.split();
        self.sink = Some(sink);
        self.source = Some(source);

        // Build session.update programmatically so we can OMIT the language
        // field when caller passes None — both OpenAI Realtime and Qwen3-ASR
        // auto-detect from audio in that case (parity with Standard mode's
        // whisper.cpp behaviour, which the user explicitly asked for).
        let session_update = build_session_update(
            self.cfg.session_template,
            &cfg.model,
            cfg.language.as_deref(),
            &cfg.dictionary,
            &self.cfg.vocabulary_field,
        )?;
        log::info!(
            "[Live] sending session.update ({} bytes)",
            session_update.len()
        );
        log::debug!("[Live] session.update payload: {}", session_update);
        self.send_text(&session_update).await?;
        Ok(())
    }

    async fn push_pcm16(&mut self, samples: &[i16]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let event = build_audio_event(samples);
        self.send_text(&event).await
    }

    async fn commit_utterance(&mut self) -> Result<()> {
        let evt = serde_json::json!({
            "event_id": uuid::Uuid::new_v4().to_string(),
            "type": "input_audio_buffer.commit",
        });
        self.send_text(&evt.to_string()).await
    }

    async fn end_of_audio(&mut self) -> Result<()> {
        let evt = serde_json::json!({
            "event_id": uuid::Uuid::new_v4().to_string(),
            "type": self.cfg.end_of_audio_event,
        });
        log::info!("[Live] sending end-of-audio event {}", self.cfg.end_of_audio_event);
        self.send_text(&evt.to_string()).await
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut sink) = self.sink.take() {
            let _ = sink.send(Message::Close(None)).await;
        }
        self.source = None;
        Ok(())
    }
}

impl RealtimeOpenAiCompatibleClient {
    async fn send_text(&mut self, text: &str) -> Result<()> {
        let sink = self.sink.as_mut().ok_or_else(|| anyhow!("not connected"))?;
        sink.send(Message::Text(text.to_string()))
            .await
            .context("ws send")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transcription::streaming::providers::{
        OPENAI_REALTIME, QWEN_REALTIME, VocabularyField,
    };

    fn openai_update(model: &str, language: Option<&str>, dictionary: &[String]) -> Value {
        let json = build_session_update(
            OPENAI_REALTIME.session_template,
            model,
            language,
            dictionary,
            &OPENAI_REALTIME.vocabulary_field,
        )
        .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn qwen_update(language: Option<&str>, dictionary: &[String]) -> Value {
        let json = build_session_update(
            QWEN_REALTIME.session_template,
            "qwen3-asr-flash-realtime",
            language,
            dictionary,
            &QWEN_REALTIME.vocabulary_field,
        )
        .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn session_update_adds_openai_vocabulary_prompt() {
        let dictionary = vec![
            "CLAUDE".to_string(),
            "Tauri".to_string(),
            "claude".to_string(),
        ];
        let value = openai_update("gpt-4o-mini-transcribe", Some("en"), &dictionary);
        let transcription = value
            .pointer("/session/audio/input/transcription")
            .unwrap();
        assert_eq!(transcription["language"], "en");
        assert_eq!(
            transcription["prompt"],
            "Expected names and specialized vocabulary: CLAUDE, Tauri."
        );
    }

    /// OpenAI's `gpt-live-transcribe` and `gpt-transcribe` take the plural
    /// `languages` array and the docs say the singular `language` must not be
    /// sent alongside it. The legacy `gpt-4o-*`/`whisper-1` models still take
    /// the singular field (covered above).
    #[test]
    fn session_update_uses_languages_array_for_new_openai_models() {
        for model in ["gpt-live-transcribe", "gpt-transcribe"] {
            let value = openai_update(model, Some("zh"), &[]);
            let transcription = value
                .pointer("/session/audio/input/transcription")
                .unwrap();
            assert_eq!(transcription["model"], model);
            assert_eq!(
                transcription["languages"],
                serde_json::json!(["zh"]),
                "{model} must receive `languages`"
            );
            assert!(
                transcription.get("language").is_none(),
                "{model} must not also receive the singular `language`"
            );
        }
    }

    #[test]
    fn session_update_legacy_model_never_gets_languages_array() {
        let value = openai_update("gpt-4o-transcribe", Some("en"), &[]);
        let transcription = value
            .pointer("/session/audio/input/transcription")
            .unwrap();
        assert_eq!(transcription["language"], "en");
        assert!(transcription.get("languages").is_none());
    }

    #[test]
    fn session_update_auto_detect_omits_both_language_fields() {
        for model in ["gpt-live-transcribe", "gpt-4o-mini-transcribe"] {
            for language in [None, Some("auto"), Some("")] {
                let value = openai_update(model, language, &[]);
                let transcription = value
                    .pointer("/session/audio/input/transcription")
                    .unwrap();
                assert!(transcription.get("language").is_none(), "{model} {language:?}");
                assert!(transcription.get("languages").is_none(), "{model} {language:?}");
            }
        }
    }

    /// Every client event carries an `event_id`; DashScope documents it as
    /// required on `session.update`, OpenAI as optional.
    #[test]
    fn session_update_carries_event_id() {
        let openai = openai_update("gpt-live-transcribe", None, &[]);
        assert!(!openai["event_id"].as_str().unwrap_or("").is_empty());
        let qwen = qwen_update(None, &[]);
        assert!(!qwen["event_id"].as_str().unwrap_or("").is_empty());
    }

    /// Qwen3-ASR-Flash-Realtime's documented `session.update` shape: `pcm` plus a
    /// separate `sample_rate`, language under `input_audio_transcription`, and
    /// server VAD. (The old `transcription_session.update` / `pcm16` shape is no
    /// longer documented.)
    #[test]
    fn qwen_session_update_matches_documented_shape() {
        let value = qwen_update(Some("zh"), &[]);
        assert_eq!(value["type"], "session.update");
        let session = &value["session"];
        assert_eq!(session["input_audio_format"], "pcm");
        assert_eq!(session["sample_rate"], 16_000);
        assert_eq!(session["input_audio_transcription"]["language"], "zh");
        assert_eq!(session["turn_detection"]["type"], "server_vad");
        assert_eq!(session["turn_detection"]["silence_duration_ms"], 400);
    }

    /// Qwen biases recognition through `input_audio_transcription.corpus.text`,
    /// not an OpenAI-style `prompt`.
    #[test]
    fn qwen_session_update_puts_vocabulary_in_corpus_text() {
        let dictionary = vec!["CLAUDE".to_string(), "Tauri".to_string(), "claude".to_string()];
        let value = qwen_update(None, &dictionary);
        let transcription = value
            .pointer("/session/input_audio_transcription")
            .unwrap();
        assert!(transcription.get("language").is_none());
        assert!(transcription.get("prompt").is_none());
        assert_eq!(
            transcription["corpus"]["text"],
            "Expected names and specialized vocabulary: CLAUDE, Tauri."
        );
    }

    #[test]
    fn qwen_session_update_without_vocabulary_has_no_corpus() {
        let value = qwen_update(None, &[]);
        let transcription = value
            .pointer("/session/input_audio_transcription")
            .unwrap();
        assert!(transcription.get("corpus").is_none());
    }

    #[test]
    fn session_update_omits_vocabulary_when_provider_has_no_field() {
        let dictionary = vec!["CLAUDE".to_string()];
        let json = build_session_update(
            QWEN_REALTIME.session_template,
            "qwen3-asr-flash-realtime",
            None,
            &dictionary,
            &VocabularyField::None,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        let transcription = value
            .pointer("/session/input_audio_transcription")
            .unwrap();
        assert!(transcription.get("prompt").is_none());
        assert!(transcription.get("corpus").is_none());
    }

    /// Qwen answers `session.finish` with `session.finished` once the trailing
    /// utterance has been delivered; the soft-flush stops draining on it.
    #[test]
    fn parse_event_session_finished_is_terminal() {
        let json = r#"{"type":"session.finished","event_id":"event_9"}"#;
        assert!(matches!(parse(json), WireEvent::Finished));
    }

    #[tokio::test]
    async fn head_advance_rearms_reorder_timeout() {
        // Regression: a completion that advances the head onto a NEW still-missing
        // rank must restart the head-of-line timer, not inherit the previous
        // block's clock — otherwise the newly-exposed head is skipped early.
        let mut c = RealtimeOpenAiCompatibleClient::new(
            &crate::transcription::streaming::providers::OPENAI_REALTIME,
        );
        c.reorder.observe("a"); // rank 0
        c.reorder.observe("b"); // rank 1 — stays missing
        c.reorder.observe("c"); // rank 2
        // rank 2 completes first → blocked on missing rank 0.
        assert!(c.ingest_completed("c", "C".into()).is_empty());
        let t0 = Instant::now();
        assert!(c.check_reorder_timeout(t0).is_empty()); // arms the timer at t0
        // rank 0 lands → head advances 0→1, still blocked on missing rank 1.
        assert_eq!(c.ingest_completed("a", "A".into()), vec!["A".to_string()]);
        // A full timeout after the ORIGINAL block must NOT skip rank 1: its clock
        // restarted when the head advanced.
        assert!(
            c.check_reorder_timeout(t0 + REORDER_HEAD_TIMEOUT).is_empty(),
            "rank 1's timer must restart on head advance, not inherit rank 0's clock",
        );
        // It DOES fire a full timeout measured from the head advance (rank 1
        // abandoned, rank 2's "C" released).
        let fired = c.check_reorder_timeout(t0 + REORDER_HEAD_TIMEOUT + REORDER_HEAD_TIMEOUT);
        assert_eq!(fired.len(), 1);
    }

    /// Parse a raw JSON event string the way `poll_event` does (decode → dispatch).
    fn parse(json: &str) -> WireEvent {
        parse_event(&serde_json::from_str(json).unwrap()).unwrap()
    }

    #[test]
    fn parse_event_completed_carries_item_id_and_transcript() {
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"itm_1","transcript":"hello world"}"#;
        match parse(json) {
            WireEvent::Completed { item_id, transcript } => {
                assert_eq!(item_id, "itm_1");
                assert_eq!(transcript, "hello world");
            }
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_empty_transcript_still_completes() {
        // Empty transcripts now flow through as `Completed` so the reorder buffer
        // can advance its head past a silent segment; the buffer (not the parser)
        // decides to emit nothing.
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"itm_1","transcript":""}"#;
        match parse(json) {
            WireEvent::Completed { transcript, .. } => assert_eq!(transcript, ""),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_committed_is_ranking_signal() {
        let json = r#"{"type":"input_audio_buffer.committed","item_id":"itm_7","previous_item_id":"itm_6"}"#;
        match parse(json) {
            WireEvent::Committed { item_id } => assert_eq!(item_id, "itm_7"),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_speech_started_is_ranking_signal() {
        let json = r#"{"type":"input_audio_buffer.speech_started","item_id":"itm_8","audio_start_ms":120}"#;
        match parse(json) {
            WireEvent::Committed { item_id } => assert_eq!(item_id, "itm_8"),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_classifies_auth_error() {
        let json = r#"{"type":"error","error":{"message":"invalid key","code":"invalid_api_key"}}"#;
        match parse(json) {
            WireEvent::Error { kind: ErrorKind::AuthFailed, .. } => {}
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_ignores_unknown_types() {
        let json = r#"{"type":"session.created","session":{"id":"x"}}"#;
        assert!(matches!(parse(json), WireEvent::Ignored));
    }

    #[test]
    fn push_pcm16_event_shape() {
        // We can't run an actual WS round-trip here; instead, test the JSON shape
        // by extracting the build_audio_event helper.
        let samples = [0i16, 1, -1, i16::MAX, i16::MIN];
        let json = build_audio_event(&samples);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "input_audio_buffer.append");
        assert!(v["event_id"].as_str().unwrap().len() > 0);
        let b64 = v["audio"].as_str().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        assert_eq!(decoded.len(), samples.len() * 2);
        // Little-endian i16
        assert_eq!(decoded[0], 0);
        assert_eq!(decoded[2], 1);
    }
}
