//! Operator journal: a dedicated, single-stream append log for the operator
//! event contract ([`OperatorEvent`]), plus versioned opaque cursors for
//! resumable, lock-free replay.
//!
//! This is deliberately its own structure rather than another
//! [`crate::journal::EvidenceTransport`] sink: the operator contract has its
//! own schema version, its own sensitive-material gate enforced before any
//! file touches disk, and a cursor-based read API that the generic
//! multi-kind transport does not need. It reuses the same on-disk envelope
//! shape and the same cross-process sidecar-lock discipline as every other
//! stream under the journal root (see [`crate::journal`]).
//!
//! Appends are dumb: serialize, gate, lock, one `write_all`, `sync_data`.
//! Reads are lock-free: open the data file directly, trust newline framing,
//! and treat an unparseable or torn tail line as skipped rather than fatal —
//! the same corruption tolerance philosophy as [`crate::replay`].

use std::fs::OpenOptions;
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::journal::{lock_stream, stream_path};
use crate::sensitive::find_sensitive;
use crate::{now_ms, EVIDENCE_SCHEMA_VERSION};

/// Version of the [`OperatorEvent`] contract itself (fields, enum variants,
/// required-ness). Distinct from `EVIDENCE_SCHEMA_VERSION`, which versions
/// the outer journal envelope every stream in this crate shares.
pub const OPERATOR_EVENT_SCHEMA_VERSION: u32 = 1;

/// Version of the opaque cursor encoding returned by [`OperatorJournal::append`]
/// and consumed by [`OperatorJournal::read_after`]. Bump if the cursor's
/// internal shape changes so old cursors are rejected as `InvalidCursor`
/// rather than silently misinterpreted.
pub const OPERATOR_CURSOR_VERSION: u8 = 1;

/// Stream kind / file basename (`<dir>/operator_events.jsonl`), in the same
/// `<kind>.jsonl` convention every other journal stream uses.
pub const OPERATOR_STREAM_KIND: &str = "operator_events";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperatorActor {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorSensitivity {
    PublicSafe,
    LocalPrivate,
    Restricted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorEventType {
    ThreadCreated,
    TurnStarted,
    UserMessage,
    RoutePlanned,
    RouteAttempted,
    RouteCompleted,
    RouteFailed,
    AssistantStarted,
    AssistantCompleted,
    ToolCallStarted,
    ToolCallCompleted,
    ApprovalRequested,
    ApprovalDecided,
    ArtifactCreated,
    TestResult,
    ReceiptLinked,
    Blocker,
    TurnCompleted,
    TurnCancelRequested,
    TurnInterrupted,
    LegacySessionImported,
}

/// One operator-facing runtime event: the durable record type appended to
/// the operator journal. `payload` carries event-specific detail and is
/// screened by [`find_sensitive`] before every append.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub run_id: Option<String>,
    pub call_id: Option<String>,
    pub event_type: OperatorEventType,
    pub occurred_at: String,
    pub actor: OperatorActor,
    pub risk_class: OperatorRisk,
    pub sensitivity: OperatorSensitivity,
    pub parent_event_id: Option<String>,
    pub correlation_id: Option<String>,
    pub source_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub payload: serde_json::Value,
}

/// Opaque cursor payload (base64-JSON encoded before it leaves this module).
/// `fingerprint` binds a cursor to one specific stream lineage: it is the
/// SHA-256 of the stream's first complete envelope line, which never
/// changes for the life of the file (this journal only ever appends).
/// `offset` is the byte position immediately after the last envelope the
/// cursor has already yielded, and always falls on a newline boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorCursor {
    version: u8,
    fingerprint: String,
    offset: u64,
}

/// One replayed event alongside the cursor positioned immediately after it.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorEvent {
    pub cursor: String,
    pub event: OperatorEvent,
}

/// A page of replayed operator events.
#[derive(Debug, Clone, PartialEq)]
pub struct OperatorPage {
    pub events: Vec<CursorEvent>,
    /// Cursor to pass into the next [`OperatorJournal::read_after`] call to
    /// continue monotonically. When this page returned events, it is the
    /// cursor after the last one. When it returned none — either the stream
    /// had nothing new past the input cursor, or `limit` was `0` — it is
    /// the *input* cursor, unchanged, so a caller can always assign
    /// `cursor = page.next_cursor` blindly in a polling loop without ever
    /// regressing to the start of the stream.
    pub next_cursor: Option<String>,
    /// Unparseable or torn lines encountered while producing this page.
    /// Bounded by the same range as `events`: lines past a `limit` cutoff
    /// are never inspected and so never counted here.
    pub skipped_lines: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("invalid_cursor: {reason}")]
    InvalidCursor { reason: String },
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

/// Dedicated append-only journal for the operator event contract.
///
/// Shares the on-disk envelope shape and cross-process sidecar-lock
/// discipline with every other stream under the journal root, but is not a
/// generic [`crate::journal::EvidenceTransport`] sink: it enforces the
/// sensitive-material gate before the stream file is ever created, and
/// exposes cursor-based, lock-free replay instead of a full-stream read.
#[derive(Debug)]
pub struct OperatorJournal {
    dir: PathBuf,
    write_lock: Mutex<()>,
}

impl OperatorJournal {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            write_lock: Mutex::new(()),
        })
    }

    /// Append one event and return the cursor positioned immediately after
    /// it. Rejects sensitive-looking payloads (see [`find_sensitive`])
    /// before the stream file is created or opened, so a rejected append
    /// leaves no trace on disk. Otherwise: one `write_all` of the envelope
    /// line under the cross-process sidecar lock, then `sync_data`.
    pub fn append(&self, event: &OperatorEvent) -> Result<CursorEvent> {
        if find_sensitive(&event.payload).is_some() {
            return Err(anyhow!(
                "refused to append operator event: payload contains sensitive material"
            ));
        }

        let line = json!({
            "v": EVIDENCE_SCHEMA_VERSION,
            "at_ms": now_ms(),
            "kind": OPERATOR_STREAM_KIND,
            "record": event,
        })
        .to_string();
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');

        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow!("operator journal write lock poisoned"))?;
        let _stream_lock = lock_stream(&self.dir, OPERATOR_STREAM_KIND)?;
        let path = stream_path(&self.dir, OPERATOR_STREAM_KIND);

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(&bytes)?;
        file.sync_data()?;
        let offset = file.metadata()?.len();
        drop(file);

        // Both locks are still held, so no concurrent writer could have
        // raced us between the write above and this read: the first line is
        // stable and matches whatever the stream's actual current lineage
        // fingerprint will be for the next `read_after` call.
        let fingerprint = first_line_fingerprint(&path)?;
        let cursor = encode_cursor(&OperatorCursor {
            version: OPERATOR_CURSOR_VERSION,
            fingerprint,
            offset,
        });

        Ok(CursorEvent {
            cursor,
            event: event.clone(),
        })
    }

    /// Replay up to `limit` events strictly after `cursor` (or from the
    /// start of the stream when `cursor` is `None`). Never takes the append
    /// lock: opens the data file directly and trusts newline framing,
    /// tolerating a torn tail the same way [`crate::replay::read_stream`]
    /// tolerates corruption elsewhere in the journal.
    pub fn read_after(&self, cursor: Option<&str>, limit: usize) -> Result<OperatorPage, CursorError> {
        let path = stream_path(&self.dir, OPERATOR_STREAM_KIND);
        let file_len = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let current_fingerprint = first_line_fingerprint(&path)?;

        let start_offset = match cursor {
            None => 0u64,
            Some(raw) => {
                let decoded = decode_cursor(raw)?;
                if decoded.version != OPERATOR_CURSOR_VERSION {
                    return Err(CursorError::InvalidCursor {
                        reason: format!(
                            "unsupported cursor version {} (expected {OPERATOR_CURSOR_VERSION})",
                            decoded.version
                        ),
                    });
                }
                if decoded.fingerprint != current_fingerprint {
                    return Err(CursorError::InvalidCursor {
                        reason: "cursor fingerprint does not match the current stream".to_string(),
                    });
                }
                if decoded.offset > file_len {
                    return Err(CursorError::InvalidCursor {
                        reason: "cursor offset is beyond the end of the stream".to_string(),
                    });
                }
                if !offset_preceded_by_newline(&path, decoded.offset)? {
                    return Err(CursorError::InvalidCursor {
                        reason: "cursor offset does not fall on a line boundary".to_string(),
                    });
                }
                decoded.offset
            }
        };

        let mut bytes = Vec::new();
        match OpenOptions::new().read(true).open(&path) {
            Ok(mut file) => {
                file.seek(SeekFrom::Start(start_offset))
                    .map_err(|err| CursorError::Storage(err.into()))?;
                file.read_to_end(&mut bytes)
                    .map_err(|err| CursorError::Storage(err.into()))?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(CursorError::Storage(err.into())),
        }

        let mut events = Vec::new();
        let mut skipped_lines = 0usize;
        let mut idx = 0usize;
        while idx < bytes.len() && events.len() < limit {
            let Some(rel_newline) = bytes[idx..].iter().position(|&b| b == b'\n') else {
                // Torn/incomplete tail: no trailing newline yet, so this is
                // not a complete record. Never fatal, just uncounted.
                skipped_lines += 1;
                break;
            };
            let line = &bytes[idx..idx + rel_newline];
            let next_offset = start_offset + (idx + rel_newline + 1) as u64;
            idx += rel_newline + 1;

            if line.is_empty() {
                continue;
            }
            match parse_operator_line(line) {
                Some(event) => {
                    let event_cursor = encode_cursor(&OperatorCursor {
                        version: OPERATOR_CURSOR_VERSION,
                        fingerprint: current_fingerprint.clone(),
                        offset: next_offset,
                    });
                    events.push(CursorEvent {
                        cursor: event_cursor,
                        event,
                    });
                }
                None => skipped_lines += 1,
            }
        }

        let next_cursor = match events.last() {
            Some(last) => Some(last.cursor.clone()),
            None => cursor.map(str::to_string),
        };

        Ok(OperatorPage {
            events,
            next_cursor,
            skipped_lines,
        })
    }
}

/// Parse one envelope line as an operator event. Any structural mismatch —
/// invalid JSON, wrong `kind`, a `record` that does not deserialize as
/// [`OperatorEvent`] — is reported as `None` (skipped), never a panic.
fn parse_operator_line(line: &[u8]) -> Option<OperatorEvent> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    if kind != OPERATOR_STREAM_KIND {
        return None;
    }
    let record = object.get("record")?.clone();
    serde_json::from_value(record).ok()
}

/// SHA-256 of the stream's first complete envelope line, or the literal
/// string `"empty"` when the stream has no valid first line yet (missing
/// file, zero-byte file, or an unterminated/unparseable first line).
fn first_line_fingerprint(path: &Path) -> Result<String> {
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok("empty".to_string()),
        Err(err) => return Err(err.into()),
    };
    let mut reader = std::io::BufReader::new(file);
    let mut line = Vec::new();
    let read = reader.read_until(b'\n', &mut line)?;
    if read == 0 || line.last() != Some(&b'\n') {
        return Ok("empty".to_string());
    }
    line.pop();
    if line.is_empty() || serde_json::from_slice::<serde_json::Value>(&line).is_err() {
        return Ok("empty".to_string());
    }
    Ok(sha256_hex(&line))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `offset` sits immediately after a `\n` byte (or is `0`, the start
/// of the file, which is always a valid boundary).
fn offset_preceded_by_newline(path: &Path, offset: u64) -> Result<bool> {
    if offset == 0 {
        return Ok(true);
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    file.seek(SeekFrom::Start(offset - 1))?;
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf)?;
    Ok(buf[0] == b'\n')
}

fn encode_cursor(cursor: &OperatorCursor) -> String {
    let json = serde_json::to_vec(cursor).expect("operator cursor always serializes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

fn decode_cursor(raw: &str) -> Result<OperatorCursor, CursorError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|err| CursorError::InvalidCursor {
            reason: format!("cursor is not valid base64: {err}"),
        })?;
    serde_json::from_slice(&bytes).map_err(|err| CursorError::InvalidCursor {
        reason: format!("cursor payload is malformed: {err}"),
    })
}
