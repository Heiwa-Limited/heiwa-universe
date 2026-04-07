use heiwa_protocol::{
    CockpitCommand, CockpitEvent, RoutingState, SessionState, TranscriptBlock,
};
use tokio::sync::mpsc;

fn test_session() -> SessionState {
    SessionState {
        session_id: "test".into(),
        transcript: vec![],
        routing: RoutingState {
            current_provider: "ollama".into(),
            current_model: "qwen3:8b".into(),
            mode: "Auto".into(),
            explanation: None,
        },
        devices: vec![],
        receipts: vec![],
    }
}

#[test]
fn cockpit_event_variants_are_constructible() {
    // Ensures the event contract compiles and all variants are usable.
    let _events: Vec<CockpitEvent> = vec![
        CockpitEvent::TranscriptAppend(TranscriptBlock::User("hello".into())),
        CockpitEvent::TranscriptAppend(TranscriptBlock::Assistant("world".into())),
        CockpitEvent::TranscriptAppend(TranscriptBlock::Tool("shell".into(), "ok".into())),
        CockpitEvent::TranscriptAppend(TranscriptBlock::Evidence("route ok".into())),
        CockpitEvent::RoutingUpdate(RoutingState {
            current_provider: "claude".into(),
            current_model: "opus-4.6".into(),
            mode: "provider:claude".into(),
            explanation: Some("user pinned".into()),
        }),
        CockpitEvent::StreamToken("tok".into()),
        CockpitEvent::StreamDone {
            tokens_in: 100,
            tokens_out: 200,
            cost: 0.01,
        },
        CockpitEvent::StreamError("timeout".into()),
        CockpitEvent::StatusUpdate("ready".into()),
    ];
}

#[test]
fn cockpit_command_variants_are_constructible() {
    let _cmds: Vec<CockpitCommand> = vec![
        CockpitCommand::SubmitInput("hello world".into()),
        CockpitCommand::Quit,
    ];
}

#[tokio::test]
async fn events_round_trip_through_channel() {
    let (tx, mut rx) = mpsc::unbounded_channel::<CockpitEvent>();

    tx.send(CockpitEvent::StatusUpdate("routing...".into()))
        .unwrap();
    tx.send(CockpitEvent::StreamToken("Hello".into())).unwrap();
    tx.send(CockpitEvent::StreamDone {
        tokens_in: 10,
        tokens_out: 20,
        cost: 0.001,
    })
    .unwrap();

    let mut received = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        received.push(ev);
    }

    assert_eq!(received.len(), 3);
    assert!(matches!(received[0], CockpitEvent::StatusUpdate(_)));
    assert!(matches!(received[1], CockpitEvent::StreamToken(_)));
    assert!(matches!(
        received[2],
        CockpitEvent::StreamDone {
            tokens_in: 10,
            ..
        }
    ));
}

#[tokio::test]
async fn commands_round_trip_through_channel() {
    let (tx, mut rx) = mpsc::unbounded_channel::<CockpitCommand>();

    tx.send(CockpitCommand::SubmitInput("fix the bug".into()))
        .unwrap();
    tx.send(CockpitCommand::Quit).unwrap();

    let cmd1 = rx.recv().await.unwrap();
    assert!(matches!(cmd1, CockpitCommand::SubmitInput(ref s) if s == "fix the bug"));

    let cmd2 = rx.recv().await.unwrap();
    assert!(matches!(cmd2, CockpitCommand::Quit));
}

#[test]
fn session_state_serialization_round_trip() {
    let mut session = test_session();
    session
        .transcript
        .push(TranscriptBlock::User("hello".into()));
    session
        .transcript
        .push(TranscriptBlock::Assistant("world".into()));

    let json = serde_json::to_string(&session).unwrap();
    let deserialized: SessionState = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.session_id, "test");
    assert_eq!(deserialized.transcript.len(), 2);
    assert_eq!(deserialized.routing.current_provider, "ollama");
}

#[test]
fn parse_turn_intent_extracts_provider_and_model() {
    let req = heiwa_protocol::parse_turn_intent("use claude opus-4.6 fix the bug");
    assert_eq!(req.provider_pin.as_deref(), Some("claude"));
    assert_eq!(req.model_pin.as_deref(), Some("opus-4.6"));
    assert!(matches!(req.intent, heiwa_protocol::Intent::Code));
}

#[test]
fn parse_turn_intent_defaults_to_chat() {
    let req = heiwa_protocol::parse_turn_intent("what is the weather like");
    assert!(req.provider_pin.is_none());
    assert!(req.model_pin.is_none());
    assert!(matches!(req.intent, heiwa_protocol::Intent::Chat));
}
