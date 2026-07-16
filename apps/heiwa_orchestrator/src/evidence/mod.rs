//! Local-first evidence plane for DREX orchestration.
//!
//! Backend pivot 2026-07-15: Lance + GitHub replace SpacetimeDB. DREX
//! decisions and failures append to newline-delimited JSON under
//! `~/.heiwa/evidence/`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::drex::{ResolutionTier, RoutePlan, DEFAULT_POLICY_VERSION};

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

pub trait EvidenceTransport: Send + Sync + 'static {
    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()>;
    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()>;
    fn attach_drex_decision_to_route(&self, request_id: &str, drex_decision_id: &str)
        -> Result<()>;
}

/// Appends every record as one JSON line to `<dir>/<kind>.jsonl`.
#[derive(Debug)]
pub struct JsonlTransport {
    dir: PathBuf,
    write_lock: Mutex<()>,
}

impl JsonlTransport {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            write_lock: Mutex::new(()),
        })
    }

    /// Default location: `~/.heiwa/evidence/`, overridable via
    /// `HEIWA_EVIDENCE_DIR` (tests and sandboxes must set it so they never
    /// write into the operator's real evidence corpus).
    pub fn default_local() -> Result<Self> {
        if let Some(dir) = std::env::var_os("HEIWA_EVIDENCE_DIR") {
            return Self::new(std::path::PathBuf::from(dir));
        }
        let dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("cannot resolve home directory"))?
            .join(".heiwa")
            .join("evidence");
        Self::new(dir)
    }

    fn append<T: Serialize>(&self, kind: &str, record: &T) -> Result<()> {
        let line = json!({
            "at_ms": now_ms(),
            "kind": kind,
            "record": record,
        })
        .to_string();
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow!("evidence write lock poisoned"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(format!("{kind}.jsonl")))?;
        writeln!(file, "{line}")?;
        Ok(())
    }
}

impl EvidenceTransport for JsonlTransport {
    fn upsert_drex_decision(&self, decision: PersistedDrexDecision) -> Result<()> {
        self.append("drex_decisions", &decision)
    }

    fn insert_drex_failure(&self, failure: PersistedDrexFailure) -> Result<()> {
        self.append("drex_failures", &failure)
    }

    fn attach_drex_decision_to_route(
        &self,
        request_id: &str,
        drex_decision_id: &str,
    ) -> Result<()> {
        self.append(
            "route_links",
            &json!({ "request_id": request_id, "drex_decision_id": drex_decision_id }),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopTransport;

impl EvidenceTransport for NoopTransport {
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
}

#[derive(Debug)]
pub struct EvidenceRuntime<T: EvidenceTransport = NoopTransport> {
    pub transport: T,
}

impl Default for EvidenceRuntime<NoopTransport> {
    fn default() -> Self {
        Self {
            transport: NoopTransport,
        }
    }
}

impl<T: EvidenceTransport> EvidenceRuntime<T> {
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
        // The route-decision log may be written before or after DREX
        // persistence. Keep the DREX row durable even if the join pointer
        // cannot be attached yet; later route writes can still supply the
        // same id.
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
