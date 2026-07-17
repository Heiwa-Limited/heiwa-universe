//! Materialized state over the journal: replay folds, restart recovery,
//! and stream compaction.
//!
//! The journal appends full-row snapshots per mutation, so materialization is
//! a last-write-wins fold keyed by id. Runtime restart never resurrects live
//! rows: recovery closes out whatever was in flight and appends the closure
//! events, keeping the journal the single consistent truth.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::journal::{lock_stream, stream_path, EvidenceTransport};
use crate::now_iso;
use crate::records::{PersistedWorkerLease, PersistedWorkerSession};
use crate::replay::read_stream;

/// Read model over the worker journal streams: latest snapshot per session
/// and lease, with `session_closes` tombstones applied.
#[derive(Debug, Default)]
pub struct WorkerStateView {
    pub sessions: HashMap<String, PersistedWorkerSession>,
    pub leases: HashMap<String, PersistedWorkerLease>,
    /// Journal lines that could not be parsed into typed records.
    pub skipped_lines: usize,
}

impl WorkerStateView {
    pub fn replay(dir: &Path) -> Result<Self> {
        let mut view = Self::default();
        let mut session_last_at: HashMap<String, u64> = HashMap::new();

        let sessions = read_stream(dir, "worker_sessions")?;
        view.skipped_lines += sessions.skipped_lines;
        for event in sessions.events {
            match serde_json::from_value::<PersistedWorkerSession>(event.record) {
                Ok(session) => {
                    session_last_at.insert(session.session_id.clone(), event.at_ms);
                    view.sessions.insert(session.session_id.clone(), session);
                }
                Err(_) => view.skipped_lines += 1,
            }
        }

        let leases = read_stream(dir, "worker_leases")?;
        view.skipped_lines += leases.skipped_lines;
        for event in leases.events {
            match serde_json::from_value::<PersistedWorkerLease>(event.record) {
                Ok(lease) => {
                    view.leases.insert(lease.lease_id.clone(), lease);
                }
                Err(_) => view.skipped_lines += 1,
            }
        }

        // Tombstones live in their own stream, so file order says nothing
        // about cross-stream order; apply a close only if it is not older
        // than the session's last snapshot.
        let closes = read_stream(dir, "session_closes")?;
        view.skipped_lines += closes.skipped_lines;
        for event in closes.events {
            let Some(session_id) = event.record.get("session_id").and_then(Value::as_str) else {
                view.skipped_lines += 1;
                continue;
            };
            let last_at = session_last_at.get(session_id).copied().unwrap_or(0);
            if event.at_ms < last_at {
                continue;
            }
            if let Some(session) = view.sessions.get_mut(session_id) {
                session.status = "closed".to_string();
                if session.closed_at.is_none() {
                    session.closed_at = Some(session.last_seen_at.clone());
                }
            }
        }

        Ok(view)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryReport {
    pub sessions_closed: usize,
    pub leases_revoked: usize,
}

/// Restart recovery: close every session and revoke every lease that was
/// still live when the previous runtime stopped. Worker connections do not
/// survive a restart, so pretending those rows are live would be a lie; the
/// closure events keep the journal consistent and make the interruption
/// visible to operator surfaces. Idempotent — a second pass finds nothing.
pub fn recover_interrupted<T: EvidenceTransport>(
    dir: &Path,
    transport: &T,
) -> Result<RecoveryReport> {
    let view = WorkerStateView::replay(dir)?;
    let now = now_iso();
    let mut report = RecoveryReport::default();

    for (_, session) in view.sessions {
        if matches!(session.status.as_str(), "closed" | "expired") {
            continue;
        }
        let mut closed = session;
        closed.status = "closed".to_string();
        closed.updated_at = now.clone();
        closed.closed_at = Some(now.clone());
        let session_id = closed.session_id.clone();
        transport.upsert_worker_session(closed)?;
        transport.close_session(session_id)?;
        report.sessions_closed += 1;
    }

    for (_, lease) in view.leases {
        if !matches!(lease.status.as_str(), "issued" | "acked") {
            continue;
        }
        let mut revoked = lease;
        revoked.status = "revoked".to_string();
        revoked.updated_at = now.clone();
        revoked.completed_at = Some(now.clone());
        revoked.failure_code = Some("RUNTIME_RESTART".to_string());
        revoked.reason = Some("runtime restarted before lease completion".to_string());
        transport.upsert_worker_lease(revoked)?;
        report.leases_revoked += 1;
    }

    Ok(report)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CompactionReport {
    pub records_before: usize,
    pub records_after: usize,
    pub corrupt_lines_dropped: usize,
}

/// Rewrite a keyed snapshot stream keeping only the newest record per key.
/// Only meaningful for last-write-wins streams (`worker_sessions`,
/// `worker_leases`); append-forever receipt streams must not be compacted.
/// Holds the stream's append lock for the whole rewrite and swaps the file
/// atomically, so concurrent appenders never lose a write.
pub fn compact_stream(dir: &Path, kind: &str, key_field: &str) -> Result<CompactionReport> {
    let path = stream_path(dir, kind);
    if !path.exists() {
        return Ok(CompactionReport::default());
    }

    let _stream_lock = lock_stream(dir, kind)?;
    let mut raw = Vec::new();
    OpenOptions::new()
        .read(true)
        .open(&path)?
        .read_to_end(&mut raw)?;

    // Preserve original line bytes so compaction never rewrites envelopes,
    // only drops superseded ones.
    let mut lines: Vec<(Vec<u8>, Option<String>)> = Vec::new();
    let mut report = CompactionReport::default();
    for chunk in raw.split(|byte| *byte == b'\n') {
        if chunk.is_empty() {
            continue;
        }
        let parsed = serde_json::from_slice::<Value>(chunk).ok();
        match parsed {
            Some(value) if value.get("record").is_some() => {
                let key = value
                    .get("record")
                    .and_then(|record| record.get(key_field))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                lines.push((chunk.to_vec(), key));
            }
            _ => report.corrupt_lines_dropped += 1,
        }
    }
    report.records_before = lines.len();

    let mut last_index_per_key: HashMap<String, usize> = HashMap::new();
    for (index, (_, key)) in lines.iter().enumerate() {
        if let Some(key) = key {
            last_index_per_key.insert(key.clone(), index);
        }
    }

    let tmp_path = dir.join(format!(".{kind}.jsonl.compact"));
    {
        let mut tmp = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)?;
        for (index, (line, key)) in lines.iter().enumerate() {
            let keep = match key {
                Some(key) => last_index_per_key.get(key) == Some(&index),
                // Records without the key field are kept verbatim: compaction
                // must never destroy data it does not understand.
                None => true,
            };
            if keep {
                tmp.write_all(line)?;
                tmp.write_all(b"\n")?;
                report.records_after += 1;
            }
        }
        tmp.sync_data()?;
    }
    std::fs::rename(&tmp_path, &path)?;

    Ok(report)
}
