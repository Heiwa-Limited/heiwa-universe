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

use anyhow::{anyhow, bail, Result};
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

/// Hard ceiling for one durable operator envelope, including its newline.
/// This keeps both append and cursor-boundary validation bounded even when a
/// stream file has been modified by an untrusted local process.
const MAX_OPERATOR_ENVELOPE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPERATOR_CORRUPT_LINES_PER_READ: usize = 1_024;
const MAX_OPERATOR_CORRUPT_BYTES_PER_READ: usize = 64 * 1024 * 1024;

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
/// the operator journal. The complete serialized event (metadata and
/// event-specific `payload`) is screened by [`find_sensitive`] before every
/// append.
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
/// SHA-256 of the stream's first valid operator envelope line, which never
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
    /// Corrupt scanning has fixed line/byte budgets in addition to the event
    /// limit; exceeding either budget fails closed.
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

    /// Evidence root used by ownership services layered above this dumb
    /// append/replay primitive.
    pub fn root(&self) -> &Path {
        &self.dir
    }

    /// Append one event and return the cursor positioned immediately after
    /// it. Rejects sensitive-looking event metadata or payloads (see
    /// [`find_sensitive`])
    /// before the stream file is created or opened, so a rejected append
    /// leaves no trace on disk. Otherwise: one `write_all` of the envelope
    /// line under the cross-process sidecar lock, then `sync_data`.
    pub fn append(&self, event: &OperatorEvent) -> Result<CursorEvent> {
        let serialized_event = serde_json::to_value(event)?;
        if find_sensitive(&serialized_event).is_some() {
            return Err(anyhow!(
                "refused to append operator event: event contains sensitive material"
            ));
        }

        let line = json!({
            "v": EVIDENCE_SCHEMA_VERSION,
            "at_ms": now_ms(),
            "kind": OPERATOR_STREAM_KIND,
            "record": serialized_event,
        })
        .to_string();
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        if bytes.len() > MAX_OPERATOR_ENVELOPE_BYTES {
            return Err(anyhow!(
                "refused to append operator event: envelope is too large ({} bytes; maximum {MAX_OPERATOR_ENVELOPE_BYTES})",
                bytes.len()
            ));
        }

        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow!("operator journal write lock poisoned"))?;
        let _stream_lock = lock_stream(&self.dir, OPERATOR_STREAM_KIND)?;
        let path = stream_path(&self.dir, OPERATOR_STREAM_KIND);

        // Detect hostile out-of-band prefixes before mutating the stream. If
        // the stream already has a valid lineage anchor, reuse it after the
        // append; otherwise the new event becomes the first valid anchor.
        let fingerprint_before = first_line_fingerprint(&path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        repair_unterminated_tail(&mut file)?;
        file.seek(SeekFrom::End(0))?;
        file.write_all(&bytes)?;
        file.sync_data()?;
        let offset = file.metadata()?.len();
        drop(file);

        // Both locks are still held, so no concurrent writer could have
        // raced us between the write above and this read: the first line is
        // stable and matches whatever the stream's actual current lineage
        // fingerprint will be for the next `read_after` call.
        let fingerprint = if fingerprint_before == "empty" {
            first_line_fingerprint(&path)?
        } else {
            fingerprint_before
        };
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
    ///
    /// Valid-event work is bounded by `limit`; invalid rows are independently
    /// bounded by fixed corrupt-line and corrupt-byte budgets. Each candidate
    /// line is capped at the append ceiling, so hostile out-of-band edits can
    /// neither grow memory without bound nor bypass `limit` forever.
    pub fn read_after(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<OperatorPage, CursorError> {
        let path = stream_path(&self.dir, OPERATOR_STREAM_KIND);
        let file_len = match std::fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => 0,
            Err(err) => return Err(CursorError::Storage(err.into())),
        };
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
                if !offset_follows_valid_operator_event(&path, decoded.offset)? {
                    return Err(CursorError::InvalidCursor {
                        reason: "cursor offset does not fall on a valid event boundary".to_string(),
                    });
                }
                decoded.offset
            }
        };

        let mut events = Vec::new();
        let mut skipped_lines = 0usize;
        match OpenOptions::new().read(true).open(&path) {
            Ok(mut file) => {
                file.seek(SeekFrom::Start(start_offset))
                    .map_err(|err| CursorError::Storage(err.into()))?;
                let mut reader = std::io::BufReader::new(file);
                let mut offset = start_offset;
                let mut line = Vec::new();
                let mut corrupt_budget = CorruptScanBudget::default();
                while events.len() < limit {
                    match read_capped_line(&mut reader, &mut line).map_err(CursorError::Storage)? {
                        CappedLine::Eof => break,
                        CappedLine::Torn => {
                            skipped_lines += 1;
                            corrupt_budget
                                .record(line.len())
                                .map_err(CursorError::Storage)?;
                            break;
                        }
                        CappedLine::Complete => {
                            offset += line.len() as u64;
                            let content = &line[..line.len() - 1];
                            match parse_operator_line(content) {
                                Some(event) => {
                                    let event_cursor = encode_cursor(&OperatorCursor {
                                        version: OPERATOR_CURSOR_VERSION,
                                        fingerprint: current_fingerprint.clone(),
                                        offset,
                                    });
                                    events.push(CursorEvent {
                                        cursor: event_cursor,
                                        event,
                                    });
                                }
                                None => {
                                    skipped_lines += 1;
                                    corrupt_budget
                                        .record(line.len())
                                        .map_err(CursorError::Storage)?;
                                }
                            }
                        }
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(CursorError::Storage(err.into())),
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

/// Under the writer locks, remove only bytes after the last complete newline.
/// A crash can leave an unterminated JSON line; appending to it would join two
/// envelopes into one permanently skipped record.
fn repair_unterminated_tail(file: &mut std::fs::File) -> Result<()> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(len - 1))?;
    let mut final_byte = [0u8; 1];
    file.read_exact(&mut final_byte)?;
    if final_byte[0] == b'\n' {
        return Ok(());
    }
    const CHUNK: usize = 8192;
    let mut end = len;
    let oldest_allowed_start = len.saturating_sub((MAX_OPERATOR_ENVELOPE_BYTES - 1) as u64);
    let complete_len = loop {
        let start = end.saturating_sub(CHUNK as u64).max(oldest_allowed_start);
        file.seek(SeekFrom::Start(start))?;
        let mut chunk = vec![0; (end - start) as usize];
        file.read_exact(&mut chunk)?;
        if let Some(index) = chunk.iter().rposition(|byte| *byte == b'\n') {
            break start + index as u64 + 1;
        }
        if start == 0 {
            break 0;
        }
        if start == oldest_allowed_start {
            file.seek(SeekFrom::Start(start - 1))?;
            let mut preceding = [0u8; 1];
            file.read_exact(&mut preceding)?;
            if preceding[0] == b'\n' {
                break start;
            }
            bail!(
                "operator journal torn tail exceeds maximum operator envelope of {MAX_OPERATOR_ENVELOPE_BYTES} bytes"
            );
        }
        end = start;
    };
    file.set_len(complete_len)?;
    Ok(())
}

/// Parse one envelope line as an operator event. Any structural mismatch —
/// invalid JSON, wrong `kind`, a `record` that does not deserialize as
/// [`OperatorEvent`] — is reported as `None` (skipped), never a panic.
fn parse_operator_line(line: &[u8]) -> Option<OperatorEvent> {
    #[derive(Deserialize)]
    struct OperatorEnvelope {
        v: u32,
        at_ms: u64,
        kind: String,
        record: OperatorEvent,
    }

    let envelope: OperatorEnvelope = serde_json::from_slice(line).ok()?;
    if envelope.v != EVIDENCE_SCHEMA_VERSION || envelope.kind != OPERATOR_STREAM_KIND {
        return None;
    }
    let _ = envelope.at_ms;
    Some(envelope.record)
}

/// SHA-256 of the stream's first valid, complete operator envelope line, or
/// the literal string `"empty"` when the stream has no valid anchor yet.
/// Corrupt prefixes are skipped only within the same fixed budget as replay.
fn first_line_fingerprint(path: &Path) -> Result<String> {
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok("empty".to_string()),
        Err(err) => return Err(err.into()),
    };
    let mut reader = std::io::BufReader::new(file);
    let mut line = Vec::new();
    let mut corrupt_budget = CorruptScanBudget::default();
    loop {
        match read_capped_line(&mut reader, &mut line)? {
            CappedLine::Eof => return Ok("empty".to_string()),
            CappedLine::Torn => {
                corrupt_budget.record(line.len())?;
                return Ok("empty".to_string());
            }
            CappedLine::Complete => {
                let content = &line[..line.len() - 1];
                if parse_operator_line(content).is_some() {
                    return Ok(sha256_hex(content));
                }
                corrupt_budget.record(line.len())?;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CappedLine {
    Eof,
    Complete,
    Torn,
}

/// Read one newline-framed envelope without allowing an out-of-band line to
/// grow `line` past the append ceiling. `line` includes the newline for a
/// complete record, matching the ceiling's definition.
fn read_capped_line<R: BufRead>(reader: &mut R, line: &mut Vec<u8>) -> Result<CappedLine> {
    line.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(if line.is_empty() {
                CappedLine::Eof
            } else {
                CappedLine::Torn
            });
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if line.len().saturating_add(take) > MAX_OPERATOR_ENVELOPE_BYTES {
            bail!(
                "operator journal line exceeds maximum operator envelope of {MAX_OPERATOR_ENVELOPE_BYTES} bytes"
            );
        }
        let complete = available[take - 1] == b'\n';
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if complete {
            return Ok(CappedLine::Complete);
        }
    }
}

#[derive(Debug, Default)]
struct CorruptScanBudget {
    lines: usize,
    bytes: usize,
}

impl CorruptScanBudget {
    fn record(&mut self, bytes: usize) -> Result<()> {
        self.lines = self.lines.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        if self.lines > MAX_OPERATOR_CORRUPT_LINES_PER_READ
            || self.bytes > MAX_OPERATOR_CORRUPT_BYTES_PER_READ
        {
            bail!("operator journal corrupt scan budget exceeded");
        }
        Ok(())
    }
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

/// Whether `offset` is the start of the stream or sits immediately after a
/// complete, parseable operator-event envelope. A newline alone is not a
/// valid cursor boundary: otherwise a caller could resume after a corrupt or
/// non-operator line and silently skip damaged evidence.
fn offset_follows_valid_operator_event(path: &Path, offset: u64) -> Result<bool> {
    if offset == 0 {
        return Ok(true);
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    file.seek(SeekFrom::Start(offset - 1))?;
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf)?;
    if buf[0] != b'\n' || offset == 1 {
        return Ok(false);
    }

    // Find the beginning of the preceding line without scanning from the
    // beginning of an append-forever stream. Only the candidate envelope is
    // read into memory.
    const CHUNK: u64 = 8192;
    let line_end = offset - 1;
    let mut search_end = line_end;
    let oldest_allowed_start = line_end.saturating_sub(MAX_OPERATOR_ENVELOPE_BYTES as u64 - 1);
    let line_start = loop {
        let search_start = search_end.saturating_sub(CHUNK).max(oldest_allowed_start);
        file.seek(SeekFrom::Start(search_start))?;
        let mut chunk = vec![0; usize::try_from(search_end - search_start)?];
        file.read_exact(&mut chunk)?;
        if let Some(index) = chunk.iter().rposition(|byte| *byte == b'\n') {
            break search_start + index as u64 + 1;
        }
        if search_start == 0 {
            break 0;
        }
        if search_start == oldest_allowed_start {
            file.seek(SeekFrom::Start(search_start - 1))?;
            let mut preceding = [0u8; 1];
            file.read_exact(&mut preceding)?;
            if preceding[0] == b'\n' {
                break search_start;
            }
            return Ok(false);
        }
        search_end = search_start;
    };
    if line_start == line_end {
        return Ok(false);
    }
    let line_len = usize::try_from(line_end - line_start)?;
    file.seek(SeekFrom::Start(line_start))?;
    let mut line = vec![0; line_len];
    file.read_exact(&mut line)?;
    Ok(parse_operator_line(&line).is_some())
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
