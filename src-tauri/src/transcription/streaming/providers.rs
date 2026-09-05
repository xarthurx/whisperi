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

/// Where a provider accepts custom vocabulary / context for recognition
/// biasing inside its `session.update` transcription config.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum VocabularyField {
    /// Provider has no biasing field; the dictionary is not sent.
    None,
    /// OpenAI: free-text `transcription.prompt`.
    Prompt,
    /// Alibaba Qwen3-ASR: `input_audio_transcription.corpus.text`, a context
    /// text (background, entity vocabulary) of up to 10,000 tokens.
    CorpusText,
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
    /// Which `session.update` field (if any) carries the user's dictionary.
    pub vocabulary_field: VocabularyField,
    /// Client event type sent once at stop to flush whatever the server is
    /// still holding: OpenAI commits the buffer, DashScope ends the session
    /// (its `input_audio_buffer.commit` is rejected in server-VAD mode and the
    /// in-progress utterance is discarded unless `session.finish` is sent).
    pub end_of_audio_event: &'static str,
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
    // `gpt-live-transcribe` is the model OpenAI's realtime-transcription guide
    // recommends; the whole `gpt-4o-*-transcribe` family (and `whisper-1`) was
    // deprecated on 2026-08-26 with API removal on 2027-02-26. Sent via the GA
    // `session.audio.input.transcription` shape with
    // `session.type: "transcription"` in session.update.
    default_model: "gpt-live-transcribe",
    audio_sample_rate: 24_000,
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[],
    vad_mode: VadMode::ServerVad { silence_ms: 500 },
    vocabulary_field: VocabularyField::Prompt,
    end_of_audio_event: "input_audio_buffer.commit",
    session_template: include_str!("session_templates/openai.json"),
};

pub static QWEN_REALTIME: ProviderConfig = ProviderConfig {
    id: "qwen",
    display_name: "Qwen3-ASR-Flash-Realtime",
    // Model Studio's newer `qwen-audio-3.0-asr-flash-streaming` speaks the
    // DashScope-native `run-task` protocol, not this OpenAI-style wire, so the
    // `qwen3-asr-flash-realtime` series (still current, snapshots through
    // 2026-02-10) stays the model for this adapter. The legacy
    // `dashscope-intl` host remains supported; the recommended
    // `{WorkspaceId}.ap-southeast-1.maas.aliyuncs.com` host needs a workspace id
    // the app does not collect.
    ws_url_template: "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime?model={model}",
    default_model: "qwen3-asr-flash-realtime",
    audio_sample_rate: 16_000,
    auth_scheme: AuthScheme::Bearer,
    // DashScope documents only Authorization plus optional workspace /
    // data-inspection headers; the Realtime beta header is not part of it.
    extra_headers: &[],
    vad_mode: VadMode::ServerVad { silence_ms: 400 },
    vocabulary_field: VocabularyField::CorpusText,
    end_of_audio_event: "session.finish",
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
        // OpenAI's recommended realtime transcription model since the 2026-08-26
        // deprecation of the gpt-4o-*-transcribe family (shutdown 2027-02-26).
        assert_eq!(cfg.default_model, "gpt-live-transcribe");
        assert!(matches!(cfg.vad_mode, VadMode::ServerVad { silence_ms: 500 }));
        assert_eq!(cfg.vocabulary_field, VocabularyField::Prompt);
        // Server VAD commits turns itself; the stop soft-flush still commits so
        // trailing speech VAD hasn't closed yet is transcribed.
        assert_eq!(cfg.end_of_audio_event, "input_audio_buffer.commit");
        // Transcription-only mode requires `?intent=transcription`
        assert!(cfg.ws_url_template.contains("intent=transcription"));
    }

    #[test]
    fn lookup_returns_qwen_config() {
        let cfg = lookup("qwen").unwrap();
        assert_eq!(cfg.audio_sample_rate, 16_000);
        assert!(matches!(cfg.vad_mode, VadMode::ServerVad { silence_ms: 400 }));
        // DashScope's documented request headers are Authorization (+ optional
        // workspace/data-inspection); the Realtime *beta* header is gone.
        assert!(cfg.extra_headers.is_empty());
        // Qwen3-ASR biases recognition via `input_audio_transcription.corpus.text`.
        assert_eq!(cfg.vocabulary_field, VocabularyField::CorpusText);
        // In VAD mode `input_audio_buffer.commit` is disabled; the session must be
        // ended with `session.finish` or the in-progress utterance is discarded.
        assert_eq!(cfg.end_of_audio_event, "session.finish");
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("groq").is_none());
    }
}
