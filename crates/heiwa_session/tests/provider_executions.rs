//! Provider execution observations folded from operator route events.

use heiwa_evidence::{
    OperatorActor, OperatorEvent, OperatorEventType, OperatorJournal, OperatorRisk,
    OperatorSensitivity, OPERATOR_EVENT_SCHEMA_VERSION,
};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use heiwa_session::provider_executions::{
    observe_journal_tail, EvidenceState, LatestOutcome, ProviderExecution, ProviderExecutionView,
};
use serde_json::{json, Value};

const SENTINEL: &str = "SENTINEL-PROVIDER-MESSAGE-TEXT";

fn event(id: &str, kind: OperatorEventType, minute: u32, payload: Value) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: id.to_string(),
        thread_id: "thread-obs".to_string(),
        turn_id: Some("turn-obs".to_string()),
        run_id: None,
        call_id: Some("call-obs".to_string()),
        work_id: None,
        event_type: kind,
        occurred_at: format!("2026-10-08T10:{minute:02}:00Z"),
        actor: OperatorActor {
            kind: "runtime".to_string(),
            id: "model-call-executor".to_string(),
        },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

fn completed(id: &str, minute: u32, provider: &str, channel: Option<Value>) -> OperatorEvent {
    let mut payload = json!({"attempt": 1, "provider": provider, "model": "m-1"});
    if let Some(channel) = channel {
        payload["channel"] = channel;
    }
    event(id, OperatorEventType::RouteCompleted, minute, payload)
}

fn failed(id: &str, minute: u32, provider: &str, extra: Value) -> OperatorEvent {
    let mut payload = json!({
        "attempt": 1,
        "provider": provider,
        "model": "m-1",
        "failure_class": "authentication",
        "message": SENTINEL,
    });
    for (key, value) in extra.as_object().unwrap() {
        payload[key] = value.clone();
    }
    event(id, OperatorEventType::RouteFailed, minute, payload)
}

fn api(account: &str) -> Value {
    json!({"kind": "api_key", "account_id": account})
}

fn cli(binary: &str) -> Value {
    json!({"kind": "oauth_cli", "binary": binary})
}

fn provider_failure(channel: Value) -> Value {
    json!({"channel": channel, "provider_invoked": true, "failure_origin": "provider"})
}

fn view_of(events: &[OperatorEvent]) -> (tempfile::TempDir, ProviderExecutionView) {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    for event in events {
        journal.append(event).unwrap();
    }
    let service = OperatorSessionService::new(OperatorJournal::new(dir.path().into()).unwrap());
    let view = service.provider_executions().unwrap();
    (dir, view)
}

fn row<'a>(view: &'a ProviderExecutionView, provider: &str, kind: &str) -> &'a ProviderExecution {
    view.observations
        .iter()
        .find(|row| row.provider == provider && row.channel.kind == kind)
        .unwrap_or_else(|| panic!("no {provider}/{kind} row in {view:#?}"))
}

#[test]
fn legacy_rows_keep_an_unknown_channel_apart_from_recorded_channels() {
    let (_dir, view) = view_of(&[
        completed("legacy", 1, "gemini", None),
        completed("api", 2, "gemini", Some(api("google-api-7"))),
    ]);
    assert_eq!(view.observations.len(), 2);
    assert_eq!(row(&view, "gemini", "unknown").successes, 1);
    let api_row = row(&view, "gemini", "api_key");
    assert_eq!(api_row.channel.account_id.as_deref(), Some("google-api-7"));
    assert_eq!(view.evidence.state, EvidenceState::Complete);
}

#[test]
fn one_provider_over_api_and_cli_is_two_independent_rows() {
    let (_dir, view) = view_of(&[
        completed("api-ok", 1, "gemini", Some(api("google-api-7"))),
        failed("cli-fail", 2, "gemini", provider_failure(cli("gemini"))),
    ]);
    assert_eq!(
        row(&view, "gemini", "api_key").latest,
        LatestOutcome::Success
    );
    let cli_row = row(&view, "gemini", "oauth_cli");
    assert_eq!(cli_row.latest, LatestOutcome::Failure);
    assert!(cli_row.last_success.is_none());
    assert_eq!(cli_row.channel.binary.as_deref(), Some("gemini"));
}

#[test]
fn success_failure_and_recovery_report_dated_latest_outcomes_only() {
    let (_dir, failing) = view_of(&[
        completed("ok-1", 1, "claude", Some(cli("claude"))),
        failed("fail-2", 2, "claude", provider_failure(cli("claude"))),
    ]);
    let row_failing = row(&failing, "claude", "oauth_cli");
    assert_eq!(row_failing.latest, LatestOutcome::Failure);
    assert_eq!(
        row_failing.last_success.as_ref().unwrap().at,
        "2026-10-08T10:01:00Z"
    );
    let failure = row_failing.last_failure.as_ref().unwrap();
    assert_eq!(
        (failure.class.as_str(), failure.origin.as_str()),
        ("authentication", "provider")
    );

    let (_dir, recovered) = view_of(&[
        completed("ok-1", 1, "claude", Some(cli("claude"))),
        failed("fail-2", 2, "claude", provider_failure(cli("claude"))),
        completed("ok-3", 3, "claude", Some(cli("claude"))),
    ]);
    let row_recovered = row(&recovered, "claude", "oauth_cli");
    assert_eq!(row_recovered.latest, LatestOutcome::Success);
    assert_eq!((row_recovered.successes, row_recovered.failures), (2, 1));
    // Observations are dated facts, never a readiness verdict.
    let serialized = serde_json::to_string(&recovered).unwrap();
    for word in ["ready", "verified", "healthy", "ttl"] {
        assert!(!serialized.contains(word), "{word} in {serialized}");
    }
}

#[test]
fn resolver_and_accounting_failures_are_not_provider_failures() {
    let (_dir, view) = view_of(&[
        failed(
            "resolver",
            1,
            "openrouter",
            json!({"channel": {"kind": "unknown"}, "provider_invoked": false, "failure_origin": "resolver"}),
        ),
        failed(
            "accounting",
            2,
            "gemini",
            json!({"channel": api("google-api-7"), "provider_invoked": true, "failure_origin": "accounting", "failure_class": "invalid_usage"}),
        ),
        failed("legacy", 3, "codex", json!({})),
    ]);
    assert!(view
        .observations
        .iter()
        .all(|row| row.provider != "openrouter"));
    assert_eq!(view.excluded.not_invoked, 1);
    let accounting = row(&view, "gemini", "api_key");
    assert_eq!((accounting.successes, accounting.failures), (1, 0));
    assert_eq!(view.excluded.accounting, 1);
    // A legacy failure keeps its class but cannot claim provider origin.
    let legacy = row(&view, "codex", "unknown");
    assert_eq!(legacy.last_failure.as_ref().unwrap().origin, "unverified");
}

#[test]
fn cancelled_and_unfinished_attempts_do_not_become_failures() {
    let mut cancelled = failed(
        "cancel",
        2,
        "claude",
        json!({"channel": cli("claude"), "failure_class": "cancelled", "provider_invoked": true}),
    );
    cancelled.call_id = Some("call-cancel".to_string());
    let mut open = event(
        "attempt-open",
        OperatorEventType::RouteAttempted,
        3,
        json!({"attempt": 1, "provider": "codex", "model": "m-2"}),
    );
    open.call_id = Some("call-open".to_string());
    let mut answered = event(
        "attempt-answered",
        OperatorEventType::RouteAttempted,
        1,
        json!({"attempt": 1, "provider": "claude", "model": "m-1"}),
    );
    answered.call_id = Some("call-cancel".to_string());
    let (_dir, view) = view_of(&[answered, cancelled, open]);
    assert!(view.observations.is_empty(), "{view:#?}");
    assert_eq!(view.excluded.cancelled, 1);
    assert_eq!(view.open_attempts.len(), 1);
    assert_eq!(view.open_attempts[0].provider, "codex");
}

#[test]
fn an_outcome_for_a_finished_turn_is_still_a_provider_observation() {
    let dir = tempfile::tempdir().unwrap();
    let service = OperatorSessionService::new(OperatorJournal::new(dir.path().into()).unwrap());
    let turn = service
        .start_turn("thread-obs", StartTurnRequest::auto("request-1", "do it"))
        .unwrap();
    let journal = OperatorJournal::new(dir.path().into()).unwrap();
    let mut done = event(
        "turn-done",
        OperatorEventType::TurnCompleted,
        1,
        json!({"outcome": "completed"}),
    );
    done.turn_id = Some(turn.turn_id.clone());
    journal.append(&done).unwrap();
    let mut late = completed("late", 2, "claude", Some(cli("claude")));
    late.turn_id = Some(turn.turn_id.clone());
    journal.append(&late).unwrap();

    // Thread admission rejects the late row; the provider still answered.
    let reader = OperatorSessionService::new(OperatorJournal::new(dir.path().into()).unwrap());
    assert!(reader.thread("thread-obs").unwrap().skipped_events >= 1);
    let view = reader.provider_executions().unwrap();
    assert_eq!(row(&view, "claude", "oauth_cli").successes, 1);
}

#[test]
fn duplicate_ids_and_unsupported_schema_rows_are_not_observed() {
    let mut future = completed("future", 2, "gemini", Some(api("google-api-7")));
    future.schema_version = OPERATOR_EVENT_SCHEMA_VERSION + 1;
    let (_dir, view) = view_of(&[
        completed("same", 1, "gemini", Some(api("google-api-7"))),
        completed("same", 1, "gemini", Some(api("google-api-7"))),
        future,
    ]);
    assert_eq!(row(&view, "gemini", "api_key").successes, 1);
}

#[test]
fn provider_messages_never_reach_the_view() {
    let (dir, view) = view_of(&[failed("fail", 1, "claude", provider_failure(cli("claude")))]);
    assert!(!serde_json::to_string(&view).unwrap().contains(SENTINEL));
    let tail = observe_journal_tail(
        &OperatorJournal::new(dir.path().into()).unwrap(),
        1 << 20,
        100,
    );
    assert!(!serde_json::to_string(&tail).unwrap().contains(SENTINEL));
}

#[test]
fn malformed_identity_is_not_projected() {
    let (_dir, view) = view_of(&[
        completed("bad-provider", 1, "Gemini CLI!", Some(api("google-api-7"))),
        completed(
            "bad-account",
            2,
            "gemini",
            Some(json!({"kind": "api_key", "account_id": "has space"})),
        ),
        completed("bad-kind", 3, "gemini", Some(json!({"kind": "keychain"}))),
    ]);
    assert_eq!(view.excluded.malformed, 1);
    assert!(row(&view, "gemini", "api_key").channel.account_id.is_none());
    assert_eq!(row(&view, "gemini", "unknown").successes, 1);
}

#[test]
fn tail_views_distinguish_empty_partial_and_unavailable() {
    let empty_dir = tempfile::tempdir().unwrap();
    let empty = observe_journal_tail(
        &OperatorJournal::new(empty_dir.path().into()).unwrap(),
        4096,
        10,
    );
    assert_eq!(empty.evidence.state, EvidenceState::Empty);

    let (dir, _) = view_of(&[
        completed("old", 1, "claude", Some(cli("claude"))),
        completed("new", 2, "codex", Some(cli("codex"))),
    ]);
    let journal = OperatorJournal::new(dir.path().into()).unwrap();
    let complete = observe_journal_tail(&journal, 1 << 20, 100);
    assert_eq!(complete.evidence.state, EvidenceState::Complete);
    assert_eq!(
        complete.evidence.window_from.as_deref(),
        Some("2026-10-08T10:01:00Z")
    );
    let newest = observe_journal_tail(&journal, 1 << 20, 1);
    assert_eq!(newest.evidence.state, EvidenceState::Partial);
    assert_eq!(newest.observations.len(), 1);
    assert_eq!(newest.observations[0].provider, "codex");

    let broken = tempfile::tempdir().unwrap();
    std::fs::create_dir(broken.path().join("operator_events.jsonl")).unwrap();
    let unavailable = observe_journal_tail(
        &OperatorJournal::new(broken.path().into()).unwrap(),
        4096,
        10,
    );
    assert_eq!(unavailable.evidence.state, EvidenceState::Unavailable);
    assert_eq!(unavailable.evidence.error, Some("storage"));
}

#[test]
fn a_replaced_journal_lineage_rebuilds_observations() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().into()).unwrap();
    journal
        .append(&completed("old", 1, "claude", Some(cli("claude"))))
        .unwrap();
    let service = OperatorSessionService::new(OperatorJournal::new(dir.path().into()).unwrap());
    assert_eq!(service.provider_executions().unwrap().observations.len(), 1);

    std::fs::remove_file(dir.path().join("operator_events.jsonl")).unwrap();
    journal
        .append(&completed("new", 2, "codex", Some(cli("codex"))))
        .unwrap();
    let view = service.provider_executions().unwrap();
    assert_eq!(view.observations.len(), 1, "{view:#?}");
    assert_eq!(view.observations[0].provider, "codex");
}
