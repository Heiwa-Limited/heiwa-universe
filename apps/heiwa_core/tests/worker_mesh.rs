use heiwa_core::runtime::{
    gateway::{
        parse_worker_envelope, DispatchAckPayload, DispatchPolicy, RegisterPayload, WorkerEnvelope,
        WorkerEnvelopeType,
    },
    state::{WorkerProtocolFlavor, WorkerRegistry, WorkerSessionRegistration},
};
use serde_json::json;

#[test]
fn canonical_worker_envelope_rejects_non_v1_versions() {
    let raw = json!({
        "version": "legacy",
        "type": "REGISTER",
        "timestamp": "2026-04-02T12:00:00Z",
        "node_id": "node-123",
        "payload": {
            "instance_id": "instance-1",
            "runtime": "python",
            "runtime_version": "3.12.0",
            "worker_version": "0.9.0",
            "capabilities": ["llm"],
            "max_concurrency": 2
        }
    })
    .to_string();

    let error = parse_worker_envelope(&raw).expect_err("non-v1 envelope must fail");
    assert_eq!(error.code, "VERSION_MISMATCH");
}

#[test]
fn canonical_worker_envelope_accepts_register_shape() {
    let raw = json!({
        "version": "v1",
        "type": "REGISTER",
        "timestamp": "2026-04-02T12:00:00Z",
        "node_id": "node-123",
        "payload": {
            "instance_id": "instance-1",
            "runtime": "python",
            "runtime_version": "3.12.0",
            "worker_version": "0.9.0",
            "capabilities": ["llm", "fs"],
            "max_concurrency": 2
        }
    })
    .to_string();

    let envelope = parse_worker_envelope(&raw).expect("valid envelope");
    assert_eq!(envelope.kind, WorkerEnvelopeType::Register);
    let payload: RegisterPayload =
        serde_json::from_value(envelope.payload).expect("register payload");
    assert_eq!(payload.capabilities, vec!["llm", "fs"]);
    assert_eq!(payload.max_concurrency, 2);
}

#[test]
fn worker_registry_tracks_session_dispatch_and_completion() {
    let mut registry = WorkerRegistry::default();
    let session = registry.register_session(WorkerSessionRegistration {
        session_id: "session-1".to_string(),
        node_id: "node-123".to_string(),
        instance_id: "instance-1".to_string(),
        runtime: "python".to_string(),
        runtime_version: "3.12.0".to_string(),
        worker_version: "0.9.0".to_string(),
        protocol: WorkerProtocolFlavor::V1,
        capabilities: vec!["llm".to_string(), "fs".to_string()],
        metadata: json!({"platform":"darwin-arm64"}),
        max_concurrency: 2,
        session_expires_at_ms: 10_000,
        last_seen_at_ms: 1_000,
    });

    assert_eq!(session.session_id, "session-1");
    let (chosen_session, lease) = registry
        .reserve_dispatch(
            "llm",
            "task-1".to_string(),
            "lease-1".to_string(),
            2_000,
            5_000,
        )
        .expect("dispatch should reserve");
    assert_eq!(chosen_session.session_id, "session-1");
    assert_eq!(lease.task_id, "task-1");

    registry
        .record_dispatch_ack("session-1", "task-1", "lease-1", true, 2_100)
        .expect("ack should succeed");
    let validated = registry
        .validate_lease("session-1", "task-1", "lease-1", 2_200)
        .expect("lease should remain valid");
    assert_eq!(validated.capability, "llm");

    let completed = registry
        .complete_dispatch("lease-1")
        .expect("completion should remove lease");
    assert_eq!(completed.session_id, "session-1");
    assert!(registry.resolve_lease_for_task("task-1").is_none());
    assert_eq!(registry.session("session-1").expect("session").active_tasks, 0);
}

#[test]
fn worker_registry_rejects_expired_or_mismatched_leases() {
    let mut registry = WorkerRegistry::default();
    registry.register_session(WorkerSessionRegistration {
        session_id: "session-2".to_string(),
        node_id: "node-456".to_string(),
        instance_id: "instance-2".to_string(),
        runtime: "python".to_string(),
        runtime_version: "3.12.0".to_string(),
        worker_version: "0.9.0".to_string(),
        protocol: WorkerProtocolFlavor::Legacy,
        capabilities: vec!["llm".to_string()],
        metadata: json!({"platform":"darwin-arm64"}),
        max_concurrency: 1,
        session_expires_at_ms: 10_000,
        last_seen_at_ms: 1_000,
    });
    registry
        .reserve_dispatch(
            "llm",
            "task-2".to_string(),
            "lease-2".to_string(),
            2_000,
            3_000,
        )
        .expect("dispatch should reserve");

    let mismatch = registry
        .validate_lease("session-2", "task-else", "lease-2", 2_500)
        .expect_err("mismatch must fail");
    assert_eq!(
        mismatch.code,
        heiwa_core::runtime::state::RegistryErrorCode::CapabilityMismatch
    );

    let expired = registry
        .validate_lease("session-2", "task-2", "lease-2", 3_500)
        .expect_err("expired lease must fail");
    assert_eq!(
        expired.code,
        heiwa_core::runtime::state::RegistryErrorCode::LeaseExpired
    );
}

#[test]
fn dispatch_ack_payload_round_trips() {
    let payload = DispatchAckPayload {
        task_id: "task-1".to_string(),
        lease_id: "lease-1".to_string(),
        accepted: true,
        reason: None,
    };
    let encoded = serde_json::to_value(&payload).expect("serialize");
    let decoded: DispatchAckPayload = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded.accepted, true);
}

#[test]
fn worker_envelope_round_trip_preserves_dispatch_schema() {
    let envelope = WorkerEnvelope {
        version: "v1".to_string(),
        kind: WorkerEnvelopeType::Dispatch,
        timestamp: "2026-04-02T12:01:00Z".to_string(),
        node_id: "node-123".to_string(),
        session_id: Some("session-1".to_string()),
        payload: serde_json::to_value(DispatchPolicy {
            side_effects: "deny".to_string(),
            timeout_ms: 300_000,
        })
        .expect("serialize"),
    };
    let encoded = serde_json::to_string(&envelope).expect("serialize");
    let decoded = parse_worker_envelope(&encoded).expect("parse");
    assert_eq!(decoded.kind, WorkerEnvelopeType::Dispatch);
}
