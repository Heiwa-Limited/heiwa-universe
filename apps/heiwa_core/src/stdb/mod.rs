use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use heiwa_bindings::{
    close_session_reducer::close_session as CloseSessionReducer,
    attach_drex_decision_to_route_reducer::attach_drex_decision_to_route as AttachDrexDecisionToRouteReducer,
    record_dispatch_ack_reducer::record_dispatch_ack as RecordDispatchAckReducer,
    record_drex_decision_reducer::record_drex_decision as RecordDrexDecisionReducer,
    record_drex_failure_reducer::record_drex_failure as RecordDrexFailureReducer,
    record_run_reducer::record_run as RecordRunReducer,
    register_artifact_reducer::register_artifact as RegisterArtifactReducer,
    upsert_lease_reducer::upsert_lease as UpsertLeaseReducer,
    upsert_session_reducer::upsert_session as UpsertSessionReducer,
    DbConnection,
};
use serde_json::json;

use crate::drex::{RoutePlan, ResolutionTier, DEFAULT_POLICY_VERSION};

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct PersistedDrexFailure {
    pub drex_decision_id: String,
    pub request_id: String,
    pub failure_mode: String,
    pub stage: String,
    pub details_json: String,
    pub recovered: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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

pub trait StdbTransport: Send + Sync + 'static {
    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()>;
    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()>;
    fn attach_drex_decision_to_route(&self, request_id: &str, drex_decision_id: &str)
        -> Result<()>;
    fn upsert_worker_session(&self, session: PersistedWorkerSession) -> Result<()>;
    fn close_session(&self, session_id: String) -> Result<()>;
    fn upsert_worker_lease(&self, lease: PersistedWorkerLease) -> Result<()>;
    fn record_dispatch_ack(&self, ack: PersistedDispatchAck) -> Result<()>;
    fn register_artifact(&self, artifact: PersistedArtifact) -> Result<()>;
    fn record_run_receipt(&self, receipt: PersistedRunReceipt) -> Result<()>;
    fn record_run_failure(&self, failure: PersistedRunFailure) -> Result<()>;
}

#[derive(Clone)]
pub struct ReducerTransport {
    pub conn: Arc<DbConnection>,
}

impl ReducerTransport {
    pub fn new(conn: Arc<DbConnection>) -> Self {
        Self { conn }
    }
}

impl StdbTransport for ReducerTransport {
    fn upsert_worker_session(&self, session: PersistedWorkerSession) -> Result<()> {
        self.conn.reducers
            .upsert_session(
                session.session_id,
                session.node_id,
                session.instance_id,
                session.runtime,
                session.runtime_version,
                session.worker_version,
                session.protocol,
                session.capabilities_json,
                session.metadata_json,
                session.max_concurrency,
                session.active_tasks,
                session.status,
                session.load,
                session.created_at,
                session.updated_at,
                session.expires_at,
                session.last_seen_at,
                session.closed_at,
                session.current_task_id,
                session.lease_id,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn close_session(&self, session_id: String) -> Result<()> {
        self.conn.reducers
            .close_session(session_id)
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn upsert_worker_lease(&self, lease: PersistedWorkerLease) -> Result<()> {
        self.conn.reducers
            .upsert_lease(
                lease.lease_id,
                lease.task_id,
                lease.session_id,
                lease.node_id,
                lease.capability,
                lease.status,
                lease.issued_at,
                lease.updated_at,
                lease.expires_at,
                lease.acked_at,
                lease.completed_at,
                lease.failure_code,
                lease.reason,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn record_dispatch_ack(&self, ack: PersistedDispatchAck) -> Result<()> {
        self.conn.reducers
            .record_dispatch_ack(
                ack.ack_id,
                ack.lease_id,
                ack.session_id,
                ack.task_id,
                ack.node_id,
                ack.status,
                ack.decided_at,
                ack.detail,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn register_artifact(&self, artifact: PersistedArtifact) -> Result<()> {
        self.conn.reducers
            .register_artifact(
                artifact.artifact_id,
                artifact.lease_id,
                artifact.run_id,
                artifact.session_id,
                artifact.user_id,
                artifact.mission_id,
                artifact.cell_run_id,
                artifact.artifact_type,
                artifact.title,
                artifact.uri,
                artifact.path,
                artifact.content_json,
                artifact.created_at,
                artifact.owner_id,
                artifact.principal_id,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn record_run_receipt(&self, receipt: PersistedRunReceipt) -> Result<()> {
        self.conn.reducers
            .record_run(
                receipt.run_id,
                receipt.user_id,
                receipt.proposal_id,
                receipt.lease_id,
                receipt.session_id,
                receipt.started_at,
                receipt.ended_at,
                receipt.status,
                receipt.chain_result_json,
                receipt.signals_json,
                receipt.artifact_index_json,
                receipt.node_id,
                receipt.replay_receipt_json,
                receipt.mode,
                receipt.model_id,
                receipt.tokens_input,
                receipt.tokens_output,
                receipt.tokens_total,
                receipt.cost,
                receipt.owner_id,
                receipt.principal_id,
                receipt.failure_code,
                receipt.failure_message,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn record_run_failure(&self, failure: PersistedRunFailure) -> Result<()> {
        use heiwa_bindings::record_run_failure_reducer::record_run_failure;
        self.conn.reducers
            .record_run_failure(
                failure.failure_id,
                failure.run_id,
                failure.lease_id,
                failure.session_id,
                failure.failure_code,
                failure.failure_message,
                failure.failure_type,
                failure.retryable,
                failure.details_json,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()> {
        self.conn.reducers
            .record_drex_decision(
                decision.drex_decision_id,
                decision.request_id,
                decision.task_id,
                decision.active_tier,
                decision.route_runtime,
                decision.route_model,
                decision.scope,
                decision.abstraction,
                decision.context_span,
                decision.execution_proximity,
                decision.blast_radius,
                decision.coordination_load,
                decision.latency_pressure,
                decision.macro_score,
                decision.meso_score,
                decision.micro_score,
                decision.score_confidence,
                decision.authority_required,
                decision.requires_approval,
                decision.reasons_json,
                decision.vector_json,
                decision.scorecard_json,
                decision.gate_json,
                decision.policy_version,
                decision.created_at_ms,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()> {
        self.conn.reducers
            .record_drex_failure(
                failure.drex_decision_id,
                failure.request_id,
                failure.failure_mode,
                failure.stage,
                failure.details_json,
                failure.recovered,
                failure.created_at_ms,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn attach_drex_decision_to_route(
        &self,
        request_id: &str,
        drex_decision_id: &str,
    ) -> Result<()> {
        self.conn.reducers
            .attach_drex_decision_to_route(
                request_id.to_string(),
                drex_decision_id.to_string(),
            )
            .map_err(|error| anyhow!(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopTransport;

impl StdbTransport for NoopTransport {
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

#[derive(Debug)]
pub struct StdbRuntime<T: StdbTransport = NoopTransport> {
    pub transport: T,
}

impl Default for StdbRuntime<NoopTransport> {
    fn default() -> Self {
        Self {
            transport: NoopTransport,
        }
    }
}

impl<T: StdbTransport> StdbRuntime<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub async fn record_drex_decision(
        &self,
        request_id: &str,
        task_id: &str,
        route_plan: &RoutePlan,
    ) -> Result<PersistedDrexDecision> {
        let created_at_ms = now_ms();
        let record = PersistedDrexDecision {
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
        };

        self.transport.upsert_drex_decision(record.clone())?;
        // During the migration window the route-decision log may be written before or
        // after DREX persistence. Keep the DREX row durable even if the join pointer
        // cannot be attached yet; later route writes can still supply the same id.
        let _ = self
            .transport
            .attach_drex_decision_to_route(request_id, &record.drex_decision_id);
        Ok(record)
    }

    pub async fn record_drex_failure(
        &self,
        drex_decision_id: &str,
        request_id: &str,
        failure_mode: &str,
        stage: &str,
        details_json: &str,
        recovered: bool,
    ) -> Result<PersistedDrexFailure> {
        let record = PersistedDrexFailure {
            drex_decision_id: drex_decision_id.to_string(),
            request_id: request_id.to_string(),
            failure_mode: failure_mode.to_string(),
            stage: stage.to_string(),
            details_json: details_json.to_string(),
            recovered,
            created_at_ms: now_ms(),
        };
        self.transport.insert_drex_failure(record.clone())?;
        Ok(record)
    }

    pub async fn record_receipt_bundle(
        &self,
        receipt: PersistedRunReceipt,
        artifacts: Vec<PersistedArtifact>,
    ) -> Result<PersistedRunReceipt> {
        if artifacts.is_empty() {
            return Err(anyhow!("run receipt requires at least one artifact"));
        }

        for artifact in artifacts {
            self.transport.register_artifact(artifact)?;
        }
        self.transport.record_run_receipt(receipt.clone())?;
        Ok(receipt)
    }

    pub async fn record_run_failure(
        &self,
        run_id: &str,
        lease_id: &str,
        session_id: &str,
        failure_code: &str,
        failure_message: &str,
        failure_type: &str,
        retryable: bool,
        details_json: &str,
    ) -> Result<PersistedRunFailure> {
        let record = PersistedRunFailure {
            failure_id: format!("fail-{}", uuid::Uuid::new_v4()),
            run_id: run_id.to_string(),
            lease_id: lease_id.to_string(),
            session_id: session_id.to_string(),
            failure_code: failure_code.to_string(),
            failure_message: failure_message.to_string(),
            failure_type: failure_type.to_string(),
            retryable,
            details_json: details_json.to_string(),
        };
        self.transport.record_run_failure(record.clone())?;
        Ok(record)
    }

    pub async fn upsert_worker_session(
        &self,
        session: PersistedWorkerSession,
    ) -> Result<PersistedWorkerSession> {
        self.transport.upsert_worker_session(session.clone())?;
        Ok(session)
    }

    pub async fn upsert_worker_lease(
        &self,
        lease: PersistedWorkerLease,
    ) -> Result<PersistedWorkerLease> {
        self.transport.upsert_worker_lease(lease.clone())?;
        Ok(lease)
    }

    pub async fn record_worker_dispatch_ack(
        &self,
        ack: PersistedDispatchAck,
    ) -> Result<PersistedDispatchAck> {
        self.transport.record_dispatch_ack(ack.clone())?;
        Ok(ack)
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
