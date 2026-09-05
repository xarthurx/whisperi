//! Integration tests for the realtime streaming client against a mock WS server.
//! No network calls; everything runs on localhost.

use std::net::SocketAddr;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use whisperi_lib::transcription::streaming::{SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind};
use whisperi_lib::transcription::streaming::realtime_openai_compatible::RealtimeOpenAiCompatibleClient;
use whisperi_lib::transcription::streaming::providers::{
    AuthScheme, OPENAI_REALTIME, ProviderConfig, QWEN_REALTIME, VadMode, VocabularyField,
};

/// Spin up a one-shot mock WS server. The handler receives the connection
/// and runs the provided fn; on its first call to listener.accept() the
/// fn body runs until completion.
async fn mock_server<F, Fut>(handler: F) -> SocketAddr
where
    F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = accept_async(stream).await.unwrap();
        handler(ws).await;
    });
    addr
}

#[tokio::test]
async fn happy_path_completed_event_propagates() {
    let addr = mock_server(|mut ws| async move {
        // Expect session.update first
        let msg = ws.next().await.unwrap().unwrap();
        let txt = msg.to_text().unwrap();
        assert!(txt.contains("\"type\":\"session.update\""), "got: {}", txt);
        // Send one completed utterance
        ws.send(Message::Text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello there"}"#
                .to_string(),
        ))
        .await
        .unwrap();
    })
    .await;

    // Build a custom provider config pointing at our mock server
    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-fake".to_string(),
        })
        .await
        .unwrap();

    let event = client.poll_event().await.unwrap().unwrap();
    match event {
        StreamingEvent::UtteranceCompleted { text, utterance_seq } => {
            assert_eq!(text, "hello there");
            assert_eq!(utterance_seq, 1);
        }
        e => panic!("unexpected event: {:?}", e),
    }
}

/// The bug this whole feature fixes: with server-side VAD the provider
/// transcribes each committed segment asynchronously, so a short utterance
/// spoken *after* a long one can have its `.completed` arrive *first*. The client
/// must restore spoken (commit) order before surfacing utterances, keying off the
/// `item_id` carried by `input_audio_buffer.committed`.
#[tokio::test]
async fn out_of_order_completions_surface_in_spoken_order() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await; // consume session.update
        // Spoken order: A (long) committed first, then B (short).
        // Completion order: B finishes first (short), then A.
        for ev in [
            r#"{"type":"input_audio_buffer.committed","item_id":"A"}"#,
            r#"{"type":"input_audio_buffer.committed","item_id":"B"}"#,
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"B","transcript":"world"}"#,
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"A","transcript":"hello"}"#,
        ] {
            ws.send(Message::Text(ev.to_string())).await.unwrap();
        }
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-fake".to_string(),
        })
        .await
        .unwrap();

    // Collect surfaced utterances; commit/ranking signals return Ok(None).
    let mut texts = Vec::new();
    while texts.len() < 2 {
        if let Some(StreamingEvent::UtteranceCompleted { text, .. }) =
            client.poll_event().await.unwrap()
        {
            texts.push(text);
        }
    }
    // Spoken order (A then B), NOT completion order (B then A).
    assert_eq!(texts, vec!["hello", "world"]);
}

/// Build a `&'static ProviderConfig` pointing at the local mock server.
/// We leak a `Box` here because `ProviderConfig::ws_url_template` is `&'static str`;
/// in tests this is acceptable.
fn test_provider_for(addr: SocketAddr) -> &'static ProviderConfig {
    test_provider_with(addr, OPENAI_REALTIME.session_template, "input_audio_buffer.commit")
}

/// Like [`test_provider_for`], but with the given session template and
/// end-of-audio event, so a test can exercise the Qwen-style
/// `session.finish` / `session.finished` handshake against the mock.
fn test_provider_with(
    addr: SocketAddr,
    session_template: &'static str,
    end_of_audio_event: &'static str,
) -> &'static ProviderConfig {
    let url = Box::leak(format!("ws://{}/", addr).into_boxed_str());
    Box::leak(Box::new(ProviderConfig {
        id: "test",
        display_name: "Test",
        ws_url_template: url,
        default_model: "test-model",
        audio_sample_rate: 16_000,
        auth_scheme: AuthScheme::Bearer,
        extra_headers: &[],
        vad_mode: VadMode::ManualCommit,
        vocabulary_field: VocabularyField::Prompt,
        end_of_audio_event,
        session_template,
    }))
}

#[tokio::test]
async fn auth_failure_emits_auth_failed_error() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await; // consume session.update
        ws.send(Message::Text(
            r#"{"type":"error","error":{"message":"bad key","code":"invalid_api_key"}}"#.to_string(),
        ))
        .await
        .unwrap();
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-bad".to_string(),
        })
        .await
        .unwrap();

    let event = client.poll_event().await.unwrap().unwrap();
    match event {
        StreamingEvent::Error {
            kind: ErrorKind::AuthFailed,
            ..
        } => {}
        e => panic!("expected AuthFailed, got {:?}", e),
    }
}

#[tokio::test]
async fn server_close_propagates_error() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await; // consume session.update
        ws.send(Message::Close(None)).await.unwrap();
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-x".to_string(),
        })
        .await
        .unwrap();

    let result = client.poll_event().await;
    assert!(result.is_err(), "expected error on server close");
}

#[tokio::test]
async fn max_message_size_enforced() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await;
        // 2 MB string — exceeds 1 MB cap
        let huge = "x".repeat(2 * 1024 * 1024);
        let payload = format!(r#"{{"type":"junk","payload":"{}"}}"#, huge);
        let _ = ws.send(Message::Text(payload.into())).await;
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-x".to_string(),
        })
        .await
        .unwrap();

    let result = client.poll_event().await;
    assert!(result.is_err(), "expected error on oversize message");
}

/// Soft-flush boundary (client side). When a Live session stops, the audio-pump
/// task sends the provider's end-of-audio event (`input_audio_buffer.commit` for
/// OpenAI) and then drains trailing utterance events for up to ~800ms
/// (commands/live.rs). The provider may finalise the last utterance a little
/// AFTER the commit. This test pins the client-side behaviour that drain relies
/// on: a delayed `.completed` arriving after the commit is still received by
/// `poll_event()` and surfaces as the final utterance. (The 800ms drain / 1.5s
/// join wall-clock timers live in the Tauri command and are not unit-testable
/// here without a Tauri runtime.)
#[tokio::test]
async fn commit_then_delayed_completed_is_drained() {
    let addr = mock_server(|mut ws| async move {
        // Expect session.update first.
        let msg = ws.next().await.unwrap().unwrap();
        assert!(
            msg.to_text().unwrap().contains("\"type\":\"session.update\""),
            "got: {}",
            msg.to_text().unwrap()
        );
        // Expect the soft-flush commit message.
        let commit = ws.next().await.unwrap().unwrap();
        let commit_txt = commit.to_text().unwrap();
        assert!(
            commit_txt.contains("\"type\":\"input_audio_buffer.commit\""),
            "expected input_audio_buffer.commit, got: {}",
            commit_txt
        );
        // Simulate provider finalisation latency, then send the trailing
        // utterance — this is the event the soft-flush exists to capture.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ws.send(Message::Text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"final words"}"#
                .to_string(),
        ))
        .await
        .unwrap();
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "test-model".to_string(),
            language: Some("en".to_string()),
            dictionary: Vec::new(),
            api_key: "sk-fake".to_string(),
        })
        .await
        .unwrap();

    // Soft-flush sequence: end-of-audio (commit for OpenAI), then drain the
    // delayed trailing utterance.
    client.end_of_audio().await.unwrap();
    let event = client.poll_event().await.unwrap().unwrap();
    match event {
        StreamingEvent::UtteranceCompleted {
            text,
            utterance_seq,
        } => {
            assert_eq!(text, "final words");
            assert_eq!(utterance_seq, 1);
        }
        e => panic!("expected trailing UtteranceCompleted, got {:?}", e),
    }
}

/// DashScope soft-flush handshake. In server-VAD mode Qwen3-ASR rejects
/// `input_audio_buffer.commit`; the client must send `session.finish`, after
/// which the server delivers the in-progress utterance and then
/// `session.finished`. The pump's drain loop stops on that terminal event
/// (surfaced as `SessionClosed`) instead of waiting out its 800ms deadline.
#[tokio::test]
async fn session_finish_drains_trailing_utterance_then_closes() {
    let addr = mock_server(|mut ws| async move {
        // Expect the Qwen-shaped session.update first.
        let msg = ws.next().await.unwrap().unwrap();
        let txt = msg.to_text().unwrap();
        assert!(txt.contains("\"type\":\"session.update\""), "got: {}", txt);
        assert!(txt.contains("\"input_audio_format\":\"pcm\""), "got: {}", txt);
        assert!(txt.contains("\"sample_rate\":16000"), "got: {}", txt);
        // Expect the end-of-audio event to be session.finish, not a commit.
        let finish = ws.next().await.unwrap().unwrap();
        let finish_txt = finish.to_text().unwrap();
        assert!(
            finish_txt.contains("\"type\":\"session.finish\""),
            "expected session.finish, got: {}",
            finish_txt
        );
        ws.send(Message::Text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"A","transcript":"trailing words"}"#
                .to_string(),
        ))
        .await
        .unwrap();
        ws.send(Message::Text(
            r#"{"type":"session.finished","event_id":"event_1"}"#.to_string(),
        ))
        .await
        .unwrap();
    })
    .await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_with(
        addr,
        QWEN_REALTIME.session_template,
        "session.finish",
    ));
    client
        .open(SessionConfig {
            provider_id: "test",
            model: "qwen3-asr-flash-realtime".to_string(),
            language: None,
            dictionary: Vec::new(),
            api_key: "sk-fake".to_string(),
        })
        .await
        .unwrap();

    client.end_of_audio().await.unwrap();
    match client.poll_event().await.unwrap().unwrap() {
        StreamingEvent::UtteranceCompleted { text, .. } => assert_eq!(text, "trailing words"),
        e => panic!("expected trailing UtteranceCompleted, got {:?}", e),
    }
    assert!(
        matches!(
            client.poll_event().await.unwrap(),
            Some(StreamingEvent::SessionClosed)
        ),
        "session.finished must surface as SessionClosed so the drain loop stops"
    );
}
