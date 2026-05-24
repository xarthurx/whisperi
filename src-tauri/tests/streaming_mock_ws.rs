//! Integration tests for the realtime streaming client against a mock WS server.
//! No network calls; everything runs on localhost.

use std::net::SocketAddr;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use whisperi_lib::transcription::streaming::{SessionConfig, StreamingEvent, StreamingTranscriber};
use whisperi_lib::transcription::streaming::realtime_openai_compatible::RealtimeOpenAiCompatibleClient;
use whisperi_lib::transcription::streaming::providers::{ProviderConfig, AuthScheme, VadMode, OPENAI_REALTIME};

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
        assert!(msg.to_text().unwrap().contains("transcription_session.update"));
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

/// Build a `&'static ProviderConfig` pointing at the local mock server.
/// We leak a `Box` here because `ProviderConfig::ws_url_template` is `&'static str`;
/// in tests this is acceptable.
fn test_provider_for(addr: SocketAddr) -> &'static ProviderConfig {
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
        session_template: OPENAI_REALTIME.session_template,
    }))
}
