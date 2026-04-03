use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use heiwa_bindings::{
    attach_drex_decision_to_route_reducer::attach_drex_decision_to_route as AttachDrexDecisionToRouteReducer,
    record_drex_decision_reducer::record_drex_decision as RecordDrexDecisionReducer,
    record_drex_failure_reducer::record_drex_failure as RecordDrexFailureReducer,
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

pub trait StdbTransport: Send + Sync + 'static {
    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()>;
    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()>;
    fn attach_drex_decision_to_route(&self, request_id: &str, drex_decision_id: &str)
        -> Result<()>;
    fn register_session(
        &self,
        session_id: String,
        owner_id: Option<String>,
        node_id: String,
        session_type: String,
        expires_at: Option<String>,
        metadata_json: String,
    ) -> Result<()>;
    fn close_session(&self, session_id: String) -> Result<()>;
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
    fn register_session(
        &self,
        session_id: String,
        owner_id: Option<String>,
        node_id: String,
        session_type: String,
        expires_at: Option<String>,
        metadata_json: String,
    ) -> Result<()> {
        use heiwa_bindings::upsert_session;
        self.conn.reducers
            .upsert_session(
                session_id,
                owner_id,
                node_id,
                session_type,
                expires_at,
                metadata_json,
            )
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn close_session(&self, session_id: String) -> Result<()> {
        use heiwa_bindings::close_session;
        self.conn.reducers
            .close_session(session_id)
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

    fn register_session(
        &self,
        _session_id: String,
        _owner_id: Option<String>,
        _node_id: String,
        _session_type: String,
        _expires_at: Option<String>,
        _metadata_json: String,
    ) -> Result<()> {
        Ok(())
    }

    fn close_session(&self, _session_id: String) -> Result<()> {
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
