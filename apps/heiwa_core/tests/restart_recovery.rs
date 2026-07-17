//! Runtime restart recovery: a crashed/stopped runtime must close out
//! whatever the journal says was live before serving again.

use heiwa_core::evidence::{
    EvidenceTransport, JsonlTransport, PersistedWorkerLease, PersistedWorkerSession,
    WorkerStateView,
};
use heiwa_core::runtime::init_evidence_at;

fn live_session(id: &str) -> PersistedWorkerSession {
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
        active_tasks: 1,
        status: "active".to_string(),
        load: 1.0,
        created_at: "2026-07-16T00:00:00Z".to_string(),
        updated_at: "2026-07-16T00:00:00Z".to_string(),
        expires_at: "2026-07-16T01:00:00Z".to_string(),
        last_seen_at: "2026-07-16T00:00:00Z".to_string(),
        closed_at: None,
        current_task_id: Some("task-1".to_string()),
        lease_id: Some("lease-1".to_string()),
    }
}

fn open_lease(id: &str, session_id: &str) -> PersistedWorkerLease {
    PersistedWorkerLease {
        lease_id: id.to_string(),
        task_id: "task-1".to_string(),
        session_id: session_id.to_string(),
        node_id: "node-1".to_string(),
        capability: "exec".to_string(),
        status: "acked".to_string(),
        issued_at: "2026-07-16T00:00:00Z".to_string(),
        updated_at: "2026-07-16T00:00:00Z".to_string(),
        expires_at: "2026-07-16T01:00:00Z".to_string(),
        acked_at: Some("2026-07-16T00:00:01Z".to_string()),
        completed_at: None,
        failure_code: None,
        reason: None,
    }
}

#[test]
fn startup_closes_sessions_and_leases_left_live_by_previous_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Previous runtime: registered a session, dispatched a lease, then died.
    let previous = JsonlTransport::new(dir.path().to_path_buf()).expect("transport");
    previous.upsert_worker_session(live_session("s1")).unwrap();
    previous
        .upsert_worker_lease(open_lease("lease-1", "s1"))
        .unwrap();
    drop(previous);

    // New runtime boots against the same journal.
    let (_runtime, report) = init_evidence_at(dir.path().to_path_buf()).expect("init evidence");
    assert_eq!(report.sessions_closed, 1);
    assert_eq!(report.leases_revoked, 1);

    let view = WorkerStateView::replay(dir.path()).expect("replay");
    assert_eq!(view.sessions["s1"].status, "closed");
    assert_eq!(view.leases["lease-1"].status, "revoked");
    assert_eq!(
        view.leases["lease-1"].failure_code.as_deref(),
        Some("RUNTIME_RESTART")
    );
}
