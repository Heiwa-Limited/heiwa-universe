//! Journal mechanics: append envelope, replay, corruption tolerance, locking.

use std::fs::OpenOptions;
use std::io::Write;

use heiwa_evidence::{
    read_stream, EvidenceTransport, JsonlTransport, PersistedWorkerSession, EVIDENCE_SCHEMA_VERSION,
};

fn sample_session(id: &str) -> PersistedWorkerSession {
    PersistedWorkerSession {
        session_id: id.to_string(),
        node_id: "node-1".to_string(),
        instance_id: "inst-1".to_string(),
        runtime: "claude".to_string(),
        runtime_version: "1.0".to_string(),
        worker_version: "0.1".to_string(),
        protocol: "v1".to_string(),
        capabilities_json: "[\"exec\"]".to_string(),
        metadata_json: "{}".to_string(),
        max_concurrency: 1,
        active_tasks: 0,
        status: "active".to_string(),
        load: 0.0,
        created_at: "2026-07-16T00:00:00Z".to_string(),
        updated_at: "2026-07-16T00:00:00Z".to_string(),
        expires_at: "2026-07-16T01:00:00Z".to_string(),
        last_seen_at: "2026-07-16T00:00:00Z".to_string(),
        closed_at: None,
        current_task_id: None,
        lease_id: None,
    }
}

#[test]
fn append_and_replay_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let transport = JsonlTransport::new(dir.path().to_path_buf()).expect("transport");
    transport
        .upsert_worker_session(sample_session("s1"))
        .expect("append");

    let stream = read_stream(dir.path(), "worker_sessions").expect("replay");
    assert_eq!(stream.skipped_lines, 0);
    assert_eq!(stream.events.len(), 1);
    let event = &stream.events[0];
    assert_eq!(event.v, EVIDENCE_SCHEMA_VERSION);
    assert_eq!(event.kind, "worker_sessions");
    assert!(event.at_ms > 0);
    let record: PersistedWorkerSession =
        serde_json::from_value(event.record.clone()).expect("typed record");
    assert_eq!(record.session_id, "s1");
    assert_eq!(record.status, "active");
}

#[test]
fn replay_of_missing_stream_is_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stream = read_stream(dir.path(), "worker_sessions").expect("replay");
    assert_eq!(stream.events.len(), 0);
    assert_eq!(stream.skipped_lines, 0);
}

#[test]
fn replay_skips_corrupt_and_truncated_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let transport = JsonlTransport::new(dir.path().to_path_buf()).expect("transport");
    transport
        .upsert_worker_session(sample_session("s1"))
        .expect("append 1");
    transport
        .upsert_worker_session(sample_session("s2"))
        .expect("append 2");

    // Simulate corruption: one garbage line, one truncated tail (crash mid-write).
    let path = dir.path().join("worker_sessions.jsonl");
    let mut file = OpenOptions::new().append(true).open(&path).expect("open");
    writeln!(file, "this is not json").expect("garbage line");
    write!(file, "{{\"v\":1,\"at_ms\":123,\"kind\":\"worker_se").expect("truncated tail");

    let stream = read_stream(dir.path(), "worker_sessions").expect("replay");
    assert_eq!(stream.events.len(), 2, "valid events survive corruption");
    assert_eq!(
        stream.skipped_lines, 2,
        "corrupt + truncated lines are counted"
    );
}

#[test]
fn replay_accepts_legacy_unversioned_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("worker_sessions.jsonl"),
        "{\"at_ms\":42,\"kind\":\"worker_sessions\",\"record\":{\"session_id\":\"legacy\"}}\n",
    )
    .expect("write legacy line");

    let stream = read_stream(dir.path(), "worker_sessions").expect("replay");
    assert_eq!(stream.events.len(), 1);
    assert_eq!(
        stream.events[0].v, 0,
        "legacy lines report schema version 0"
    );
    assert_eq!(
        stream.events[0].record["session_id"].as_str(),
        Some("legacy")
    );
}

#[test]
fn concurrent_appends_from_independent_transports_do_not_tear_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = dir.path().to_path_buf();

    // Two transport instances (separate in-process mutexes) so serialization
    // must come from the cross-process file lock, not the instance mutex.
    let handles: Vec<_> = (0..2)
        .map(|worker| {
            let dir_path = dir_path.clone();
            std::thread::spawn(move || {
                let transport = JsonlTransport::new(dir_path).expect("transport");
                for i in 0..100 {
                    let session = sample_session(&format!("w{worker}-{i}"));
                    transport.upsert_worker_session(session).expect("append");
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread join");
    }

    let stream = read_stream(&dir_path, "worker_sessions").expect("replay");
    assert_eq!(stream.skipped_lines, 0, "no torn/interleaved lines");
    assert_eq!(stream.events.len(), 200);
}

#[test]
fn journal_summary_reports_stream_counts_and_skips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let transport = JsonlTransport::new(dir.path().to_path_buf()).expect("transport");
    transport
        .upsert_worker_session(sample_session("s1"))
        .unwrap();
    transport
        .upsert_worker_session(sample_session("s2"))
        .unwrap();
    transport
        .journal("node_heartbeats", serde_json::json!({"node": "n1"}))
        .unwrap();
    let mut file = OpenOptions::new()
        .append(true)
        .open(dir.path().join("worker_sessions.jsonl"))
        .expect("open");
    writeln!(file, "garbage").expect("garbage");

    let summary = heiwa_evidence::journal_summary(dir.path()).expect("summary");
    let sessions = summary
        .iter()
        .find(|stream| stream.kind == "worker_sessions")
        .expect("worker_sessions stream listed");
    assert_eq!(sessions.events, 2);
    assert_eq!(sessions.skipped_lines, 1);
    let heartbeats = summary
        .iter()
        .find(|stream| stream.kind == "node_heartbeats")
        .expect("node_heartbeats stream listed");
    assert_eq!(heartbeats.events, 1);
    // Sidecar lock files must not be listed as streams.
    assert!(summary.iter().all(|stream| !stream.kind.starts_with('.')));
}
