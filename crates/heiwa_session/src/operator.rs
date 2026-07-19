//! Sole-writer operator session service.
//!
//! Wraps a [`heiwa_evidence::OperatorJournal`] with the in-process
//! invariants a single runtime process must enforce on top of a dumb
//! append-only log: idempotent turn submission, event validation, and
//! deterministic materialization of threads/turns from the raw event
//! stream.
//!
//! The journal itself is the *only* durable truth. Nothing here caches
//! state across calls: writers take the service transaction mutex, fold
//! whatever is on disk right now, and append before releasing it. Reads
//! replay the journal without that mutex; [`OperatorJournal`] already owns
//! its append-side lock. That keeps writer read-modify-append sequences
//! atomic after a crash with no recovery step beyond
//! [`OperatorSessionService::recover_interrupted`], while retaining
//! lock-free read-only replay.
//!
//! Cross-process duplicate submission is explicitly best-effort: the mutex
//! here only serializes calls within one process. The sole-writer contract
//! (exactly one runtime process holds the journal) lands in a later task;
//! until then, two processes racing `start_turn` against the same journal
//! could both observe "no existing turn" and each append one. Single
//! in-process callers are race-free.
//!
//! Cancellation contract: operator cancellation makes intent durable
//! FIRST — a `turn_cancel_requested` event is appended *before* the runner
//! is signalled — and the cancelled turn then terminates as
//! `turn_interrupted` with payload reason `OPERATOR_CANCELLED`.
//! `turn_cancel_requested` is therefore never terminal itself: it records
//! intent, and the follow-up `turn_interrupted` is the closing record —
//! the same closure shape [`OperatorSessionService::recover_interrupted`]
//! appends with reason `RUNTIME_RESTART`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use heiwa_evidence::{
    now_iso, CursorError, CursorEvent, OperatorActor, OperatorEvent, OperatorEventType,
    OperatorJournal, OperatorPage, OperatorRisk, OperatorSensitivity,
    OPERATOR_EVENT_SCHEMA_VERSION,
};

/// How a turn should be routed to a provider/model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    Auto,
    LocalOnly,
    RemoteOnly,
    Explicit,
}

/// Routing constraints attached to a single turn. Carried verbatim inside
/// the `turn_started` event payload so it is durable and replayable, and
/// echoed back on [`OperatorTurnView`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TurnRoutePolicy {
    pub mode: RouteMode,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub allowed_models: Vec<String>,
    pub excluded_models: Vec<String>,
    pub minimum_quality_class: u8,
    pub maximum_marginal_cost_usd: Option<f64>,
    pub turn_budget_usd: Option<f64>,
    pub privacy: String,
}

/// A caller's request to start (or resubmit) one turn.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StartTurnRequest {
    pub client_request_id: String,
    pub prompt: String,
    pub route_policy: TurnRoutePolicy,
}

impl StartTurnRequest {
    /// Convenience constructor for the common case: automatic routing, no
    /// caller-specified spending ceiling, standard privacy tier, and the
    /// lowest quality floor (`1`, i.e. no floor beyond "works").
    pub fn auto(client_request_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            client_request_id: client_request_id.into(),
            prompt: prompt.into(),
            route_policy: TurnRoutePolicy {
                mode: RouteMode::Auto,
                preferred_provider: None,
                preferred_model: None,
                allowed_models: Vec::new(),
                excluded_models: Vec::new(),
                minimum_quality_class: 1,
                maximum_marginal_cost_usd: None,
                turn_budget_usd: None,
                privacy: "standard".to_string(),
            },
        }
    }
}

/// Result of [`OperatorSessionService::start_turn`].
///
/// `cursor` is the cursor positioned immediately after the turn's
/// `user_message` append — i.e. the boundary a caller should hand to
/// [`OperatorSessionService::events_after`] to observe exactly that turn's
/// execution events (route/tool/assistant/completion) without replaying the
/// submission itself. For a duplicate submission this is the *stored*
/// cursor recorded when the original `user_message` was appended, which is
/// why materialization retains a per-turn `user_message` cursor: it is the
/// only way to serve this idempotently without re-appending anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSubmission {
    pub thread_id: String,
    pub turn_id: String,
    pub cursor: String,
    pub duplicate: bool,
}

/// Materialized view of one turn, folded from the operator event stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorTurnView {
    pub turn_id: String,
    pub client_request_id: Option<String>,
    /// One of `"open"`, `"completed"`, `"interrupted"`, `"blocked"`. The
    /// latter three are terminal (see [`is_turn_terminal`]).
    pub status: String,
    pub prompt: Option<String>,
    /// Cursor positioned immediately after this turn's `user_message`
    /// append. `None` only if materialization somehow never saw a
    /// `user_message` for a `turn_started` it did see (should not happen
    /// via `start_turn`, but replay stays defensive about it).
    pub user_message_cursor: Option<String>,
    pub started_at: String,
    pub route_policy: Option<TurnRoutePolicy>,
}

/// Materialized view of one thread: its turns in creation order, plus two
/// tolerance counters — one per damage domain — for anything that could
/// not be projected. Neither counter ever fails a read (write-side is
/// strict via [`OperatorSessionService::append_event`], read-side is
/// tolerant).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorThreadView {
    pub thread_id: String,
    pub turns: Vec<OperatorTurnView>,
    /// Schema/state-level rejects: events whose line parsed fine but that
    /// could not be projected — unsupported schema versions, or
    /// nonterminal events addressed to a turn that was already terminal.
    /// Thread-scoped, because a parsed event carries its `thread_id`.
    pub skipped_events: usize,
    /// Journal-level damage: lines in the underlying stream that never
    /// parsed as operator events at all — torn tails, garbage bytes, or
    /// envelopes whose `event_type` string is unknown to this build. A
    /// damaged line has no readable `thread_id`, so this count is
    /// stream-wide, not thread-scoped: every thread view (including views
    /// of threads with no events yet) reports the same number.
    pub skipped_lines: usize,
}

/// Lightweight summary of one thread for [`OperatorSessionService::list_threads`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorThreadSummary {
    pub thread_id: String,
    pub turn_count: usize,
    pub latest_turn_id: Option<String>,
    pub latest_status: Option<String>,
}

/// Sole-writer service over one [`OperatorJournal`]. See the module docs for
/// the durability and concurrency contract.
#[derive(Debug)]
pub struct OperatorSessionService {
    journal: OperatorJournal,
    /// Serializes in-process read-modify-append writer transactions. It is
    /// deliberately separate from the journal's own append lock, so replay
    /// and materialization never wait behind a slow writer.
    write_transaction: Mutex<()>,
}

impl OperatorSessionService {
    pub fn new(journal: OperatorJournal) -> Self {
        Self {
            journal,
            write_transaction: Mutex::new(()),
        }
    }

    /// Start a turn, or return the existing one if `request.client_request_id`
    /// already has a matching `turn_started` in this thread.
    ///
    /// Holds the service mutex for the whole read-modify-write: materialize
    /// the thread, check for a duplicate, and (if none) append
    /// `thread_created` (only the first time a thread is used), then
    /// `turn_started`, then `user_message`. Idempotency is decided purely by
    /// what is already on disk, per the module docs.
    pub fn start_turn(&self, thread_id: &str, request: StartTurnRequest) -> Result<TurnSubmission> {
        let _write_transaction = self.lock_write_transaction()?;
        let threads = materialize_all(&self.journal)?.threads;

        if let Some(folded) = threads.get(thread_id) {
            if let Some(turn) = folded.turns.iter().find(|turn| {
                turn.client_request_id.as_deref() == Some(request.client_request_id.as_str())
            }) {
                let cursor = turn.user_message_cursor.clone().ok_or_else(|| {
                    anyhow!(
                        "duplicate turn {} for thread {thread_id} has no recorded user_message cursor",
                        turn.turn_id
                    )
                })?;
                return Ok(TurnSubmission {
                    thread_id: thread_id.to_string(),
                    turn_id: turn.turn_id.clone(),
                    cursor,
                    duplicate: true,
                });
            }
        }

        let thread_exists = threads.contains_key(thread_id);
        let now = now_iso();
        let operator_actor = OperatorActor {
            kind: "operator".to_string(),
            id: "local-operator".to_string(),
        };

        if !thread_exists {
            let created = new_event(
                thread_id,
                None,
                None,
                OperatorEventType::ThreadCreated,
                now.clone(),
                operator_actor.clone(),
                json!({}),
            );
            self.journal.append(&created)?;
        }

        let turn_id = deterministic_turn_id(thread_id, &request.client_request_id);

        let turn_started = new_event(
            thread_id,
            Some(turn_id.clone()),
            None,
            OperatorEventType::TurnStarted,
            now.clone(),
            operator_actor.clone(),
            json!({
                "client_request_id": request.client_request_id,
                "route_policy": request.route_policy,
            }),
        );
        self.journal.append(&turn_started)?;

        let user_message = new_event(
            thread_id,
            Some(turn_id.clone()),
            None,
            OperatorEventType::UserMessage,
            now,
            operator_actor,
            json!({ "text": request.prompt }),
        );
        let appended = self.journal.append(&user_message)?;

        Ok(TurnSubmission {
            thread_id: thread_id.to_string(),
            turn_id,
            cursor: appended.cursor,
            duplicate: false,
        })
    }

    /// Append one caller-constructed event after validating it. See
    /// [`validate_event`] for the exact rules. Write-side is strict: a
    /// rejected event never reaches the journal.
    pub fn append_event(&self, event: OperatorEvent) -> Result<CursorEvent> {
        let _write_transaction = self.lock_write_transaction()?;
        let threads = materialize_all(&self.journal)?.threads;
        validate_event(&threads, &event)?;
        self.journal.append(&event)
    }

    /// Replay events for one thread starting strictly after `cursor`.
    ///
    /// Reads the underlying journal in fixed-size pages (bounded, not
    /// `usize::MAX`) and filters each page down to rows matching
    /// `thread_id`, advancing the cursor across nonmatching rows from other
    /// threads so a caller polling this thread's cursor neither misses a
    /// later matching row nor rereads unrelated rows on every poll. Stops
    /// as soon as `limit` matching events have been collected or the
    /// journal is exhausted.
    pub fn events_after(
        &self,
        thread_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> std::result::Result<OperatorPage, CursorError> {
        const PAGE_SIZE: usize = 256;

        let mut collected: Vec<CursorEvent> = Vec::new();
        let mut skipped_lines = 0usize;
        let mut raw_cursor: Option<String> = cursor.map(str::to_string);
        let mut next_cursor: Option<String> = raw_cursor.clone();

        if limit == 0 {
            return Ok(OperatorPage {
                events: collected,
                next_cursor,
                skipped_lines,
            });
        }

        loop {
            let page = self.journal.read_after(raw_cursor.as_deref(), PAGE_SIZE)?;
            if page.events.is_empty() {
                // EOF (or a stalled torn tail): fully consumed, safe to fold
                // in its skip count. next_cursor echoes the input per the
                // journal's own contract.
                skipped_lines += page.skipped_lines;
                next_cursor = page.next_cursor;
                break;
            }

            let mut reached_limit_at: Option<usize> = None;
            for (index, row) in page.events.iter().enumerate() {
                if row.event.thread_id == thread_id {
                    collected.push(row.clone());
                    if collected.len() == limit {
                        reached_limit_at = Some(index);
                        break;
                    }
                }
            }

            if let Some(index) = reached_limit_at {
                // Stop exactly at the limit-th match's own cursor, not at
                // the raw page boundary, so a resumed read never skips
                // matching rows that happened to share this page.
                next_cursor = Some(page.events[index].cursor.clone());
                if index == page.events.len() - 1 {
                    skipped_lines += page.skipped_lines;
                }
                break;
            }

            // Whole page scanned without reaching the limit: every row in
            // it (matching or not) has now been yielded to this call, so
            // its skip count is safe to fold in and we can advance past it
            // entirely. `next_cursor` is left alone here — it is only ever
            // read after a `break`, and both break paths set it themselves,
            // so updating it on this non-terminal path would just be a
            // dead store.
            skipped_lines += page.skipped_lines;
            raw_cursor = page.next_cursor;
        }

        Ok(OperatorPage {
            events: collected,
            next_cursor,
            skipped_lines,
        })
    }

    /// Materialized view of one thread. Threads with no events yet return
    /// an empty view rather than an error. `skipped_lines` on the view is
    /// stream-wide journal damage (see [`OperatorThreadView::skipped_lines`])
    /// and is reported even on the empty-thread branch.
    pub fn thread(&self, thread_id: &str) -> Result<OperatorThreadView> {
        let materialized = materialize_all(&self.journal)?;
        Ok(match materialized.threads.get(thread_id) {
            Some(folded) => folded.to_view(materialized.skipped_lines),
            None => OperatorThreadView {
                thread_id: thread_id.to_string(),
                turns: Vec::new(),
                skipped_events: 0,
                skipped_lines: materialized.skipped_lines,
            },
        })
    }

    /// Summaries of the most recently active threads, most recent first,
    /// bounded to `limit`.
    pub fn list_threads(&self, limit: usize) -> Result<Vec<OperatorThreadSummary>> {
        let threads = materialize_all(&self.journal)?.threads;
        let mut folded: Vec<&FoldedThread> = threads.values().collect();
        folded.sort_by_key(|thread| std::cmp::Reverse(thread.last_order));
        Ok(folded
            .into_iter()
            .take(limit)
            .map(FoldedThread::to_summary)
            .collect())
    }

    /// Close out every nonterminal turn with a `turn_interrupted` event
    /// (`reason: RUNTIME_RESTART`), as if the runtime had just restarted.
    /// Idempotent: once every turn is terminal, subsequent calls return
    /// `0` and append nothing.
    pub fn recover_interrupted(&self) -> Result<usize> {
        let _write_transaction = self.lock_write_transaction()?;
        let threads = materialize_all(&self.journal)?.threads;
        let runtime_actor = OperatorActor {
            kind: "runtime".to_string(),
            id: "heiwa-core".to_string(),
        };

        let mut closed = 0usize;
        // BTreeMap iteration would be nicer for determinism, but HashMap is
        // fine here: each closure is independent and order does not affect
        // the result, only which event lands first in the journal.
        for (thread_id, folded) in &threads {
            for turn in &folded.turns {
                if is_turn_terminal(&turn.status) {
                    continue;
                }
                let event = new_event(
                    thread_id,
                    Some(turn.turn_id.clone()),
                    None,
                    OperatorEventType::TurnInterrupted,
                    now_iso(),
                    runtime_actor.clone(),
                    json!({ "reason": "RUNTIME_RESTART" }),
                );
                self.journal.append(&event)?;
                closed += 1;
            }
        }
        Ok(closed)
    }

    fn lock_write_transaction(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.write_transaction
            .lock()
            .map_err(|_| anyhow!("operator session write transaction mutex poisoned"))
    }
}

// ---------------------------------------------------------------------
// Event construction.
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn new_event(
    thread_id: &str,
    turn_id: Option<String>,
    call_id: Option<String>,
    event_type: OperatorEventType,
    occurred_at: String,
    actor: OperatorActor,
    payload: serde_json::Value,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: Uuid::new_v4().to_string(),
        thread_id: thread_id.to_string(),
        turn_id,
        run_id: None,
        call_id,
        event_type,
        occurred_at,
        actor,
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

/// Deterministic turn id derived from `(thread_id, client_request_id)`.
///
/// Idempotency is decided by materializing the journal and matching on the
/// stored `client_request_id` (see [`OperatorSessionService::start_turn`]);
/// this determinism is a defense-in-depth property on top of that, not a
/// substitute for it. It gives the same `client_request_id` resubmitted
/// against the same thread a stable, reproducible turn id even if the
/// journal were ever inspected out of band, and makes the id collision-free
/// across threads without a shared counter.
fn deterministic_turn_id(thread_id: &str, client_request_id: &str) -> String {
    let namespace = operator_turn_namespace();
    // A unit separator can't appear in either input via normal usage and
    // keeps `("a", "b:c")` distinct from `("a:b", "c")`.
    let name = format!("{thread_id}\u{1f}{client_request_id}");
    Uuid::new_v5(&namespace, name.as_bytes()).to_string()
}

/// Fixed namespace UUID for turn-id derivation, itself derived (once per
/// call; cheap) from a SHA-256 of a stable domain string rather than a
/// hand-picked literal, so it is reproducible from source alone.
fn operator_turn_namespace() -> Uuid {
    let digest = Sha256::digest(b"heiwa.operator.turn.v1");
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

// ---------------------------------------------------------------------
// Validation (write-side, strict).
// ---------------------------------------------------------------------

/// Validate one event against the current materialized state before it is
/// allowed to reach the journal. `event_type` itself is always "known" by
/// construction (it is a closed Rust enum), so there is nothing to check
/// there beyond what the type system already guarantees; this function
/// covers schema version, required identifiers, and terminal-state
/// transitions.
fn validate_event(threads: &HashMap<String, FoldedThread>, event: &OperatorEvent) -> Result<()> {
    if event.schema_version != OPERATOR_EVENT_SCHEMA_VERSION {
        bail!(
            "rejected operator event {}: unsupported schema_version {} (expected {OPERATOR_EVENT_SCHEMA_VERSION})",
            event.event_id,
            event.schema_version
        );
    }

    if requires_turn_id(&event.event_type) && event.turn_id.is_none() {
        bail!(
            "rejected operator event {}: event type {:?} requires turn_id",
            event.event_id,
            event.event_type
        );
    }

    if requires_call_id(&event.event_type) && event.call_id.is_none() {
        bail!(
            "rejected operator event {}: event type {:?} requires call_id",
            event.event_id,
            event.event_type
        );
    }

    if event.event_type == OperatorEventType::TurnStarted {
        validate_turn_started(threads, event)?;
    } else if let Some(turn_id) = &event.turn_id {
        let turn = threads
            .get(&event.thread_id)
            .and_then(|folded| folded.turns.iter().find(|turn| &turn.turn_id == turn_id))
            .ok_or_else(|| {
                anyhow!(
                    "rejected operator event {}: turn {turn_id} does not exist in thread {}",
                    event.event_id,
                    event.thread_id
                )
            })?;

        if is_turn_terminal(&turn.status) && !is_terminal_event(&event.event_type) {
            bail!(
                "rejected operator event {}: turn {turn_id} is already terminal ({}); nonterminal event {:?} rejected",
                event.event_id,
                turn.status,
                event.event_type
            );
        }
    }

    Ok(())
}

/// A `turn_started` event is the one deliberate creation exception: legacy
/// import may synthesize a turn before its other historical events replay.
/// Once created, both its turn id and any supplied client request id are
/// immutable identities within the thread.
fn validate_turn_started(
    threads: &HashMap<String, FoldedThread>,
    event: &OperatorEvent,
) -> Result<()> {
    let turn_id = event
        .turn_id
        .as_deref()
        .expect("requires_turn_id checked before turn_started validation");
    let Some(thread) = threads.get(&event.thread_id) else {
        return Ok(());
    };

    if thread.turns.iter().any(|turn| turn.turn_id == turn_id) {
        bail!(
            "rejected operator event {}: turn {turn_id} already exists in thread {}",
            event.event_id,
            event.thread_id
        );
    }

    let client_request_id = event
        .payload
        .get("client_request_id")
        .and_then(|value| value.as_str());
    if let Some(client_request_id) = client_request_id {
        if thread
            .turns
            .iter()
            .any(|turn| turn.client_request_id.as_deref() == Some(client_request_id))
        {
            bail!(
                "rejected operator event {}: client_request_id {client_request_id:?} already belongs to a turn in thread {}",
                event.event_id,
                event.thread_id
            );
        }
    }

    Ok(())
}

fn requires_turn_id(event_type: &OperatorEventType) -> bool {
    matches!(
        event_type,
        OperatorEventType::TurnStarted
            | OperatorEventType::TurnCompleted
            | OperatorEventType::TurnCancelRequested
            | OperatorEventType::TurnInterrupted
            | OperatorEventType::RoutePlanned
            | OperatorEventType::RouteAttempted
            | OperatorEventType::RouteCompleted
            | OperatorEventType::RouteFailed
            | OperatorEventType::AssistantStarted
            | OperatorEventType::AssistantCompleted
            | OperatorEventType::ToolCallStarted
            | OperatorEventType::ToolCallCompleted
    )
}

fn requires_call_id(event_type: &OperatorEventType) -> bool {
    matches!(
        event_type,
        OperatorEventType::RoutePlanned
            | OperatorEventType::RouteAttempted
            | OperatorEventType::RouteCompleted
            | OperatorEventType::RouteFailed
            | OperatorEventType::ToolCallStarted
            | OperatorEventType::ToolCallCompleted
    )
}

/// The three event types that close a turn out. `Blocker` may or may not
/// carry a `turn_id` (it is not in [`requires_turn_id`]'s list, since a
/// blocker can be thread-scoped); when it does target a turn, that turn
/// becomes terminal in status `"blocked"`.
///
/// `TurnCancelRequested` is deliberately absent: per the cancellation
/// contract (module docs), it is appended before the runner is signalled
/// and only records operator *intent* — the turn stays open until the
/// resulting `turn_interrupted` (payload reason `OPERATOR_CANCELLED`)
/// lands as the actual closing record.
fn is_terminal_event(event_type: &OperatorEventType) -> bool {
    matches!(
        event_type,
        OperatorEventType::TurnCompleted
            | OperatorEventType::TurnInterrupted
            | OperatorEventType::Blocker
    )
}

fn is_turn_terminal(status: &str) -> bool {
    matches!(status, "completed" | "interrupted" | "blocked")
}

// ---------------------------------------------------------------------
// Materialization (read-side, tolerant).
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FoldedTurn {
    turn_id: String,
    client_request_id: Option<String>,
    status: String,
    prompt: Option<String>,
    user_message_cursor: Option<String>,
    started_at: String,
    route_policy: Option<TurnRoutePolicy>,
}

#[derive(Debug, Clone)]
struct FoldedThread {
    thread_id: String,
    turns: Vec<FoldedTurn>,
    skipped_events: usize,
    /// Position of the last event this thread was touched by, in global
    /// append order. Used only to order [`OperatorSessionService::list_threads`]
    /// by recency; never exposed directly.
    last_order: usize,
}

impl FoldedThread {
    fn new(thread_id: &str) -> Self {
        Self {
            thread_id: thread_id.to_string(),
            turns: Vec::new(),
            skipped_events: 0,
            last_order: 0,
        }
    }

    /// `skipped_lines` is passed in rather than stored: it is stream-wide
    /// (owned by [`MaterializedJournal`]), not per-thread state.
    fn to_view(&self, skipped_lines: usize) -> OperatorThreadView {
        OperatorThreadView {
            thread_id: self.thread_id.clone(),
            turns: self
                .turns
                .iter()
                .map(|turn| OperatorTurnView {
                    turn_id: turn.turn_id.clone(),
                    client_request_id: turn.client_request_id.clone(),
                    status: turn.status.clone(),
                    prompt: turn.prompt.clone(),
                    user_message_cursor: turn.user_message_cursor.clone(),
                    started_at: turn.started_at.clone(),
                    route_policy: turn.route_policy.clone(),
                })
                .collect(),
            skipped_events: self.skipped_events,
            skipped_lines,
        }
    }

    fn to_summary(&self) -> OperatorThreadSummary {
        OperatorThreadSummary {
            thread_id: self.thread_id.clone(),
            turn_count: self.turns.len(),
            latest_turn_id: self.turns.last().map(|turn| turn.turn_id.clone()),
            latest_status: self.turns.last().map(|turn| turn.status.clone()),
        }
    }
}

/// Result of folding the whole journal: per-thread projections plus the
/// stream-wide count of journal-level damage encountered along the way.
#[derive(Debug)]
struct MaterializedJournal {
    threads: HashMap<String, FoldedThread>,
    /// Deduplicated count of damaged journal lines (see
    /// [`OperatorThreadView::skipped_lines`] for what qualifies).
    skipped_lines: usize,
}

/// Fold the entire operator journal into per-thread state.
///
/// Applies, in append order: reader-side dedup of repeated `event_id`
/// values, skip-and-count for unsupported schema versions, and skip-and-
/// count for nonterminal events addressed to a turn that is already
/// terminal. Journal-level damage (lines that never parse as events) is
/// accumulated separately into `skipped_lines`. Never fails on content —
/// only a genuine I/O/storage error from the underlying journal
/// propagates.
fn materialize_all(journal: &OperatorJournal) -> Result<MaterializedJournal> {
    const PAGE_SIZE: usize = 256;
    let mut threads: HashMap<String, FoldedThread> = HashMap::new();
    let mut seen_event_ids: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = None;
    let mut order = 0usize;
    let mut skipped_lines = 0usize;
    // Whether the previous page returned events but fewer than PAGE_SIZE.
    // Such a page stopped at end-of-stream (or a torn tail), meaning it
    // already inspected — and counted — every damaged line between its
    // last event and the end of the file. The journal cursor only advances
    // to the last *event*, so the follow-up read that terminates this loop
    // re-inspects exactly that trailing span; folding its count in again
    // would double-count the same damage.
    let mut prev_page_was_short = false;

    loop {
        let page = journal.read_after(cursor.as_deref(), PAGE_SIZE)?;
        if page.events.is_empty() {
            if !prev_page_was_short {
                skipped_lines += page.skipped_lines;
            }
            break;
        }
        skipped_lines += page.skipped_lines;
        prev_page_was_short = page.events.len() < PAGE_SIZE;
        for row in &page.events {
            order += 1;
            apply_event(&mut threads, &mut seen_event_ids, row, order);
        }
        cursor = page.next_cursor.clone();
    }

    Ok(MaterializedJournal {
        threads,
        skipped_lines,
    })
}

fn apply_event(
    threads: &mut HashMap<String, FoldedThread>,
    seen_event_ids: &mut HashSet<String>,
    row: &CursorEvent,
    order: usize,
) {
    let event = &row.event;
    if !seen_event_ids.insert(event.event_id.clone()) {
        return; // Reader-side dedup of a repeated event_id.
    }

    let entry = threads
        .entry(event.thread_id.clone())
        .or_insert_with(|| FoldedThread::new(&event.thread_id));
    entry.last_order = order;

    if event.schema_version != OPERATOR_EVENT_SCHEMA_VERSION {
        entry.skipped_events += 1;
        return;
    }

    match event.event_type {
        OperatorEventType::ThreadCreated => {}
        OperatorEventType::TurnStarted => apply_turn_started(entry, event),
        OperatorEventType::UserMessage => apply_user_message(entry, event, row),
        OperatorEventType::TurnCompleted => apply_terminal(entry, event, "completed"),
        OperatorEventType::TurnInterrupted => apply_terminal(entry, event, "interrupted"),
        OperatorEventType::Blocker => apply_terminal(entry, event, "blocked"),
        // `TurnCancelRequested` is intent, not closure: per the
        // cancellation contract (module docs) the turn stays open until
        // the follow-up `turn_interrupted` (reason `OPERATOR_CANCELLED`)
        // arrives, so it folds like any other nonterminal touch.
        OperatorEventType::TurnCancelRequested
        | OperatorEventType::RoutePlanned
        | OperatorEventType::RouteAttempted
        | OperatorEventType::RouteCompleted
        | OperatorEventType::RouteFailed
        | OperatorEventType::AssistantStarted
        | OperatorEventType::AssistantCompleted
        | OperatorEventType::ToolCallStarted
        | OperatorEventType::ToolCallCompleted
        | OperatorEventType::ApprovalRequested
        | OperatorEventType::ApprovalDecided
        | OperatorEventType::ArtifactCreated
        | OperatorEventType::TestResult
        | OperatorEventType::ReceiptLinked
        | OperatorEventType::LegacySessionImported => apply_nonterminal_touch(entry, event),
    }
}

fn apply_turn_started(entry: &mut FoldedThread, event: &OperatorEvent) {
    let Some(turn_id) = event.turn_id.clone() else {
        entry.skipped_events += 1;
        return;
    };
    let client_request_id = event
        .payload
        .get("client_request_id")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    if entry.turns.iter().any(|turn| turn.turn_id == turn_id)
        || client_request_id
            .as_deref()
            .is_some_and(|client_request_id| {
                entry
                    .turns
                    .iter()
                    .any(|turn| turn.client_request_id.as_deref() == Some(client_request_id))
            })
    {
        entry.skipped_events += 1;
        return;
    }
    let route_policy = event
        .payload
        .get("route_policy")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    entry.turns.push(FoldedTurn {
        turn_id,
        client_request_id,
        status: "open".to_string(),
        prompt: None,
        user_message_cursor: None,
        started_at: event.occurred_at.clone(),
        route_policy,
    });
}

fn apply_user_message(entry: &mut FoldedThread, event: &OperatorEvent, row: &CursorEvent) {
    let Some(turn_id) = &event.turn_id else {
        entry.skipped_events += 1;
        return;
    };
    let Some(turn) = entry.turns.iter_mut().find(|turn| &turn.turn_id == turn_id) else {
        entry.skipped_events += 1;
        return;
    };
    if is_turn_terminal(&turn.status) {
        entry.skipped_events += 1;
        return;
    }
    turn.prompt = event
        .payload
        .get("text")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    turn.user_message_cursor = Some(row.cursor.clone());
}

fn apply_terminal(entry: &mut FoldedThread, event: &OperatorEvent, status: &str) {
    let Some(turn_id) = &event.turn_id else {
        // A thread-scoped Blocker (or a malformed terminal event) with no
        // turn to close: nothing to project, not counted as invalid.
        return;
    };
    let Some(turn) = entry.turns.iter_mut().find(|turn| &turn.turn_id == turn_id) else {
        entry.skipped_events += 1;
        return;
    };
    if is_turn_terminal(&turn.status) {
        // A second terminal event for an already-closed turn: keep the
        // first terminal state rather than clobbering it.
        entry.skipped_events += 1;
        return;
    }
    turn.status = status.to_string();
}

fn apply_nonterminal_touch(entry: &mut FoldedThread, event: &OperatorEvent) {
    let Some(turn_id) = &event.turn_id else {
        return; // Not turn-scoped (e.g. a thread-level note); nothing to touch.
    };
    match entry.turns.iter_mut().find(|turn| &turn.turn_id == turn_id) {
        Some(turn) if is_turn_terminal(&turn.status) => {
            entry.skipped_events += 1;
        }
        Some(_) => {}
        None => entry.skipped_events += 1,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use heiwa_evidence::OperatorJournal;

    use super::OperatorSessionService;

    #[test]
    fn read_only_replay_does_not_wait_for_a_write_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let service =
            OperatorSessionService::new(OperatorJournal::new(dir.path().to_path_buf()).unwrap());

        // Hold the service's writer transaction lock. A read-only replay
        // must still complete; the journal has its own append-side lock.
        let _write_transaction = service.write_transaction.lock().unwrap();
        let (sent, received) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| sent.send(service.thread("default")).unwrap());
            let view = received
                .recv_timeout(Duration::from_secs(2))
                .expect("read-only replay must not wait for a writer transaction")
                .unwrap();
            assert!(view.turns.is_empty());
        });
    }
}
