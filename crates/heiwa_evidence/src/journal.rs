//! Append-only JSONL journal transport.
//!
//! One stream per record kind (`<dir>/<kind>.jsonl`). Every line is a
//! versioned envelope `{v, at_ms, kind, record}`. Appends serialize through a
//! sidecar lock file (`.<kind>.jsonl.lock`) so multiple processes — runtime,
//! shell, orchestrator — can share one journal without torn lines, and so
//! compaction can atomically swap the stream without losing racing writes.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::json;

use crate::records::*;
use crate::{journal_root, now_ms, EVIDENCE_SCHEMA_VERSION};

pub trait EvidenceTransport: Send + Sync + 'static {
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

    /// Untyped append for records that don't warrant a dedicated method
    /// (task dispatches, capability leases, node heartbeats, battlefields).
    fn journal(&self, _kind: &str, _payload: serde_json::Value) -> Result<()> {
        Ok(())
    }
}

/// Acquire the cross-process append lock for one stream. The lock file is a
/// stable sidecar: it survives compaction's rename of the data file, so a
/// writer can never end up appending to an unlinked inode.
pub(crate) fn lock_stream(dir: &Path, kind: &str) -> Result<File> {
    let lock_path = dir.join(format!(".{kind}.jsonl.lock"));
    let lock_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock_path)?;
    lock_file.lock()?;
    Ok(lock_file)
}

pub(crate) fn stream_path(dir: &Path, kind: &str) -> PathBuf {
    dir.join(format!("{kind}.jsonl"))
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
        Self::new(journal_root()?)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn append<T: Serialize>(&self, kind: &str, record: &T) -> Result<()> {
        let line = json!({
            "v": EVIDENCE_SCHEMA_VERSION,
            "at_ms": now_ms(),
            "kind": kind,
            "record": record,
        })
        .to_string();
        let mut payload = line.into_bytes();
        payload.push(b'\n');

        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow!("evidence write lock poisoned"))?;
        let _stream_lock = lock_stream(&self.dir, kind)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(stream_path(&self.dir, kind))?;
        file.write_all(&payload)?;
        file.sync_data()?;
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

    fn upsert_worker_session(&self, session: PersistedWorkerSession) -> Result<()> {
        self.append("worker_sessions", &session)
    }

    fn close_session(&self, session_id: String) -> Result<()> {
        self.append("session_closes", &json!({ "session_id": session_id }))
    }

    fn upsert_worker_lease(&self, lease: PersistedWorkerLease) -> Result<()> {
        self.append("worker_leases", &lease)
    }

    fn record_dispatch_ack(&self, ack: PersistedDispatchAck) -> Result<()> {
        self.append("dispatch_acks", &ack)
    }

    fn register_artifact(&self, artifact: PersistedArtifact) -> Result<()> {
        self.append("artifacts", &artifact)
    }

    fn record_run_receipt(&self, receipt: PersistedRunReceipt) -> Result<()> {
        self.append("runs", &receipt)
    }

    fn record_run_failure(&self, failure: PersistedRunFailure) -> Result<()> {
        self.append("run_failures", &failure)
    }

    fn journal(&self, kind: &str, payload: serde_json::Value) -> Result<()> {
        self.append(kind, &payload)
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
