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
use super::{SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsSource = SplitStream<WsStream>;

pub struct RealtimeOpenAiCompatibleClient {
    cfg: &'static ProviderConfig,
    sink: Option<WsSink>,
    source: Option<WsSource>,
    utterance_seq: u32,
}

impl RealtimeOpenAiCompatibleClient {
    pub fn new(provider: &'static ProviderConfig) -> Self {
        Self {
            cfg: provider,
            sink: None,
            source: None,
            utterance_seq: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.cfg.audio_sample_rate
    }

    pub fn vad_mode(&self) -> &VadMode {
        &self.cfg.vad_mode
    }

    /// Read one event from the WebSocket. Returns Ok(Some(event)) when a real event arrives,
    /// Ok(None) on Ping/Pong/binary/non-event text, and Err on transport failure or close.
    pub async fn poll_event(&mut self) -> Result<Option<StreamingEvent>> {
        let source = self.source.as_mut().ok_or_else(|| anyhow!("not connected"))?;
        let Some(msg) = source.next().await else {
            return Err(anyhow!("websocket closed"));
        };
        let msg = msg.context("ws read")?;
        match msg {
            Message::Text(text) => Self::parse_event(&text, &mut self.utterance_seq),
            Message::Close(_) => Err(anyhow!("websocket closed by server")),
            _ => Ok(None),
        }
    }

    fn parse_event(text: &str, utterance_seq: &mut u32) -> Result<Option<StreamingEvent>> {
        let v: Value = serde_json::from_str(text).context("parse event json")?;
        let evt_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match evt_type {
            "conversation.item.input_audio_transcription.completed"
            | "transcription_session.completed" /* fallback variant some providers emit */ => {
                let transcript = v
                    .get("transcript")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if transcript.is_empty() {
                    return Ok(None);
                }
                *utterance_seq += 1;
                Ok(Some(StreamingEvent::UtteranceCompleted {
                    text: transcript,
                    utterance_seq: *utterance_seq,
                }))
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
                Ok(Some(StreamingEvent::Error { message, kind }))
            }
            _ => Ok(None),
        }
    }
}

fn classify_error(code: &str) -> ErrorKind {
    match code {
        "invalid_api_key" | "unauthorized" | "auth_failed" => ErrorKind::AuthFailed,
        "rate_limit_exceeded" | "rate_limited" => ErrorKind::RateLimited,
        c if c.starts_with("server_") => ErrorKind::ServerError,
        _ => ErrorKind::BadResponse,
    }
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

        let (stream, _resp): (WsStream, _) = connect_async_with_config(request, Some(ws_config), false)
            .await
            .context("ws connect")?;

        let (sink, source) = stream.split();
        self.sink = Some(sink);
        self.source = Some(source);

        // Substitute model + language in the session template, then send
        let language = cfg.language.clone().unwrap_or_else(|| "en".to_string());
        let session_update = self
            .cfg
            .session_template
            .replace("{model}", &cfg.model)
            .replace("{language}", &language);
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
    fn parse_event_recognizes_completed_transcript() {
        let mut seq = 0u32;
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello world"}"#;
        let evt = RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().unwrap();
        match evt {
            StreamingEvent::UtteranceCompleted { text, utterance_seq } => {
                assert_eq!(text, "hello world");
                assert_eq!(utterance_seq, 1);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[test]
    fn parse_event_skips_empty_transcript() {
        let mut seq = 0u32;
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":""}"#;
        assert!(RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().is_none());
    }

    #[test]
    fn parse_event_classifies_auth_error() {
        let mut seq = 0u32;
        let json = r#"{"type":"error","error":{"message":"invalid key","code":"invalid_api_key"}}"#;
        let evt = RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().unwrap();
        match evt {
            StreamingEvent::Error { kind: ErrorKind::AuthFailed, .. } => {}
            _ => panic!("wrong error kind"),
        }
    }

    #[test]
    fn parse_event_ignores_unknown_types() {
        let mut seq = 0u32;
        let json = r#"{"type":"session.created","session":{"id":"x"}}"#;
        assert!(RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().is_none());
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
