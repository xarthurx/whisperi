//! Provider configuration registry.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum AuthScheme {
    Bearer,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum VadMode {
    /// Caller invokes `commit_utterance()` to mark utterance boundaries.
    /// Used for OpenAI `gpt-realtime-whisper`.
    ManualCommit,
    /// Server-side voice activity detection.
    ServerVad { silence_ms: u32 },
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub id: &'static str,
    pub display_name: &'static str,
    /// WebSocket URL template. `{model}` is substituted at runtime if present.
    pub ws_url_template: &'static str,
    pub default_model: &'static str,
    pub audio_sample_rate: u32,
    pub auth_scheme: AuthScheme,
    pub extra_headers: &'static [(&'static str, &'static str)],
    pub vad_mode: VadMode,
    /// Whether this provider accepts a transcription `prompt` in its
    /// `session.update` payload for vocabulary/context biasing.
    pub supports_transcription_prompt: bool,
    /// Raw JSON template for the initial `session.update` event.
    /// `{model}` and `{language}` are substituted by the adapter.
    pub session_template: &'static str,
}

pub static OPENAI_REALTIME: ProviderConfig = ProviderConfig {
    id: "openai",
    display_name: "OpenAI Realtime",
    // `?intent=transcription` is undocumented but mandatory to put the session
    // into transcription-only mode. Without it the server treats the
    // connection as conversational and rejects session.update with
    // `missing_model` because it expects a different shape. We must NOT
    // include `?model=...` here — transcription sessions reject that.
    ws_url_template: "wss://api.openai.com/v1/realtime?intent=transcription",
    // `gpt-4o-mini-transcribe` is OpenAI's documented streaming transcription
    // model. Sent via the new `session.audio.input.transcription` shape with
    // `session.type: "transcription"` in session.update, which is what the
    // server's session.created event confirms it expects.
    default_model: "gpt-4o-mini-transcribe",
    audio_sample_rate: 24_000,
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[],
    vad_mode: VadMode::ServerVad { silence_ms: 500 },
    supports_transcription_prompt: true,
    session_template: include_str!("session_templates/openai.json"),
};

pub static QWEN_REALTIME: ProviderConfig = ProviderConfig {
    id: "qwen",
    display_name: "Qwen3-ASR-Flash-Realtime",
    ws_url_template: "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime?model={model}",
    default_model: "qwen3-asr-flash-realtime",
    audio_sample_rate: 16_000,
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[("OpenAI-Beta", "realtime=v1")],
    vad_mode: VadMode::ServerVad { silence_ms: 400 },
    supports_transcription_prompt: false,
    session_template: include_str!("session_templates/qwen.json"),
};

pub fn lookup(id: &str) -> Option<&'static ProviderConfig> {
    match id {
        "openai" => Some(&OPENAI_REALTIME),
        "qwen" => Some(&QWEN_REALTIME),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_openai_config() {
        let cfg = lookup("openai").unwrap();
        assert_eq!(cfg.audio_sample_rate, 24_000);
        assert_eq!(cfg.default_model, "gpt-4o-mini-transcribe");
        assert!(matches!(cfg.vad_mode, VadMode::ServerVad { silence_ms: 500 }));
        assert!(cfg.supports_transcription_prompt);
        // Transcription-only mode requires `?intent=transcription`
        assert!(cfg.ws_url_template.contains("intent=transcription"));
    }

    #[test]
    fn lookup_returns_qwen_config() {
        let cfg = lookup("qwen").unwrap();
        assert_eq!(cfg.audio_sample_rate, 16_000);
        assert!(matches!(cfg.vad_mode, VadMode::ServerVad { silence_ms: 400 }));
        assert_eq!(cfg.extra_headers, &[("OpenAI-Beta", "realtime=v1")]);
        assert!(!cfg.supports_transcription_prompt);
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("groq").is_none());
    }
}
