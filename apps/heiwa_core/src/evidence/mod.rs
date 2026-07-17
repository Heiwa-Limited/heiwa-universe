//! Local-first evidence plane.
//!
//! Backend pivot 2026-07-15: Lance + GitHub replace SpacetimeDB. The journal
//! mechanics — envelope schema, append locking, replay, materialized worker
//! state, restart recovery, compaction — live in the shared
//! [`heiwa_evidence`] crate so core, orchestrator, and operator surfaces read
//! and write one evidence service. This module only owns the mapping from
//! core's DREX types into persisted records.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

pub use heiwa_evidence::{
    compact_stream, journal_root, read_stream, receipts_root, recover_interrupted,
    CompactionReport, EvidenceEvent, EvidenceRuntime, EvidenceTransport, JsonlTransport,
    NoopTransport, PersistedArtifact, PersistedDispatchAck, PersistedDrexDecision,
    PersistedDrexFailure, PersistedRunFailure, PersistedRunReceipt, PersistedWorkerLease,
    PersistedWorkerSession, RecoveryReport, ReplayedStream, WorkerStateView,
    EVIDENCE_SCHEMA_VERSION,
};

use crate::drex::{ResolutionTier, RoutePlan, DEFAULT_POLICY_VERSION};

/// Map a routed plan into the persisted DREX decision record. Pass the result
/// to [`EvidenceRuntime::record_drex_decision`], which appends it and attaches
/// the route-link join pointer.
pub fn build_drex_decision(
    request_id: &str,
    task_id: &str,
    route_plan: &RoutePlan,
) -> PersistedDrexDecision {
    let created_at_ms = now_ms();
    PersistedDrexDecision {
        drex_decision_id: format!("drex:{request_id}:{created_at_ms}"),
        request_id: request_id.to_string(),
        task_id: task_id.to_string(),
        active_tier: tier_label(&route_plan.decision.active_tier).to_string(),
        route_runtime: route_plan.runtime_hint.clone(),
        route_model: route_plan
            .selected_model
            .as_ref()
            .map(|tier| tier.model_id.clone())
            .unwrap_or_default(),
        scope: route_plan.decision.vector.scope,
        abstraction: route_plan.decision.vector.abstraction,
        context_span: route_plan.decision.vector.context_span,
        execution_proximity: route_plan.decision.vector.execution_proximity,
        blast_radius: route_plan.decision.vector.blast_radius,
        coordination_load: route_plan.decision.vector.coordination_load,
        latency_pressure: route_plan.decision.vector.latency_pressure,
        macro_score: route_plan.decision.scorecard.macro_score,
        meso_score: route_plan.decision.scorecard.meso_score,
        micro_score: route_plan.decision.scorecard.micro_score,
        score_confidence: route_plan.decision.scorecard.confidence,
        authority_required: route_plan.decision.gate.authority_required.clone(),
        requires_approval: route_plan.decision.gate.requires_approval,
        reasons_json: json!(route_plan.decision.gate.reasons).to_string(),
        vector_json: json!({
            "scope": route_plan.decision.vector.scope,
            "abstraction": route_plan.decision.vector.abstraction,
            "context_span": route_plan.decision.vector.context_span,
            "execution_proximity": route_plan.decision.vector.execution_proximity,
            "blast_radius": route_plan.decision.vector.blast_radius,
            "coordination_load": route_plan.decision.vector.coordination_load,
            "latency_pressure": route_plan.decision.vector.latency_pressure,
        })
        .to_string(),
        scorecard_json: json!({
            "macro_score": route_plan.decision.scorecard.macro_score,
            "meso_score": route_plan.decision.scorecard.meso_score,
            "micro_score": route_plan.decision.scorecard.micro_score,
            "confidence": route_plan.decision.scorecard.confidence,
        })
        .to_string(),
        gate_json: json!({
            "authority_required": route_plan.decision.gate.authority_required,
            "requires_approval": route_plan.decision.gate.requires_approval,
            "reasons": route_plan.decision.gate.reasons,
        })
        .to_string(),
        policy_version: DEFAULT_POLICY_VERSION.to_string(),
        created_at_ms,
    }
}

fn tier_label(tier: &ResolutionTier) -> &'static str {
    match tier {
        ResolutionTier::Macro => "macro",
        ResolutionTier::Meso => "meso",
        ResolutionTier::Micro => "micro",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as u64
}
