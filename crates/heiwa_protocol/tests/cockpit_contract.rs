use heiwa_protocol::{CockpitCommand, CockpitEvent, RoutingState, SessionState, TranscriptBlock};
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
        CockpitEvent::StreamDone { tokens_in: 10, .. }
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
    assert!(matches!(req.intent, heiwa_protocol::Intent::Build));
}

#[test]
fn parse_turn_intent_defaults_to_chat() {
    let req = heiwa_protocol::parse_turn_intent("good morning everyone");
    assert!(req.provider_pin.is_none());
    assert!(req.model_pin.is_none());
    assert!(matches!(req.intent, heiwa_protocol::Intent::Chat));
}

#[test]
fn parse_turn_intent_build_variants() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("refactor the router module").intent,
        Intent::Build
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("fix the failing cargo test").intent,
        Intent::Build
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("implement the new adapter").intent,
        Intent::Build
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("patch main.rs").intent,
        Intent::Build
    );
}

#[test]
fn parse_turn_intent_deploy() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("deploy to railway").intent,
        Intent::Deploy
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("ship the release").intent,
        Intent::Deploy
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("publish the docker image").intent,
        Intent::Deploy
    );
}

#[test]
fn parse_turn_intent_audit() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("review the PR").intent,
        Intent::Audit
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("lint this file").intent,
        Intent::Audit
    );
}

#[test]
fn parse_turn_intent_research() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("explain how DREX works").intent,
        Intent::Research
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("what is the difference between X and Y").intent,
        Intent::Research
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("summarize the last meeting").intent,
        Intent::Research
    );
}

#[test]
fn parse_turn_intent_strategy() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("plan the roadmap").intent,
        Intent::Strategy
    );
    assert_eq!(
        heiwa_protocol::parse_turn_intent("design the architecture").intent,
        Intent::Strategy
    );
}

#[test]
fn parse_turn_intent_status_check() {
    use heiwa_protocol::Intent;
    assert_eq!(
        heiwa_protocol::parse_turn_intent("check the system status").intent,
        Intent::StatusCheck
    );
}

#[test]
fn parse_turn_intent_with_keyword_extracts_pin() {
    let req = heiwa_protocol::parse_turn_intent("with ollama summarize this");
    assert_eq!(req.provider_pin.as_deref(), Some("ollama"));
    assert!(req.model_pin.is_none()); // no model after provider
}

#[test]
fn parse_turn_intent_using_keyword_extracts_pin() {
    let req = heiwa_protocol::parse_turn_intent("using claude sonnet-4 fix the tests");
    assert_eq!(req.provider_pin.as_deref(), Some("claude"));
    assert_eq!(req.model_pin.as_deref(), Some("sonnet-4"));
}

#[test]
fn parse_turn_intent_unknown_provider_not_pinned() {
    let req = heiwa_protocol::parse_turn_intent("use foobar to do something");
    assert!(req.provider_pin.is_none());
    assert!(req.model_pin.is_none());
}

#[test]
fn parse_turn_intent_as_drex_key_round_trip() {
    use heiwa_protocol::Intent;
    assert_eq!(Intent::Chat.as_drex_key(), "chat");
    assert_eq!(Intent::Build.as_drex_key(), "build");
    assert_eq!(Intent::Deploy.as_drex_key(), "deploy");
    assert_eq!(Intent::Audit.as_drex_key(), "audit");
    assert_eq!(Intent::Research.as_drex_key(), "research");
    assert_eq!(Intent::Strategy.as_drex_key(), "strategy");
    assert_eq!(Intent::StatusCheck.as_drex_key(), "status_check");
}

#[test]
fn session_agent_can_use_leased_tool_but_cannot_manage_permissions() {
    let root = std::env::current_dir().unwrap();
    let mut scope = heiwa_protocol::ExecutionScope::local_default(root);
    scope.tool_leases.push(heiwa_protocol::ToolLease {
        name: "shell".into(),
        risk_class: heiwa_protocol::RiskClass::HostMutating,
        allowed: true,
    });
    let agent = heiwa_protocol::SessionPrincipal::new(
        "agent:builder",
        heiwa_protocol::PrincipalKind::Agent,
        heiwa_protocol::ExecutionRole::Agent,
    );

    assert!(scope
        .authorize_tool(&agent, "shell", heiwa_protocol::Permission::RunShell)
        .is_allowed());
    assert!(!scope
        .authorize(&agent, heiwa_protocol::Permission::ManageSession)
        .is_allowed());
}

#[test]
fn leased_tool_still_fails_closed_when_lease_missing() {
    let root = std::env::current_dir().unwrap();
    let scope = heiwa_protocol::ExecutionScope::local_default(root);
    let agent = heiwa_protocol::SessionPrincipal::new(
        "agent:builder",
        heiwa_protocol::PrincipalKind::Agent,
        heiwa_protocol::ExecutionRole::Agent,
    );

    let decision = scope.authorize_tool(&agent, "shell", heiwa_protocol::Permission::RunShell);
    assert!(!decision.is_allowed());
    assert!(decision.reason().contains("lease"));
}

#[test]
fn viewer_is_read_only_even_with_tool_lease() {
    let root = std::env::current_dir().unwrap();
    let mut scope = heiwa_protocol::ExecutionScope::local_default(root);
    scope.tool_leases.push(heiwa_protocol::ToolLease {
        name: "shell".into(),
        risk_class: heiwa_protocol::RiskClass::HostMutating,
        allowed: true,
    });
    let viewer = heiwa_protocol::SessionPrincipal::new(
        "user:viewer",
        heiwa_protocol::PrincipalKind::HumanUser,
        heiwa_protocol::ExecutionRole::Viewer,
    );

    assert!(scope
        .authorize(&viewer, heiwa_protocol::Permission::ReadSessionContext)
        .is_allowed());
    assert!(!scope
        .authorize_tool(&viewer, "shell", heiwa_protocol::Permission::RunShell)
        .is_allowed());
}
