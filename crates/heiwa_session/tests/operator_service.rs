//! Materialization, idempotency, validation, and recovery tests for
//! [`heiwa_session::operator::OperatorSessionService`].
//!
//! Every test builds its own `OperatorJournal` under a fresh `tempfile`
//! directory and never touches `HOME` or the real `~/.heiwa` corpus.

use heiwa_evidence::{
    OperatorActor, OperatorEvent, OperatorEventType, OperatorJournal, OperatorRisk,
    OperatorSensitivity, OPERATOR_EVENT_SCHEMA_VERSION,
};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use heiwa_session::{rebuild_operator_indexes_at, EmbeddingSink};
use serde_json::json;
use std::sync::Mutex;

fn test_service(path: &std::path::Path) -> OperatorSessionService {
    OperatorSessionService::new(OperatorJournal::new(path.to_path_buf()).unwrap())
}

#[derive(Default)]
struct RecordingEmbedder {
    rows: Mutex<Vec<(String, String, String)>>,
}

impl EmbeddingSink for RecordingEmbedder {
    fn upsert_text(&self, thread_id: &str, event_id: &str, text: &str) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push((
            thread_id.to_string(),
            event_id.to_string(),
            text.to_string(),
        ));
        Ok(())
    }
}

#[test]
fn rebuild_indexes_projects_safe_text_and_only_embeds_messages() {
    let evidence = tempfile::tempdir().unwrap();
    let indexes = tempfile::tempdir().unwrap();
    let service = test_service(evidence.path());
    let turn = service
        .start_turn(
            "default",
            StartTurnRequest::auto("request-1", "index this user text"),
        )
        .unwrap();
    let mut assistant = base_event(
        "default",
        Some(&turn.turn_id),
        None,
        OperatorEventType::AssistantCompleted,
    );
    assistant.payload = json!({"text": "index this assistant text"});
    service.append_event(assistant).unwrap();
    let mut tool = base_event(
        "default",
        Some(&turn.turn_id),
        Some("call-1"),
        OperatorEventType::ToolCallCompleted,
    );
    tool.sensitivity = OperatorSensitivity::Restricted;
    tool.payload = json!({"name": "shell", "output": "restricted but safe tool output"});
    service.append_event(tool).unwrap();

    let sink = RecordingEmbedder::default();
    let first =
        rebuild_operator_indexes_at(&service, &sink, &indexes.path().join("sessions.sqlite3"))
            .unwrap();
    let second =
        rebuild_operator_indexes_at(&service, &sink, &indexes.path().join("sessions.sqlite3"))
            .unwrap();

    assert_eq!(first.fts_rows, 3);
    assert_eq!(first.embedded_rows, 2);
    assert_eq!(first.embedding_failures, 0);
    assert_eq!(second, first);
    assert_eq!(
        sink.rows.lock().unwrap().len(),
        4,
        "each rebuild embeds user and assistant only"
    );
}

/// Build a syntactically-valid `OperatorEvent` for validation tests: correct
/// schema version, a fixed occurred_at, and a fresh random event_id so
/// repeated calls never collide. Callers override whatever field their test
/// cares about.
fn base_event(
    thread_id: &str,
    turn_id: Option<&str>,
    call_id: Option<&str>,
    event_type: OperatorEventType,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: format!("evt-{}", uuid::Uuid::new_v4()),
        thread_id: thread_id.to_string(),
        turn_id: turn_id.map(|s| s.to_string()),
        run_id: None,
        call_id: call_id.map(|s| s.to_string()),
        event_type,
        occurred_at: "2026-07-18T00:00:00Z".to_string(),
        actor: OperatorActor {
            kind: "runtime".into(),
            id: "test-runner".into(),
        },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: vec![],
        evidence_refs: vec![],
        payload: json!({}),
    }
}

// ---------------------------------------------------------------------
// Step 1 verbatim: materialization / idempotency / recovery.
// ---------------------------------------------------------------------

#[test]
fn duplicate_client_request_returns_one_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let request = StartTurnRequest::auto("req-1", "hello");
    let first = service.start_turn("default", request.clone()).unwrap();
    let second = service.start_turn("default", request).unwrap();
    assert_eq!(first.turn_id, second.turn_id);
    assert!(second.duplicate);
    assert_eq!(service.thread("default").unwrap().turns.len(), 1);
}

#[test]
fn restart_closes_unfinished_turn_once() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    service
        .start_turn("default", StartTurnRequest::auto("req-1", "hello"))
        .unwrap();
    assert_eq!(service.recover_interrupted().unwrap(), 1);
    assert_eq!(service.recover_interrupted().unwrap(), 0);
    assert_eq!(
        service.thread("default").unwrap().turns[0].status,
        "interrupted"
    );
}

#[test]
fn start_turn_rejects_sensitive_prompt_before_creating_operator_events() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let error = service
        .start_turn("default", StartTurnRequest::auto("req-1", "ghp_live-token"))
        .unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("sensitive"),
        "error should identify the preflight safety rejection: {error}"
    );

    assert!(
        service
            .events_after("default", None, 100)
            .unwrap()
            .events
            .is_empty(),
        "no thread_created or turn_started event may precede a rejected message"
    );
    assert!(
        !dir.path().join("operator_events.jsonl").exists(),
        "preflight must reject before creating the journal stream"
    );
}

#[test]
fn start_turn_rejects_sensitive_client_request_id_before_creating_operator_events() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    assert!(service
        .start_turn("default", StartTurnRequest::auto("ghp_live-token", "hello"))
        .is_err());
    assert!(service
        .events_after("default", None, 100)
        .unwrap()
        .events
        .is_empty());
    assert!(!dir.path().join("operator_events.jsonl").exists());
}

#[test]
fn start_turn_rejects_sensitive_route_policy_before_creating_operator_events() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let mut request = StartTurnRequest::auto("req-1", "hello");
    request.route_policy.preferred_provider = Some("ghp_live-token".to_string());

    assert!(service.start_turn("default", request).is_err());
    assert!(service
        .events_after("default", None, 100)
        .unwrap()
        .events
        .is_empty());
    assert!(!dir.path().join("operator_events.jsonl").exists());
}

#[test]
fn orphaned_turn_retry_appends_the_missing_user_message() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let mut orphan = base_event(
        "default",
        Some("orphan-turn"),
        None,
        OperatorEventType::TurnStarted,
    );
    orphan.payload = json!({
        "client_request_id": "req-1",
        "prompt_fingerprint": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    });
    service.append_event(orphan).unwrap();

    let retry = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hello"))
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.turn_id, "orphan-turn");
    assert_eq!(
        service.thread("default").unwrap().turns[0]
            .prompt
            .as_deref(),
        Some("hello")
    );
}

#[test]
fn retry_with_same_client_request_and_different_prompt_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    service
        .start_turn("default", StartTurnRequest::auto("req-1", "hello"))
        .unwrap();

    let error = service
        .start_turn(
            "default",
            StartTurnRequest::auto("req-1", "different prompt"),
        )
        .unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("prompt"),
        "error should identify the retry payload mismatch: {error}"
    );
}

// ---------------------------------------------------------------------
// append_event validation rejections.
// ---------------------------------------------------------------------

#[test]
fn append_event_rejects_unsupported_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let mut event = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::AssistantStarted,
    );
    event.schema_version = 99;

    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("schema_version") || message.contains("schema version"),
        "error should name the schema version violation: {message}"
    );
}

#[test]
fn unknown_schema_events_do_not_materialize_threads_or_suppress_later_creation() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let mut unknown = base_event("unknown-only", None, None, OperatorEventType::ThreadCreated);
    unknown.schema_version = 99;
    journal.append(&unknown).unwrap();
    let service = OperatorSessionService::new(journal);

    assert!(service.list_threads(10).unwrap().is_empty());
    let diagnostic = service.thread("unknown-only").unwrap();
    assert!(diagnostic.turns.is_empty());
    assert_eq!(diagnostic.skipped_events, 1);

    service
        .start_turn("other", StartTurnRequest::auto("other-1", "hi"))
        .unwrap();
    service
        .start_turn("unknown-only", StartTurnRequest::auto("unknown-1", "hi"))
        .unwrap();

    let summaries = service.list_threads(10).unwrap();
    assert_eq!(summaries[0].thread_id, "unknown-only");
    assert_eq!(summaries[1].thread_id, "other");
    let events = service
        .events_after("unknown-only", None, 100)
        .unwrap()
        .events;
    assert_eq!(
        events
            .iter()
            .filter(|row| {
                row.event.schema_version == OPERATOR_EVENT_SCHEMA_VERSION
                    && row.event.event_type == OperatorEventType::ThreadCreated
            })
            .count(),
        1,
        "the later valid start must create the thread despite the unknown-schema record"
    );
}

#[test]
fn rejected_unknown_turn_events_stay_diagnostic_until_valid_lifecycle_events_arrive() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    journal
        .append(&base_event(
            "replay-thread",
            Some("missing-turn"),
            None,
            OperatorEventType::UserMessage,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "replay-thread",
            Some("missing-turn"),
            Some("call-1"),
            OperatorEventType::RoutePlanned,
        ))
        .unwrap();
    let service = OperatorSessionService::new(journal);

    assert!(service.list_threads(10).unwrap().is_empty());
    assert_eq!(service.thread("replay-thread").unwrap().skipped_events, 2);

    // Direct-journal lifecycle records establish the thread and its turn;
    // the earlier rejected rows remain diagnostics only.
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    journal
        .append(&base_event(
            "replay-thread",
            None,
            None,
            OperatorEventType::ThreadCreated,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "replay-thread",
            Some("valid-turn"),
            None,
            OperatorEventType::TurnStarted,
        ))
        .unwrap();
    let service = OperatorSessionService::new(journal);

    let summaries = service.list_threads(10).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].thread_id, "replay-thread");
    let view = service.thread("replay-thread").unwrap();
    assert_eq!(view.turns.len(), 1);
    assert_eq!(view.skipped_events, 2);
}

#[test]
fn rejected_late_progress_does_not_advance_thread_recency() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    journal
        .append(&base_event(
            "closed-thread",
            None,
            None,
            OperatorEventType::ThreadCreated,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "closed-thread",
            Some("closed-turn"),
            None,
            OperatorEventType::TurnStarted,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "closed-thread",
            Some("closed-turn"),
            None,
            OperatorEventType::TurnCompleted,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "later-thread",
            None,
            None,
            OperatorEventType::ThreadCreated,
        ))
        .unwrap();
    journal
        .append(&base_event(
            "closed-thread",
            Some("closed-turn"),
            None,
            OperatorEventType::AssistantStarted,
        ))
        .unwrap();
    let service = OperatorSessionService::new(journal);

    let summaries = service.list_threads(10).unwrap();
    assert_eq!(summaries[0].thread_id, "later-thread");
    assert_eq!(summaries[1].thread_id, "closed-thread");
    assert_eq!(service.thread("closed-thread").unwrap().skipped_events, 1);
}

#[test]
fn append_event_rejects_turn_event_missing_turn_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let event = base_event("default", None, None, OperatorEventType::TurnCompleted);
    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("turn_id"),
        "error should name the missing turn_id: {message}"
    );
}

#[test]
fn append_event_rejects_user_message_missing_turn_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let error = service
        .append_event(base_event(
            "default",
            None,
            None,
            OperatorEventType::UserMessage,
        ))
        .unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("turn_id"),
        "error should identify the missing turn id: {error}"
    );
}

#[test]
fn append_event_rejects_nonterminal_event_for_nonexistent_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let event = base_event(
        "default",
        Some("missing-turn"),
        None,
        OperatorEventType::AssistantStarted,
    );
    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("does not exist") || message.contains("unknown turn"),
        "error should identify the missing turn: {message}"
    );
}

#[test]
fn append_event_rejects_terminal_event_for_nonexistent_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let event = base_event(
        "default",
        Some("missing-turn"),
        None,
        OperatorEventType::TurnCompleted,
    );
    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("does not exist") || message.contains("unknown turn"),
        "error should identify the missing turn: {message}"
    );
}

#[test]
fn append_event_allows_turn_started_to_create_synthetic_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    service
        .append_event(base_event(
            "legacy-thread",
            Some("legacy-turn"),
            None,
            OperatorEventType::TurnStarted,
        ))
        .unwrap();

    let view = service.thread("legacy-thread").unwrap();
    assert_eq!(view.turns.len(), 1);
    assert_eq!(view.turns[0].turn_id, "legacy-turn");
}

#[test]
fn append_event_rejects_duplicate_turn_started_turn_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    service
        .append_event(base_event(
            "default",
            Some("turn-1"),
            None,
            OperatorEventType::TurnStarted,
        ))
        .unwrap();

    let error = service
        .append_event(base_event(
            "default",
            Some("turn-1"),
            None,
            OperatorEventType::TurnStarted,
        ))
        .unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("already exists"),
        "error should identify the duplicate turn id: {error}"
    );
}

#[test]
fn append_event_rejects_conflicting_turn_started_client_request_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let mut first = base_event(
        "default",
        Some("turn-1"),
        None,
        OperatorEventType::TurnStarted,
    );
    first.payload = json!({ "client_request_id": "request-1" });
    service.append_event(first).unwrap();

    let mut conflict = base_event(
        "default",
        Some("turn-2"),
        None,
        OperatorEventType::TurnStarted,
    );
    conflict.payload = json!({ "client_request_id": "request-1" });
    let error = service.append_event(conflict).unwrap_err();
    assert!(
        error
            .to_string()
            .to_lowercase()
            .contains("client_request_id"),
        "error should identify the conflicting client request: {error}"
    );
}

#[test]
fn append_event_rejects_route_event_missing_call_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let event = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::RoutePlanned,
    );
    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("call_id"),
        "error should name the missing call_id: {message}"
    );
}

#[test]
fn append_event_rejects_tool_event_missing_call_id() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let event = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::ToolCallStarted,
    );
    let error = service.append_event(event).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("call_id"),
        "error should name the missing call_id: {message}"
    );
}

#[test]
fn append_event_accepts_well_formed_events_before_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let route = base_event(
        "default",
        Some(&submission.turn_id),
        Some("call-1"),
        OperatorEventType::RoutePlanned,
    );
    service.append_event(route).unwrap();

    let tool = base_event(
        "default",
        Some(&submission.turn_id),
        Some("call-1"),
        OperatorEventType::ToolCallStarted,
    );
    service.append_event(tool).unwrap();
}

#[test]
fn append_event_rejects_nonterminal_event_on_terminal_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let completed = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnCompleted,
    );
    service.append_event(completed).unwrap();

    let assistant = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::AssistantStarted,
    );
    let error = service.append_event(assistant).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("terminal"),
        "error should name the terminal-state violation: {message}"
    );
}

#[test]
fn append_event_rejects_cancel_request_on_terminal_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();

    let completed = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnCompleted,
    );
    service.append_event(completed).unwrap();

    let cancel = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnCancelRequested,
    );
    let error = service.append_event(cancel).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("terminal"),
        "error should name the terminal-state violation: {message}"
    );
}

#[test]
fn append_event_rejects_second_terminal_event() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();
    service
        .append_event(base_event(
            "default",
            Some(&submission.turn_id),
            None,
            OperatorEventType::TurnCompleted,
        ))
        .unwrap();

    let error = service
        .append_event(base_event(
            "default",
            Some(&submission.turn_id),
            None,
            OperatorEventType::TurnInterrupted,
        ))
        .unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("terminal"),
        "error should reject every event after terminal state: {error}"
    );
}

#[test]
fn operator_cancelled_interruption_requires_prior_cancel_request() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();
    let mut interrupted = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnInterrupted,
    );
    interrupted.payload = json!({ "reason": "OPERATOR_CANCELLED" });

    let error = service.append_event(interrupted).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("cancel"),
        "error should require prior cancellation intent: {error}"
    );
}

#[test]
fn pending_cancellation_rejects_completion_and_closes_as_operator_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();
    service
        .append_event(base_event(
            "default",
            Some(&submission.turn_id),
            None,
            OperatorEventType::TurnCancelRequested,
        ))
        .unwrap();

    let completion = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnCompleted,
    );
    assert!(service.append_event(completion).is_err());

    let mut interrupted = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnInterrupted,
    );
    interrupted.payload = json!({ "reason": "OPERATOR_CANCELLED" });
    service.append_event(interrupted).unwrap();
    assert_eq!(
        service.thread("default").unwrap().turns[0].status,
        "interrupted"
    );
}

#[test]
fn restart_recovery_closes_pending_cancellation_as_operator_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hi"))
        .unwrap();
    service
        .append_event(base_event(
            "default",
            Some(&submission.turn_id),
            None,
            OperatorEventType::TurnCancelRequested,
        ))
        .unwrap();

    assert_eq!(service.recover_interrupted().unwrap(), 1);
    let events = service.events_after("default", None, 100).unwrap().events;
    let recovered = events.last().unwrap();
    assert_eq!(
        recovered.event.event_type,
        OperatorEventType::TurnInterrupted
    );
    assert_eq!(recovered.event.payload["reason"], "OPERATOR_CANCELLED");
}

// ---------------------------------------------------------------------
// events_after: thread filtering across interleaved threads, cursor
// advance over nonmatching rows.
// ---------------------------------------------------------------------

#[test]
fn events_after_filters_thread_and_advances_cursor_across_interleaved_threads() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a1", "hi a1"))
        .unwrap();
    service
        .start_turn("thread-b", StartTurnRequest::auto("req-b1", "hi b1"))
        .unwrap();
    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a2", "hi a2"))
        .unwrap();
    service
        .start_turn("thread-b", StartTurnRequest::auto("req-b2", "hi b2"))
        .unwrap();
    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a3", "hi a3"))
        .unwrap();

    // Page through thread-a's events with a small limit. Even though
    // thread-b's events are interleaved in the global stream, we must never
    // reread a row and never miss one.
    let mut cursor: Option<String> = None;
    let mut collected_ids = Vec::new();
    loop {
        let page = service
            .events_after("thread-a", cursor.as_deref(), 2)
            .unwrap();
        if page.events.is_empty() {
            assert_eq!(page.next_cursor.as_deref(), cursor.as_deref());
            break;
        }
        for row in &page.events {
            assert_eq!(row.event.thread_id, "thread-a");
            collected_ids.push(row.event.event_id.clone());
        }
        cursor = page.next_cursor.clone();
    }

    // thread_created + 3 turns * (turn_started + user_message) = 7.
    assert_eq!(collected_ids.len(), 7);
    let unique: std::collections::HashSet<_> = collected_ids.iter().collect();
    assert_eq!(unique.len(), 7, "no row was replayed twice");
}

#[test]
fn events_after_advances_cursor_past_trailing_nonmatching_events() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a1", "hi a1"))
        .unwrap();
    service
        .start_turn("thread-b", StartTurnRequest::auto("req-b1", "hi b1"))
        .unwrap();
    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a2", "hi a2"))
        .unwrap();
    // thread-b trails last in the global stream.
    service
        .start_turn("thread-b", StartTurnRequest::auto("req-b2", "hi b2"))
        .unwrap();

    let page = service.events_after("thread-a", None, 100).unwrap();
    assert_eq!(page.events.len(), 5, "thread_created + 2 turns * 2 events");
    assert!(page
        .events
        .iter()
        .all(|row| row.event.thread_id == "thread-a"));

    // Polling again from next_cursor finds nothing new: the cursor advanced
    // all the way past the trailing thread-b events instead of getting
    // stuck at thread-a's last actual match.
    let empty = service
        .events_after("thread-a", page.next_cursor.as_deref(), 100)
        .unwrap();
    assert_eq!(empty.events.len(), 0);
    assert_eq!(empty.next_cursor, page.next_cursor);

    // A fresh thread-a event appended afterward is still picked up from
    // that same cursor: we did not lose our place either.
    service
        .start_turn("thread-a", StartTurnRequest::auto("req-a3", "hi a3"))
        .unwrap();
    let more = service
        .events_after("thread-a", page.next_cursor.as_deref(), 100)
        .unwrap();
    assert_eq!(
        more.events.len(),
        2,
        "turn_started + user_message for the new turn"
    );
    assert!(more
        .events
        .iter()
        .all(|row| row.event.thread_id == "thread-a"));
}

// ---------------------------------------------------------------------
// list_threads: bounded output.
// ---------------------------------------------------------------------

#[test]
fn list_threads_bounds_output_and_orders_most_recent_first() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    service
        .start_turn("thread-1", StartTurnRequest::auto("r1", "hi"))
        .unwrap();
    service
        .start_turn("thread-2", StartTurnRequest::auto("r2", "hi"))
        .unwrap();
    service
        .start_turn("thread-3", StartTurnRequest::auto("r3", "hi"))
        .unwrap();

    let all = service.list_threads(10).unwrap();
    assert_eq!(all.len(), 3);

    let bounded = service.list_threads(2).unwrap();
    assert_eq!(bounded.len(), 2, "limit bounds the returned thread count");
    assert_eq!(bounded[0].thread_id, "thread-3");
    assert_eq!(bounded[1].thread_id, "thread-2");
}

// ---------------------------------------------------------------------
// thread(): journal-level damage surfaced distinctly from event-level
// rejects.
// ---------------------------------------------------------------------

#[test]
fn thread_view_surfaces_journal_damage_as_skipped_lines() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    service
        .start_turn("default", StartTurnRequest::auto("req-1", "hello"))
        .unwrap();

    // Journal-level damage: a complete but unparseable line appended
    // directly to the stream file, as a crashed or foreign writer might
    // leave behind. This is below the event contract entirely — no
    // event_id, no thread_id — so it must surface as `skipped_lines`
    // (journal damage), never as `skipped_events` (schema/state rejects).
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("operator_events.jsonl"))
        .unwrap();
    writeln!(file, "this is not a journal envelope").unwrap();
    drop(file);

    let view = service.thread("default").unwrap();
    assert_eq!(
        view.skipped_lines, 1,
        "journal damage is surfaced, counted once"
    );
    assert_eq!(
        view.skipped_events, 0,
        "no schema/state-level rejects occurred"
    );
    assert_eq!(view.turns.len(), 1, "valid events still project");
    assert_eq!(view.turns[0].prompt.as_deref(), Some("hello"));
}

// ---------------------------------------------------------------------
// thread(): materialized turn status transitions.
// ---------------------------------------------------------------------

#[test]
fn thread_view_reflects_turn_completed_transition() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());

    let submission = service
        .start_turn("default", StartTurnRequest::auto("req-1", "hello"))
        .unwrap();

    let before = service.thread("default").unwrap();
    assert_eq!(before.turns.len(), 1);
    assert_eq!(before.turns[0].status, "open");
    assert_eq!(before.turns[0].turn_id, submission.turn_id);

    let completed = base_event(
        "default",
        Some(&submission.turn_id),
        None,
        OperatorEventType::TurnCompleted,
    );
    service.append_event(completed).unwrap();

    let after = service.thread("default").unwrap();
    assert_eq!(after.turns.len(), 1);
    assert_eq!(after.turns[0].status, "completed");
}
