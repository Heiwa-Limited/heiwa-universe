use heiwa_evidence::{
    CursorError, OperatorActor, OperatorEvent, OperatorEventType, OperatorJournal, OperatorRisk,
    OperatorSensitivity,
};
use serde_json::json;
use std::io::Write;

fn event(
    id: &str,
    thread: &str,
    kind: OperatorEventType,
    payload: serde_json::Value,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: 1,
        event_id: id.into(),
        thread_id: thread.into(),
        turn_id: Some("turn-1".into()),
        run_id: None,
        call_id: None,
        event_type: kind,
        occurred_at: "2026-07-18T00:00:00Z".into(),
        actor: OperatorActor {
            kind: "operator".into(),
            id: "local-operator".into(),
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

#[test]
fn append_and_resume_from_versioned_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let first = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    journal
        .append(&event(
            "e2",
            "thread-a",
            OperatorEventType::AssistantCompleted,
            json!({"text":"hello"}),
        ))
        .unwrap();
    let page = journal.read_after(Some(&first.cursor), 50).unwrap();
    assert_eq!(
        page.events
            .iter()
            .map(|row| row.event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["e2"]
    );
}

#[test]
fn rejects_sensitive_payload_before_file_creation() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let error = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::ToolCallCompleted,
            json!({"output":"Bearer live-token"}),
        ))
        .unwrap_err();
    assert!(error.to_string().contains("sensitive material"));
    assert!(!dir.path().join("operator_events.jsonl").exists());
}

#[test]
fn fingerprint_or_boundary_mismatch_is_structured() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let row = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    std::fs::write(dir.path().join("operator_events.jsonl"), "").unwrap();
    assert!(matches!(
        journal.read_after(Some(&row.cursor), 50),
        Err(CursorError::InvalidCursor { .. })
    ));
}

#[test]
fn read_after_skips_a_truncated_tail_line() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    journal
        .append(&event(
            "e2",
            "thread-a",
            OperatorEventType::AssistantCompleted,
            json!({"text":"hello"}),
        ))
        .unwrap();

    // Simulate a crash mid-append: a truncated envelope with no trailing
    // newline yet.
    let path = dir.path().join("operator_events.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    use std::io::Write;
    write!(
        file,
        "{{\"v\":1,\"at_ms\":1,\"kind\":\"operator_events\",\"record\":{{\"event_id\":\"e3\""
    )
    .unwrap();
    drop(file);

    let page = journal.read_after(None, 50).unwrap();
    assert_eq!(
        page.events
            .iter()
            .map(|row| row.event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["e1", "e2"],
        "valid events survive the truncated tail"
    );
    assert_eq!(
        page.skipped_lines, 1,
        "the truncated tail line is counted, not fatal"
    );
}

#[test]
fn append_repairs_torn_tail_before_writing_the_next_framed_event() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    let path = dir.path().join("operator_events.jsonl");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"torn\":")
        .unwrap();

    journal
        .append(&event(
            "e2",
            "thread-a",
            OperatorEventType::AssistantCompleted,
            json!({"text":"hello"}),
        ))
        .unwrap();
    let page = journal.read_after(None, 10).unwrap();
    assert_eq!(
        page.events
            .iter()
            .map(|row| row.event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["e1", "e2"]
    );
    assert_eq!(page.skipped_lines, 0);
}

#[test]
fn next_cursor_echoes_input_when_empty_and_advances_when_events_return() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let first = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();

    // Nothing new past `first.cursor` yet: next_cursor echoes the input cursor
    // unchanged, so a polling caller can always assign
    // `cursor = page.next_cursor` without ever regressing to the start.
    let empty_page = journal.read_after(Some(&first.cursor), 50).unwrap();
    assert_eq!(empty_page.events.len(), 0);
    assert_eq!(
        empty_page.next_cursor.as_deref(),
        Some(first.cursor.as_str())
    );

    // Once a new event lands, next_cursor advances to the cursor after the
    // last event actually returned.
    let second = journal
        .append(&event(
            "e2",
            "thread-a",
            OperatorEventType::AssistantCompleted,
            json!({"text":"hello"}),
        ))
        .unwrap();
    let page = journal.read_after(Some(&first.cursor), 50).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some(second.cursor.as_str()));

    // From the very start (no cursor) with nothing appended yet, next_cursor
    // stays None, matching the None input.
    let fresh_dir = tempfile::tempdir().unwrap();
    let fresh_journal = OperatorJournal::new(fresh_dir.path().to_path_buf()).unwrap();
    let fresh_page = fresh_journal.read_after(None, 50).unwrap();
    assert_eq!(fresh_page.events.len(), 0);
    assert_eq!(fresh_page.next_cursor, None);
}

#[test]
fn limit_bounded_paging_walks_stream_in_stable_pages() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    for i in 1..=10 {
        journal
            .append(&event(
                &format!("e{i}"),
                "thread-a",
                OperatorEventType::UserMessage,
                json!({"text":"hi"}),
            ))
            .unwrap();
    }

    let mut cursor: Option<String> = None;
    let mut pages = Vec::new();
    loop {
        let page = journal.read_after(cursor.as_deref(), 3).unwrap();
        assert_eq!(page.skipped_lines, 0);
        if page.events.is_empty() {
            // Exhausted: next_cursor echoes the input cursor unchanged.
            assert_eq!(page.next_cursor, cursor);
            break;
        }
        assert!(page.events.len() <= 3, "limit bounds every page");
        assert_eq!(
            page.next_cursor.as_deref(),
            Some(page.events.last().unwrap().cursor.as_str()),
            "next_cursor is the cursor after the last returned event"
        );
        cursor = page.next_cursor.clone();
        pages.push(page);
    }

    assert_eq!(pages.len(), 4, "10 events at limit 3 walk as 3+3+3+1");
    assert_eq!(
        pages
            .iter()
            .map(|page| page.events.len())
            .collect::<Vec<_>>(),
        vec![3, 3, 3, 1]
    );
    let ids: Vec<&str> = pages
        .iter()
        .flat_map(|page| page.events.iter().map(|row| row.event.event_id.as_str()))
        .collect();
    let expected: Vec<String> = (1..=10).map(|i| format!("e{i}")).collect();
    assert_eq!(ids, expected.iter().map(String::as_str).collect::<Vec<_>>());

    // Cursors are stable: re-reading from a mid-stream page cursor yields the
    // same following page.
    let again = journal
        .read_after(pages[1].next_cursor.as_deref(), 3)
        .unwrap();
    assert_eq!(
        again
            .events
            .iter()
            .map(|row| row.event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["e7", "e8", "e9"]
    );
}

fn invalid_reason(result: Result<heiwa_evidence::OperatorPage, CursorError>) -> String {
    match result {
        Err(CursorError::InvalidCursor { reason }) => reason,
        Err(other) => panic!("expected InvalidCursor, got {other:?}"),
        Ok(page) => panic!(
            "expected InvalidCursor, got Ok page with {} events",
            page.events.len()
        ),
    }
}

/// Decode a real cursor, let the caller tamper with its JSON fields, and
/// re-encode it — producing structurally valid cursors with hostile values.
fn tampered_cursor(cursor: &str, tamper: impl FnOnce(&mut serde_json::Value)) -> String {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let bytes = engine
        .decode(cursor)
        .expect("real cursors are valid base64");
    let mut value: serde_json::Value =
        serde_json::from_slice(&bytes).expect("real cursors are valid JSON");
    tamper(&mut value);
    engine.encode(serde_json::to_vec(&value).unwrap())
}

#[test]
fn rejects_cursor_with_wrong_version() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let row = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    let hostile = tampered_cursor(&row.cursor, |value| value["version"] = json!(9));
    let reason = invalid_reason(journal.read_after(Some(&hostile), 50));
    assert!(
        reason.contains("version"),
        "reason names the version mismatch: {reason}"
    );
}

#[test]
fn rejects_non_base64_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let reason = invalid_reason(journal.read_after(Some("%%% not base64 %%%"), 50));
    assert!(
        reason.contains("base64"),
        "reason names the encoding failure: {reason}"
    );
}

#[test]
fn rejects_base64_cursor_that_is_not_json() {
    use base64::Engine as _;
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let hostile = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"definitely not json");
    let reason = invalid_reason(journal.read_after(Some(&hostile), 50));
    assert!(
        reason.contains("malformed"),
        "reason names the payload failure: {reason}"
    );
}

#[test]
fn rejects_json_cursor_with_wrong_shape() {
    use base64::Engine as _;
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let hostile = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"foo":1}"#);
    let reason = invalid_reason(journal.read_after(Some(&hostile), 50));
    assert!(
        reason.contains("malformed"),
        "reason names the payload failure: {reason}"
    );
}

#[test]
fn rejects_cursor_with_offset_beyond_end_of_stream() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let row = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    let hostile = tampered_cursor(&row.cursor, |value| value["offset"] = json!(u64::MAX));
    let reason = invalid_reason(journal.read_after(Some(&hostile), 50));
    assert!(
        reason.contains("beyond"),
        "reason names the range failure: {reason}"
    );
}

#[test]
fn rejects_cursor_with_offset_off_a_line_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let row = journal
        .append(&event(
            "e1",
            "thread-a",
            OperatorEventType::UserMessage,
            json!({"text":"hi"}),
        ))
        .unwrap();
    // Correct version and fingerprint, offset points into the middle of the
    // first envelope line.
    let hostile = tampered_cursor(&row.cursor, |value| value["offset"] = json!(5));
    let reason = invalid_reason(journal.read_after(Some(&hostile), 50));
    assert!(
        reason.contains("boundary"),
        "reason names the framing failure: {reason}"
    );
}

#[test]
fn concurrent_operator_journals_do_not_tear_lines() {
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();

    // Four independent OperatorJournal instances (separate in-process
    // mutexes), so serialization must come from the cross-process sidecar
    // lock, not the instance mutex.
    let handles: Vec<_> = (0..4)
        .map(|worker| {
            let dir_path = dir_path.clone();
            std::thread::spawn(move || {
                let journal = OperatorJournal::new(dir_path).unwrap();
                for i in 0..100 {
                    let id = format!("w{worker}-{i}");
                    journal
                        .append(&event(
                            &id,
                            "thread-a",
                            OperatorEventType::UserMessage,
                            json!({"text": "hi"}),
                        ))
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread join");
    }

    let journal = OperatorJournal::new(dir_path).unwrap();
    let page = journal.read_after(None, 1000).unwrap();
    assert_eq!(page.skipped_lines, 0, "no torn/interleaved lines");
    assert_eq!(page.events.len(), 400);
    let unique_ids: std::collections::HashSet<_> = page
        .events
        .iter()
        .map(|row| row.event.event_id.clone())
        .collect();
    assert_eq!(unique_ids.len(), 400, "all event ids are unique");
}
