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

use super::providers::{AuthScheme, ProviderConfig, VadMode};
use super::reorder::ReorderBuffer;
use super::{SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind};
use std::collections::VecDeque;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsSource = SplitStream<WsStream>;

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
        // INFO: type + size only (no transcript content — that's user speech and
        // shouldn't land in production logs). DEBUG: full payload (truncated at
        // 600 bytes on a char boundary).
        let evt_type = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
            .unwrap_or_else(|| "<unparseable>".into());
        log::info!("[Live] WS recv type={} ({} bytes)", evt_type, text.len());
        log::debug!(
            "[Live] WS recv payload: {}",
            truncate_at_char_boundary(&text, 600)
        );

        match parse_event(&text)? {
            WireEvent::Committed { item_id } => {
                // Capture-order ranking signal — assign the rank now, emit nothing.
                self.reorder.observe(&item_id);
                Ok(None)
            }
            WireEvent::Completed { item_id, transcript } => {
                let released = self.reorder.complete(&item_id, transcript);
                let events = self.wrap_completed(released);
                self.ready.extend(events);
                Ok(self.ready.pop_front())
            }
            WireEvent::Error { message, kind } => {
                Ok(Some(StreamingEvent::Error { message, kind }))
            }
            WireEvent::Ignored => Ok(None),
        }
    }

    /// True while a completion is held behind an earlier utterance that hasn't
    /// finished transcribing — the caller arms its skip-head timeout on this.
    pub fn reorder_blocked(&self) -> bool {
        self.reorder.is_blocked()
    }

    /// Abandon a missing head after the caller's timeout and release whatever the
    /// reorder buffer can now surface, in spoken order.
    pub fn skip_reorder_head(&mut self) -> Vec<StreamingEvent> {
        let released = self.reorder.skip_head();
        self.wrap_completed(released)
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
    /// A message we don't act on (session.created, deltas, pings, …).
    Ignored,
}

fn item_id_of(v: &Value) -> Option<String> {
    v.get("item_id").and_then(|i| i.as_str()).map(str::to_string)
}

/// Decode a single provider message into a [`WireEvent`]. Pure — all ordering
/// and sequence-number state lives in the client, not here.
fn parse_event(text: &str) -> Result<WireEvent> {
    let v: Value = serde_json::from_str(text).context("parse event json")?;
    let evt_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match evt_type {
        // Capture-order signals: the provider emits these sequentially as the
        // user speaks, BEFORE transcription completes, so first-sight order is
        // spoken order. Both are observed because providers differ in which they
        // send; whichever names an item_id first sets its rank (idempotent).
        "input_audio_buffer.committed" | "input_audio_buffer.speech_started" => {
            match item_id_of(&v) {
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
            let item_id = item_id_of(&v)
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

/// Build the `session.update` JSON for a session. The template carries a
/// placeholder `{model}` and a literal `"language": "{language}"` field; this
/// function substitutes the model and either substitutes a real language code
/// or removes the language field entirely (for auto-detect).
///
/// Both OpenAI Realtime and Qwen3-ASR-Flash-Realtime treat an absent
/// `language` as "auto-detect from audio" — same semantics as Standard mode's
/// whisper.cpp without `-l`.
fn build_session_update(
    template: &str,
    model: &str,
    language: Option<&str>,
) -> Result<String> {
    let with_model = template.replace("{model}", model);
    let mut value: serde_json::Value =
        serde_json::from_str(&with_model).context("parse session template")?;

    // The transcription config lives at one of two paths depending on provider:
    //   - OpenAI Realtime (new shape): session.audio.input.transcription
    //   - Qwen / OpenAI Realtime (legacy shape): session.input_audio_transcription
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
    match language {
        Some(lang) if !lang.is_empty() && lang != "auto" => {
            transcription.insert("language".to_string(), serde_json::json!(lang));
        }
        _ => {
            transcription.remove("language");
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
        let session_update =
            build_session_update(self.cfg.session_template, &cfg.model, cfg.language.as_deref())?;
        log::info!("[Live] session.update payload: {}", session_update);
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

    #[test]
    fn parse_event_completed_carries_item_id_and_transcript() {
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"itm_1","transcript":"hello world"}"#;
        match parse_event(json).unwrap() {
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
        match parse_event(json).unwrap() {
            WireEvent::Completed { transcript, .. } => assert_eq!(transcript, ""),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_committed_is_ranking_signal() {
        let json = r#"{"type":"input_audio_buffer.committed","item_id":"itm_7","previous_item_id":"itm_6"}"#;
        match parse_event(json).unwrap() {
            WireEvent::Committed { item_id } => assert_eq!(item_id, "itm_7"),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_speech_started_is_ranking_signal() {
        let json = r#"{"type":"input_audio_buffer.speech_started","item_id":"itm_8","audio_start_ms":120}"#;
        match parse_event(json).unwrap() {
            WireEvent::Committed { item_id } => assert_eq!(item_id, "itm_8"),
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_classifies_auth_error() {
        let json = r#"{"type":"error","error":{"message":"invalid key","code":"invalid_api_key"}}"#;
        match parse_event(json).unwrap() {
            WireEvent::Error { kind: ErrorKind::AuthFailed, .. } => {}
            e => panic!("wrong variant: {:?}", e),
        }
    }

    #[test]
    fn parse_event_ignores_unknown_types() {
        let json = r#"{"type":"session.created","session":{"id":"x"}}"#;
        assert!(matches!(parse_event(json).unwrap(), WireEvent::Ignored));
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
