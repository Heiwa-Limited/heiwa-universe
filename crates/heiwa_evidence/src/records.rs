//! Persisted evidence record schemas.
//!
//! These are the durable shapes appended to the JSONL journal. They were the
//! SpacetimeDB row types before the 2026-07-15 backend pivot; the JSONL plane
//! keeps them verbatim so history remains replayable.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedDrexDecision {
    pub drex_decision_id: String,
    pub request_id: String,
    pub task_id: String,
    pub active_tier: String,
    pub route_runtime: String,
    pub route_model: String,
    pub scope: f64,
    pub abstraction: f64,
    pub context_span: f64,
    pub execution_proximity: f64,
    pub blast_radius: f64,
    pub coordination_load: f64,
    pub latency_pressure: f64,
    pub macro_score: f64,
    pub meso_score: f64,
    pub micro_score: f64,
    pub score_confidence: f64,
    pub authority_required: String,
    pub requires_approval: bool,
    pub reasons_json: String,
    pub vector_json: String,
    pub scorecard_json: String,
    pub gate_json: String,
    pub policy_version: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedDrexFailure {
    pub drex_decision_id: String,
    pub request_id: String,
    pub failure_mode: String,
    pub stage: String,
    pub details_json: String,
    pub recovered: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedArtifact {
    pub artifact_id: String,
    pub run_id: Option<String>,
    pub lease_id: Option<String>,
    pub session_id: Option<String>,
    pub user_id: String,
    pub mission_id: String,
    pub cell_run_id: Option<String>,
    pub artifact_type: String,
    pub title: String,
    pub uri: Option<String>,
    pub path: Option<String>,
    pub content_json: String,
    pub created_at: String,
    pub owner_id: Option<String>,
    pub principal_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedWorkerSession {
    pub session_id: String,
    pub node_id: String,
    pub instance_id: String,
    pub runtime: String,
    pub runtime_version: String,
    pub worker_version: String,
    pub protocol: String,
    pub capabilities_json: String,
    pub metadata_json: String,
    pub max_concurrency: i64,
    pub active_tasks: u32,
    pub status: String,
    pub load: f64,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub last_seen_at: String,
    pub closed_at: Option<String>,
    pub current_task_id: Option<String>,
    pub lease_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedWorkerLease {
    pub lease_id: String,
    pub task_id: String,
    pub session_id: String,
    pub node_id: String,
    pub capability: String,
    pub status: String,
    pub issued_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub acked_at: Option<String>,
    pub completed_at: Option<String>,
    pub failure_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedDispatchAck {
    pub ack_id: String,
    pub lease_id: String,
    pub session_id: String,
    pub task_id: String,
    pub node_id: String,
    pub status: String,
    pub decided_at: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedRunReceipt {
    pub run_id: String,
    pub user_id: String,
    pub proposal_id: String,
    pub lease_id: String,
    pub session_id: Option<String>,
    pub started_at: String,
    pub ended_at: String,
    pub status: String,
    pub chain_result_json: String,
    pub signals_json: String,
    pub artifact_index_json: String,
    pub node_id: String,
    pub replay_receipt_json: String,
    pub mode: String,
    pub model_id: String,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_total: i64,
    pub cost: f64,
    pub owner_id: Option<String>,
    pub principal_id: Option<String>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedRunFailure {
    pub failure_id: String,
    pub run_id: String,
    pub lease_id: String,
    pub session_id: String,
    pub failure_code: String,
    pub failure_message: String,
    pub failure_type: String,
    pub retryable: bool,
    pub details_json: String,
}
