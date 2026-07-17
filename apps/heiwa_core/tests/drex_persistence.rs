use std::sync::{Arc, Mutex};

use anyhow::Result;
use heiwa_core::drex::{
    DrexAuthorityGate, DrexDecision, DrexScoreCard, DrexVector, ExecutionMode, ResolutionTier,
    RoutePlan,
};
use heiwa_core::evidence::{
    EvidenceRuntime, EvidenceTransport, PersistedArtifact, PersistedDispatchAck,
    PersistedDrexDecision, PersistedDrexFailure, PersistedRunFailure, PersistedRunReceipt,
    PersistedWorkerLease, PersistedWorkerSession,
};
use heiwa_protocol::ModelTier;

#[derive(Clone, Default)]
struct MemoryTransport {
    decisions: Arc<Mutex<Vec<PersistedDrexDecision>>>,
    failures: Arc<Mutex<Vec<PersistedDrexFailure>>>,
    route_links: Arc<Mutex<Vec<(String, String)>>>,
}

impl EvidenceTransport for MemoryTransport {
    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()> {
        self.decisions.lock().unwrap().push(decision);
        Ok(())
    }

    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()> {
        self.failures.lock().unwrap().push(failure);
        Ok(())
    }

    fn attach_drex_decision_to_route(
        &self,
        request_id: &str,
        drex_decision_id: &str,
    ) -> Result<()> {
        self.route_links
            .lock()
            .unwrap()
            .push((request_id.to_string(), drex_decision_id.to_string()));
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

    fn register_artifact(&self, _artifact: PersistedArtifact) -> Result<()> {
        Ok(())
    }

    fn record_run_receipt(&self, _receipt: PersistedRunReceipt) -> Result<()> {
        Ok(())
    }

    fn record_run_failure(&self, _failure: PersistedRunFailure) -> Result<()> {
        Ok(())
    }
}

struct TestEvidenceClient {
    runtime: EvidenceRuntime<MemoryTransport>,
    transport: MemoryTransport,
}

impl TestEvidenceClient {
    fn new() -> Self {
        let transport = MemoryTransport::default();
        let runtime = EvidenceRuntime::new(transport.clone());
        Self { runtime, transport }
    }

    async fn record_drex_decision(
        &self,
        request_id: &str,
        task_id: &str,
        route_plan: &RoutePlan,
    ) -> Result<PersistedDrexDecision> {
        let decision = heiwa_core::evidence::build_drex_decision(request_id, task_id, route_plan);
        self.runtime.record_drex_decision(decision).await
    }

    async fn record_drex_failure(
        &self,
        drex_decision_id: &str,
        request_id: &str,
        failure_mode: &str,
        stage: &str,
        details_json: &str,
        recovered: bool,
    ) -> Result<PersistedDrexFailure> {
        self.runtime
            .record_drex_failure(
                drex_decision_id,
                request_id,
                failure_mode,
                stage,
                details_json,
                recovered,
            )
            .await
    }

    fn last_drex_decision(&self) -> Option<PersistedDrexDecision> {
        self.transport.decisions.lock().unwrap().last().cloned()
    }

    fn last_drex_failure(&self) -> Option<PersistedDrexFailure> {
        self.transport.failures.lock().unwrap().last().cloned()
    }

    fn last_route_link(&self) -> Option<(String, String)> {
        self.transport.route_links.lock().unwrap().last().cloned()
    }
}

#[tokio::test]
async fn record_drex_decision_writes_scores_and_axes() {
    let route_plan = sample_route_plan();
    let client = TestEvidenceClient::new();

    let stored = client
        .record_drex_decision("req-drex-1", "task-drex-1", &route_plan)
        .await
        .unwrap();
    let persisted = client.last_drex_decision().unwrap();

    assert_eq!(persisted.active_tier, "micro");
    assert_eq!(persisted.scope, route_plan.decision.vector.scope);
    assert_eq!(
        persisted.micro_score,
        route_plan.decision.scorecard.micro_score
    );
    assert_eq!(persisted.route_model, "ollama/qwen3.5:4b");
    assert_eq!(
        client.last_route_link(),
        Some(("req-drex-1".to_string(), stored.drex_decision_id))
    );
}

#[tokio::test]
async fn record_drex_failure_preserves_decision_linkage() {
    let route_plan = sample_route_plan();
    let client = TestEvidenceClient::new();
    let decision = client
        .record_drex_decision("req-drex-2", "task-drex-2", &route_plan)
        .await
        .unwrap();

    client
        .record_drex_failure(
            &decision.drex_decision_id,
            "req-drex-2",
            "fallback_escalation",
            "execution",
            r#"{"reason":"tool timeout"}"#,
            true,
        )
        .await
        .unwrap();

    let persisted = client.last_drex_failure().unwrap();
    assert_eq!(persisted.drex_decision_id, decision.drex_decision_id);
    assert_eq!(persisted.request_id, "req-drex-2");
    assert_eq!(persisted.failure_mode, "fallback_escalation");
    assert!(persisted.recovered);
}

fn sample_route_plan() -> RoutePlan {
    RoutePlan {
        decision: DrexDecision {
            vector: DrexVector {
                scope: 0.35,
                abstraction: 0.25,
                context_span: 0.55,
                execution_proximity: 0.95,
                blast_radius: 0.45,
                coordination_load: 0.20,
                latency_pressure: 0.85,
            },
            scorecard: DrexScoreCard {
                macro_score: 0.24,
                meso_score: 0.58,
                micro_score: 1.41,
                confidence: 0.83,
            },
            active_tier: ResolutionTier::Micro,
            gate: DrexAuthorityGate {
                authority_required: "none".to_string(),
                requires_approval: false,
                reasons: vec![],
            },
        },
        execution_mode: ExecutionMode::LocalModel,
        runtime_hint: "macbook".to_string(),
        selected_model: Some(ModelTier {
            id: 0,
            model_id: "ollama/qwen3.5:4b".to_string(),
            provider_model_id: "qwen3.5:4b".to_string(),
            provider: "ollama".to_string(),
            rate_group: "local_ollama".to_string(),
            capability_class: 2,
            effort_knob: "thinking:on".to_string(),
            effort_level: 4,
            cost_per_turn: 0.0,
            max_context_tokens: 32768,
            vram_requirement_mb: 4096,
            quantization_type: "q4_k_m".to_string(),
            kv_cache_strategy: "turboquant".to_string(),
            strengths_json: "[\"build\",\"research\",\"general\"]".to_string(),
            enabled: true,
            last_success_rate: 0.99,
            avg_latency_ms: 650,
            latency_p_95_ms: 1200,
            updated_at: "2026-04-01T00:00:00Z".to_string(),
        }),
        routing_metadata: "{\"reason\": \"test\"}".to_string(),
    }
}
