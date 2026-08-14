//! Local-first evidence plane.
//!
//! Backend pivot 2026-07-15: Lance + GitHub replace SpacetimeDB. Every record
//! that used to mirror to STDB reducers now appends to newline-delimited JSON
//! under the journal root. Text JSONL is the durable truth; any search index
//! built over it (Lance) is derived and rebuildable.
//!
//! Two evidence planes share this service:
//!
//! - **Journal** (`~/.heiwa/evidence/*.jsonl`): raw append-only runtime
//!   streams — sessions, leases, DREX decisions, run receipts. Local-only.
//!   GitHub sync is planned but gated: nothing here syncs until an explicit
//!   redaction and privacy boundary exists.
//! - **Receipts** (`~/.heiwa/state/evidence/<date>/`): individually written,
//!   redacted operator receipts (capabilities, promotion, compress). Written
//!   by surfaces that enforce `redaction_applied` before persisting.
//!
//! Env overrides: `HEIWA_EVIDENCE_DIR` relocates the journal root (tests and
//! sandboxes must set it so they never write into the operator's corpus).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

mod journal;
mod operator;
mod records;
mod replay;
mod runtime;
mod sensitive;
mod state;

pub use journal::{EvidenceTransport, JsonlTransport, NoopTransport};
pub use operator::{
    CursorError, CursorEvent, OperatorActor, OperatorEvent, OperatorEventType, OperatorJournal,
    OperatorPage, OperatorRisk, OperatorSensitivity, OPERATOR_CURSOR_VERSION,
    OPERATOR_EVENT_SCHEMA_VERSION, OPERATOR_STREAM_KIND,
};
pub use records::{
    PersistedArtifact, PersistedDispatchAck, PersistedDrexDecision, PersistedDrexFailure,
    PersistedRunFailure, PersistedRunReceipt, PersistedWorkerLease, PersistedWorkerSession,
};
pub use replay::{journal_summary, read_stream, EvidenceEvent, ReplayedStream, StreamSummary};
pub use runtime::EvidenceRuntime;
pub use sensitive::{find_sensitive, SensitiveMatch};
pub use state::{
    compact_stream, recover_interrupted, CompactionReport, RecoveryReport, WorkerStateView,
};

/// Version stamped into every journal envelope as `v`. This is the JOURNAL
/// ENVELOPE version — the outer `{v, at_ms, kind, record}` wrapper shared by
/// every stream in this crate — distinct from [`OPERATOR_EVENT_SCHEMA_VERSION`],
/// which versions the `OperatorEvent` contract carried inside `record` for
/// the operator stream specifically. Bump this on any change to envelope or
/// record shape that a replayer must distinguish; replay reports
/// pre-versioned legacy lines as `v = 0`.
pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// Canonical journal root: `HEIWA_EVIDENCE_DIR` override, else
/// `~/.heiwa/evidence/`. Resolution is owned by `heiwa_config::HeiwaPaths`.
pub fn journal_root() -> Result<PathBuf> {
    Ok(heiwa_config::HeiwaPaths::resolve().evidence_dir)
}

/// Root of the redacted receipts plane: `~/.heiwa/state/evidence/`. Kept
/// where the installed runtime already writes it; readers should resolve it
/// through this function rather than joining paths by hand.
pub fn receipts_root() -> Result<PathBuf> {
    Ok(heiwa_config::HeiwaPaths::resolve().receipts_dir())
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as u64
}

/// RFC3339 (UTC) timestamp for the current instant — the crate-standard
/// format for `occurred_at` fields on operator events and other records.
/// Public so downstream services authoring events (e.g. `heiwa_session`)
/// stamp timestamps identically instead of duplicating the formatter.
pub fn now_iso() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("timestamp formatting should succeed")
}
