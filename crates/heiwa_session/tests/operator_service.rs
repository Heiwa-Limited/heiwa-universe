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
use serde_json::json;

fn test_service(path: &std::path::Path) -> OperatorSessionService {
    OperatorSessionService::new(OperatorJournal::new(path.to_path_buf()).unwrap())
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
    assert_eq!(service.thread("default").unwrap().turns[0].status, "interrupted");
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
    assert!(page.events.iter().all(|row| row.event.thread_id == "thread-a"));

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
    assert_eq!(more.events.len(), 2, "turn_started + user_message for the new turn");
    assert!(more.events.iter().all(|row| row.event.thread_id == "thread-a"));
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
