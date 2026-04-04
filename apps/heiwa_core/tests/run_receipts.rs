use std::sync::{Arc, Mutex};

use anyhow::Result;
use heiwa_core::stdb::{
    PersistedArtifact, PersistedDispatchAck, PersistedDrexDecision, PersistedDrexFailure,
    PersistedRunReceipt, PersistedWorkerLease, PersistedWorkerSession, StdbRuntime, StdbTransport,
};

#[derive(Clone, Default)]
struct MemoryTransport {
    artifacts: Arc<Mutex<Vec<PersistedArtifact>>>,
    receipts: Arc<Mutex<Vec<PersistedRunReceipt>>>,
}

impl StdbTransport for MemoryTransport {
    fn upsert_drex_decision(&self, _decision: PersistedDrexDecision) -> Result<()> {
        Ok(())
    }

    fn insert_drex_failure(&self, _failure: PersistedDrexFailure) -> Result<()> {
        Ok(())
    }

    fn attach_drex_decision_to_route(
        &self,
        _request_id: &str,
        _drex_decision_id: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn upsert_worker_session(&self, _session: PersistedWorkerSession) -> Result<()> {
        Ok(())
    }

    fn close_session(&self, _session_id: String) -> Result<()> {
        Ok(())
    }

    fn upsert_worker_lease(&self, _lease: PersistedWorkerLease) -> Result<()> {
        Ok(())
    }

    fn record_dispatch_ack(&self, _ack: PersistedDispatchAck) -> Result<()> {
        Ok(())
    }

    fn register_artifact(&self, artifact: PersistedArtifact) -> Result<()> {
        self.artifacts.lock().unwrap().push(artifact);
        Ok(())
    }

    fn record_run_receipt(&self, receipt: PersistedRunReceipt) -> Result<()> {
        self.receipts.lock().unwrap().push(receipt);
        Ok(())
    }
}

struct TestStdbClient {
    runtime: StdbRuntime<MemoryTransport>,
    transport: MemoryTransport,
}

impl TestStdbClient {
    fn new() -> Self {
        let transport = MemoryTransport::default();
        let runtime = StdbRuntime::new(transport.clone());
        Self { runtime, transport }
    }

    fn last_receipt(&self) -> Option<PersistedRunReceipt> {
        self.transport.receipts.lock().unwrap().last().cloned()
    }

    fn artifacts(&self) -> Vec<PersistedArtifact> {
        self.transport.artifacts.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn successful_receipt_persists_run_and_artifacts() {
    let client = TestStdbClient::new();
    let artifact = PersistedArtifact {
        artifact_id: "artifact-success-1".to_string(),
        run_id: Some("run-task-1".to_string()),
        lease_id: Some("lease-1".to_string()),
        user_id: "mesh-worker".to_string(),
        mission_id: "task-1".to_string(),
        cell_run_id: None,
        artifact_type: "log".to_string(),
        title: "worker output".to_string(),
        uri: Some("artifact://runs/task-1/output".to_string()),
        path: None,
        content_json: r#"{"hash":"sha256:abc","size_bytes":128}"#.to_string(),
        created_at: "2026-04-03T07:20:00Z".to_string(),
        owner_id: None,
        principal_id: Some("session-1".to_string()),
    };
    let receipt = PersistedRunReceipt {
        run_id: "run-task-1".to_string(),
        user_id: "mesh-worker".to_string(),
        proposal_id: "task-1".to_string(),
        lease_id: "lease-1".to_string(),
        started_at: "2026-04-03T07:19:00Z".to_string(),
        ended_at: "2026-04-03T07:20:00Z".to_string(),
        status: "SUCCESS".to_string(),
        chain_result_json: r#"{"task_id":"task-1","status":"success"}"#.to_string(),
        signals_json: r#"{"node_id":"node-123","session_id":"session-1"}"#.to_string(),
        artifact_index_json: r#"["artifact-success-1"]"#.to_string(),
        node_id: "node-123".to_string(),
        replay_receipt_json: r#"{"task_id":"task-1","lease_id":"lease-1"}"#.to_string(),
        mode: "worker_mesh".to_string(),
        model_id: "llm".to_string(),
        tokens_input: 120,
        tokens_output: 44,
        tokens_total: 164,
        cost: 0.0,
        owner_id: None,
        principal_id: Some("session-1".to_string()),
        failure_code: None,
        failure_message: None,
    };

    client
        .runtime
        .record_receipt_bundle(receipt.clone(), vec![artifact.clone()])
        .await
        .expect("receipt bundle should persist");

    assert_eq!(client.last_receipt(), Some(receipt));
    assert_eq!(client.artifacts(), vec![artifact]);
}

#[tokio::test]
async fn failure_receipt_persists_structured_code_and_log_artifact() {
    let client = TestStdbClient::new();
    let artifact = PersistedArtifact {
        artifact_id: "artifact-error-1".to_string(),
        run_id: Some("run-task-2".to_string()),
        lease_id: Some("lease-2".to_string()),
        user_id: "mesh-worker".to_string(),
        mission_id: "task-2".to_string(),
        cell_run_id: None,
        artifact_type: "log".to_string(),
        title: "worker error log".to_string(),
        uri: Some("artifact://runs/task-2/error".to_string()),
        path: None,
        content_json: r#"{"code":"EXEC_ERROR","message":"tool subprocess exited 1"}"#.to_string(),
        created_at: "2026-04-03T07:22:00Z".to_string(),
        owner_id: None,
        principal_id: Some("session-2".to_string()),
    };
    let receipt = PersistedRunReceipt {
        run_id: "run-task-2".to_string(),
        user_id: "mesh-worker".to_string(),
        proposal_id: "task-2".to_string(),
        lease_id: "lease-2".to_string(),
        started_at: "2026-04-03T07:21:00Z".to_string(),
        ended_at: "2026-04-03T07:22:00Z".to_string(),
        status: "FAILED".to_string(),
        chain_result_json:
            r#"{"task_id":"task-2","message":"tool subprocess exited 1"}"#.to_string(),
        signals_json: r#"{"node_id":"node-456","session_id":"session-2"}"#.to_string(),
        artifact_index_json: r#"["artifact-error-1"]"#.to_string(),
        node_id: "node-456".to_string(),
        replay_receipt_json:
            r#"{"task_id":"task-2","lease_id":"lease-2","retryable":false}"#.to_string(),
        mode: "worker_mesh".to_string(),
        model_id: "error".to_string(),
        tokens_input: 0,
        tokens_output: 0,
        tokens_total: 0,
        cost: 0.0,
        owner_id: None,
        principal_id: Some("session-2".to_string()),
        failure_code: Some("EXEC_ERROR".to_string()),
        failure_message: Some("tool subprocess exited 1".to_string()),
    };

    client
        .runtime
        .record_receipt_bundle(receipt.clone(), vec![artifact.clone()])
        .await
        .expect("failure receipt bundle should persist");

    let stored = client.last_receipt().expect("receipt");
    assert_eq!(stored.failure_code.as_deref(), Some("EXEC_ERROR"));
    assert_eq!(
        stored.failure_message.as_deref(),
        Some("tool subprocess exited 1")
    );
    assert_eq!(client.artifacts(), vec![artifact]);
}

#[tokio::test]
async fn receipt_bundle_requires_at_least_one_artifact() {
    let client = TestStdbClient::new();
    let receipt = PersistedRunReceipt {
        run_id: "run-task-3".to_string(),
        user_id: "mesh-worker".to_string(),
        proposal_id: "task-3".to_string(),
        lease_id: "lease-3".to_string(),
        started_at: "2026-04-03T07:23:00Z".to_string(),
        ended_at: "2026-04-03T07:24:00Z".to_string(),
        status: "SUCCESS".to_string(),
        chain_result_json: "{}".to_string(),
        signals_json: "{}".to_string(),
        artifact_index_json: "[]".to_string(),
        node_id: "node-789".to_string(),
        replay_receipt_json: "{}".to_string(),
        mode: "worker_mesh".to_string(),
        model_id: "llm".to_string(),
        tokens_input: 1,
        tokens_output: 1,
        tokens_total: 2,
        cost: 0.0,
        owner_id: None,
        principal_id: Some("session-3".to_string()),
        failure_code: None,
        failure_message: None,
    };

    let error = client
        .runtime
        .record_receipt_bundle(receipt, Vec::new())
        .await
        .expect_err("receipts without artifacts must fail");
    assert!(error.to_string().contains("at least one artifact"));
}
