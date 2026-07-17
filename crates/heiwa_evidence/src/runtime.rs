//! Typed recording facade over an [`EvidenceTransport`].
//!
//! Methods are `async` purely for call-site stability (they never await);
//! callers in the runtime hold this inside async handlers.

use anyhow::{anyhow, Result};

use crate::journal::{EvidenceTransport, NoopTransport};
use crate::records::*;
use crate::now_ms;

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

    /// Record a fully built DREX decision. Callers own the mapping from their
    /// route-plan types to [`PersistedDrexDecision`] (see the apps' evidence
    /// modules), so this crate stays free of DREX internals.
    pub async fn record_drex_decision(
        &self,
        record: PersistedDrexDecision,
    ) -> Result<PersistedDrexDecision> {
        self.transport.upsert_drex_decision(record.clone())?;
        // The route-decision log may be written before or after DREX
        // persistence. Keep the DREX row durable even if the join pointer
        // cannot be attached yet; later route writes can still supply the
        // same id.
        let _ = self
            .transport
            .attach_drex_decision_to_route(&record.request_id, &record.drex_decision_id);
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

    #[allow(clippy::too_many_arguments)]
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
