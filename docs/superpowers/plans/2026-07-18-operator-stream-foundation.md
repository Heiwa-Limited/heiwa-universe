# Operator Stream Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Heiwa Desktop, REPL, TUI, loops, and local API clients one authenticated, restart-safe operator stream while routing every model call through evidence-backed DREX value selection.

**Architecture:** `heiwa_evidence` supplies dumb append/replay framing, versioned cursors, and sensitive-material rejection. `heiwa_session::OperatorSessionService` is the sole domain writer and materializes threads, turns, compatibility transcripts, FTS, and Lance projections. `heiwa_core::drex` plans each model call; `heiwa_shell::model_calls` is the only execution path that invokes provider adapters. Shell HTTP/WebSocket and the Tauri bridge project the same journal into clients.

**Tech Stack:** Rust 2021, Tokio, JSONL, serde/serde_json, SQLite FTS5, Lance through `heiwa_embed`, Axum-compatible `heiwa_core::auth`, raw localhost HTTP/WebSocket in `heiwa-shell`, Tauri 2, TypeScript/Vite, Vitest.

## Global Constraints

- Canonical operator truth is local `~/.heiwa/evidence/operator_events.jsonl`; GitHub sync stays disabled.
- `operator_events.jsonl` is append-forever and must never use `compact_stream`.
- `heiwa_session` is the sole domain writer; `heiwa_evidence` remains a dumb framing/append service.
- Raw secrets, bearer tokens, credential-file contents, and provider auth material must fail the sensitive-material gate before append.
- Journal envelope `v` and operator event `schema_version` are distinct version domains.
- All `/api/v1/operator/*` HTTP and `/ws/v1/operator` connections require `heiwa_core::auth` machine-token or signed-session authentication.
- Tauri injects auth natively; machine token must never enter renderer assets or persisted operator events.
- Every inference call routes independently; explicit pins constrain DREX but never bypass privacy, capability, quota, safety, or spend gates.
- Cost truth must use `local_zero_cost`, `target_only`, `proxy_estimate`, `exact_provider_report`, or `cannot_confirm` honestly.
- Desktop renderer state is disposable; SQLite FTS and Lance are rebuildable projections.
- No UI framework, router library, or component library is added.
- Tests inject temporary evidence/session/index roots and must not mutate real `~/.heiwa` state.
- Verify checkout runtime on `7475`; do not change installed `7474` without explicit promotion approval.

---

## File Structure

### New files

| File | Responsibility |
| --- | --- |
| `crates/heiwa_evidence/src/sensitive.rs` | Shared secret/material scanner used before durable writes. |
| `crates/heiwa_evidence/src/operator.rs` | Operator event record, cursor, append, and lock-free replay framing. |
| `crates/heiwa_evidence/tests/operator_journal.rs` | Append, cursor, corruption, concurrency, and sensitive-payload tests. |
| `crates/heiwa_session/src/operator.rs` | Sole writer, thread/turn materializer, idempotency, recovery, transcript compatibility. |
| `crates/heiwa_session/src/operator_index.rs` | FTS/Lance projection and rebuild boundary. |
| `crates/heiwa_session/tests/operator_service.rs` | Domain writer, migration, index, and restart tests. |
| `apps/heiwa_core/src/drex/call.rs` | Per-call policy, candidate admission, cheapest-above-floor selection, fallback inputs. |
| `apps/heiwa_core/tests/drex_call_routing.rs` | Quality/cost/privacy/quota/override selection contracts. |
| `apps/heiwa_shell/src/model_calls.rs` | Only shell provider-call executor; routing events, usage, fallback, cancellation. |
| `apps/heiwa_shell/src/operator.rs` | Turn runner and active-turn registry over session/model-call services. |
| `apps/heiwa_shell/tests/operator_api.rs` | Authenticated HTTP, idempotency, replay, cancel, and compatibility tests. |
| `apps/heiwa_app/desktop/src-tauri/src/operator_stream.rs` | Authenticated native WebSocket-to-Tauri-channel bridge. |
| `apps/heiwa_app/desktop/src/operator/types.ts` | Operator wire and view types. |
| `apps/heiwa_app/desktop/src/operator/store.ts` | Pure idempotent event reducer. |
| `apps/heiwa_app/desktop/src/operator/client.ts` | HTTP submit/replay and native stream controller. |
| `apps/heiwa_app/desktop/src/operator/store.test.ts` | Replay and duplicate-suppression tests. |
| `scripts/check_model_call_boundary.sh` | Prevent direct provider sends outside the routed executor and provider adapters. |

### Modified files

| File | Change |
| --- | --- |
| `crates/heiwa_evidence/Cargo.toml` | Add base64/sha2 support used by opaque cursors and fingerprints. |
| `crates/heiwa_evidence/src/lib.rs` | Export operator and sensitive APIs; clarify envelope version name. |
| `crates/heiwa_evidence/src/journal.rs` | Expose internal append primitive to operator journal without domain logic. |
| `apps/heiwa_shell/src/cmd/capabilities.rs` | Reuse `heiwa_evidence::find_sensitive`. |
| `crates/heiwa_session/Cargo.toml` | Add `heiwa_evidence`, sha2, and UUID v5 dependencies. |
| `crates/heiwa_session/src/lib.rs` | Export operator service; convert legacy transcript calls into compatibility projections. |
| `crates/heiwa_session/src/migration.rs` | Produce deterministic import events and marker fingerprints. |
| `apps/heiwa_core/src/drex/mod.rs` | Export per-call routing contract. |
| `apps/heiwa_core/src/drex/router.rs` | Reuse existing ingress scoring inside candidate admission; remove cost-first score blending. |
| `crates/heiwa_loop/src/lib.rs` | Consume routed call executor instead of invoking adapters directly. |
| `crates/heiwa_loop/tests/loop_execution.rs` | Verify each loop iteration requests a fresh model call. |
| `apps/heiwa_shell/src/lib.rs` | Export `model_calls` and `operator`. |
| `apps/heiwa_shell/src/main.rs` | Route all REPL/TUI/loop calls through services and keep compatibility wrappers thin. |
| `apps/heiwa_shell/src/cmd/app.rs` | Authenticated operator HTTP/WS routing and lock-free cursor tailing. |
| `apps/heiwa_shell/tests/app_api.rs` | Verify CLI bearer injection and no secret leakage in dry-run JSON. |
| `apps/heiwa_app/desktop/src-tauri/Cargo.toml` | Add `futures-util` and `tokio-tungstenite` for native authenticated WS. |
| `apps/heiwa_app/desktop/src-tauri/src/proxy.rs` | Inject machine auth into HTTP calls. |
| `apps/heiwa_app/desktop/src-tauri/src/lib.rs` | Register native operator subscription command. |
| `apps/heiwa_app/desktop/src/runtime.ts` | Expose operator replay/submit/stream functions. |
| `apps/heiwa_app/desktop/src/main.ts` | Replace renderer-owned messages/tasks with operator store projection. |
| `apps/heiwa_app/desktop/package.json` | Add Vitest test script and dev dependency. |
| `apps/heiwa_app/desktop/package-lock.json` | Lock Vitest dependency. |
| `docs/architecture/app-foundation.md` | Replace remaining stale backend wording and document operator stream. |
| `docs/local-self-operation.md` | Add authenticated `7475` operator probes. |
| `scripts/check_agent_baseline.sh` | Run model-call boundary check. |

---

### Task 1: Secure Operator Journal And Versioned Cursors

**Files:**

- Create: `crates/heiwa_evidence/src/sensitive.rs`
- Create: `crates/heiwa_evidence/src/operator.rs`
- Create: `crates/heiwa_evidence/tests/operator_journal.rs`
- Modify: `crates/heiwa_evidence/Cargo.toml`
- Modify: `crates/heiwa_evidence/src/lib.rs`
- Modify: `crates/heiwa_evidence/src/journal.rs`
- Modify: `apps/heiwa_shell/src/cmd/capabilities.rs`

**Interfaces:**

- Produces: `find_sensitive(&Value) -> Option<SensitiveMatch>`.
- Produces: `OperatorJournal::new(PathBuf) -> Result<Self>`.
- Produces: `OperatorJournal::append(&OperatorEvent) -> Result<CursorEvent>`.
- Produces: `OperatorJournal::read_after(Option<&str>, usize) -> Result<OperatorPage, CursorError>`.
- Produces: `OperatorEvent`, `OperatorCursor`, `CursorEvent`, `OperatorPage`, `CursorError`.

- [ ] **Step 1: Write failing sensitive-material and cursor tests**

Create `crates/heiwa_evidence/tests/operator_journal.rs` with these cases:

```rust
use heiwa_evidence::{
    CursorError, OperatorActor, OperatorEvent, OperatorEventType, OperatorJournal,
    OperatorRisk, OperatorSensitivity,
};
use serde_json::json;

fn event(
    id: &str,
    thread: &str,
    kind: OperatorEventType,
    payload: serde_json::Value,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: 1,
        event_id: id.into(),
        thread_id: thread.into(),
        turn_id: Some("turn-1".into()),
        run_id: None,
        call_id: None,
        event_type: kind,
        occurred_at: "2026-07-18T00:00:00Z".into(),
        actor: OperatorActor { kind: "operator".into(), id: "local-operator".into() },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

#[test]
fn append_and_resume_from_versioned_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let first = journal.append(&event("e1", "thread-a", OperatorEventType::UserMessage, json!({"text":"hi"}))).unwrap();
    journal.append(&event("e2", "thread-a", OperatorEventType::AssistantCompleted, json!({"text":"hello"}))).unwrap();
    let page = journal.read_after(Some(&first.cursor), 50).unwrap();
    assert_eq!(page.events.iter().map(|row| row.event.event_id.as_str()).collect::<Vec<_>>(), vec!["e2"]);
}

#[test]
fn rejects_sensitive_payload_before_file_creation() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let error = journal.append(&event("e1", "thread-a", OperatorEventType::ToolCallCompleted, json!({"output":"Bearer live-token"}))).unwrap_err();
    assert!(error.to_string().contains("sensitive material"));
    assert!(!dir.path().join("operator_events.jsonl").exists());
}

#[test]
fn fingerprint_or_boundary_mismatch_is_structured() {
    let dir = tempfile::tempdir().unwrap();
    let journal = OperatorJournal::new(dir.path().to_path_buf()).unwrap();
    let row = journal.append(&event("e1", "thread-a", OperatorEventType::UserMessage, json!({"text":"hi"}))).unwrap();
    std::fs::write(dir.path().join("operator_events.jsonl"), "").unwrap();
    assert!(matches!(journal.read_after(Some(&row.cursor), 50), Err(CursorError::InvalidCursor { .. })));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p heiwa_evidence --test operator_journal`

Expected: FAIL because operator journal exports do not exist.

- [ ] **Step 3: Extract the shared sensitive-material gate**

Move basename/prefix policy from `apps/heiwa_shell/src/cmd/capabilities.rs` into `crates/heiwa_evidence/src/sensitive.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMatch {
    pub category: &'static str,
}

pub fn find_sensitive(value: &serde_json::Value) -> Option<SensitiveMatch> {
    match value {
        serde_json::Value::String(text) if sensitive_basename(text) => {
            Some(SensitiveMatch { category: "credential_path" })
        }
        serde_json::Value::String(text) if sensitive_prefix(text) => {
            Some(SensitiveMatch { category: "token_prefix" })
        }
        serde_json::Value::Array(values) => values.iter().find_map(find_sensitive),
        serde_json::Value::Object(values) => values.values().find_map(find_sensitive),
        _ => None,
    }
}
```

Keep the existing basename and prefix lists byte-for-byte. Change capability callers to `heiwa_evidence::find_sensitive(&value).is_some()` and retain their existing tests.

- [ ] **Step 4: Implement operator record and cursor types**

In `operator.rs`, define this public surface:

```rust
pub const OPERATOR_EVENT_SCHEMA_VERSION: u32 = 1;
pub const OPERATOR_CURSOR_VERSION: u8 = 1;
pub const OPERATOR_STREAM_KIND: &str = "operator_events";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OperatorActor { pub kind: String, pub id: String }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorRisk { Low, Medium, High, Critical }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorSensitivity { PublicSafe, LocalPrivate, Restricted }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct OperatorCursor { version: u8, fingerprint: String, offset: u64 }

#[derive(Debug, Clone, PartialEq)]
pub struct CursorEvent { pub cursor: String, pub event: OperatorEvent }

#[derive(Debug, Clone, PartialEq)]
pub struct OperatorPage { pub events: Vec<CursorEvent>, pub next_cursor: Option<String>, pub skipped_lines: usize }

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("invalid_cursor: {reason}")]
    InvalidCursor { reason: String },
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
```

Add `thiserror = "2"`, `base64 = "0.22"`, and `sha2 = "0.10"` to `heiwa_evidence`.

- [ ] **Step 5: Implement dumb append and lock-free replay**

`OperatorJournal::append` must serialize the existing envelope with `v: EVIDENCE_SCHEMA_VERSION`, call `find_sensitive` on `event.payload`, take the write-side sidecar lock, call one `write_all`, `sync_data`, and return the byte offset after the newline. `read_after` must not take the append lock. It validates cursor version, first-valid-event fingerprint, file length, and preceding newline boundary before reading complete newline-delimited records.

Fingerprint rule: SHA-256 of the first valid complete envelope line. Empty stream fingerprint is `empty`. A cursor returned after the first append uses that first-line fingerprint.

- [ ] **Step 6: Add corruption and concurrent-writer tests**

Add tests that append a truncated final line and assert earlier events plus `skipped_lines == 1`, and that four `OperatorJournal` instances append 100 events each with 400 valid unique event IDs.

- [ ] **Step 7: Run focused tests and commit**

Run: `cargo test -p heiwa_evidence`

Expected: all existing and new evidence tests PASS.

```bash
git add crates/heiwa_evidence apps/heiwa_shell/src/cmd/capabilities.rs Cargo.lock
git commit -m "Add secure operator journal primitives"
```

---

### Task 2: Sole-Writer Operator Session Service

**Files:**

- Create: `crates/heiwa_session/src/operator.rs`
- Create: `crates/heiwa_session/tests/operator_service.rs`
- Modify: `crates/heiwa_session/Cargo.toml`
- Modify: `crates/heiwa_session/src/lib.rs`

**Interfaces:**

- Consumes: `OperatorJournal`, `OperatorEvent`, `OperatorPage` from Task 1.
- Produces: `OperatorSessionService`, `OperatorThreadView`, `OperatorTurnView`, `TurnSubmission`, `TurnRoutePolicy`.
- Produces: `start_turn`, `append_event`, `events_after`, `list_threads`, `recover_interrupted`.

- [ ] **Step 1: Write failing materialization/idempotency/recovery tests**

Create `operator_service.rs` tests that prove:

```rust
#[test]
fn duplicate_client_request_returns_one_turn() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    let request = StartTurnRequest::auto("req-1", "hello");
    let first = service.start_turn("default", request.clone()).unwrap();
    let second = service.start_turn("default", request).unwrap();
    assert_eq!(first.turn_id, second.turn_id);
    assert!(second.duplicate);
    assert_eq!(service.thread("default").unwrap().turns.len(), 1);
}

#[test]
fn restart_closes_unfinished_turn_once() {
    let dir = tempfile::tempdir().unwrap();
    let service = test_service(dir.path());
    service.start_turn("default", StartTurnRequest::auto("req-1", "hello")).unwrap();
    assert_eq!(service.recover_interrupted().unwrap(), 1);
    assert_eq!(service.recover_interrupted().unwrap(), 0);
    assert_eq!(service.thread("default").unwrap().turns[0].status, "interrupted");
}
```

Use `OperatorSessionService::new(OperatorJournal::new(path.to_path_buf())?)`; do not mutate `HOME` in these tests.

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p heiwa-session --test operator_service`

Expected: FAIL because operator service types do not exist.

- [ ] **Step 3: Add session dependency and domain types**

Add `heiwa_evidence`, `sha2 = "0.10"`, and UUID feature `v5` to `crates/heiwa_session/Cargo.toml`. Define:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode { Auto, LocalOnly, RemoteOnly, Explicit }

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StartTurnRequest {
    pub client_request_id: String,
    pub prompt: String,
    pub route_policy: TurnRoutePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSubmission { pub thread_id: String, pub turn_id: String, pub cursor: String, pub duplicate: bool }
```

`StartTurnRequest::auto` sets mode `Auto`, minimum quality class `1`, privacy `standard`, and no explicit spending limit.

- [ ] **Step 4: Implement sole-writer methods**

`OperatorSessionService` owns `Mutex<OperatorJournal>`. `start_turn` holds the service mutex, materializes existing events, returns the existing turn on matching `client_request_id`, otherwise appends `thread_created` when needed, then `turn_started` and `user_message`. `append_event` validates known event type, schema version, required turn/call identifiers, and terminal-state transitions before calling journal append.

Expose exact methods:

```rust
impl OperatorSessionService {
    pub fn new(journal: OperatorJournal) -> Self;
    pub fn start_turn(&self, thread_id: &str, request: StartTurnRequest) -> anyhow::Result<TurnSubmission>;
    pub fn append_event(&self, event: OperatorEvent) -> anyhow::Result<CursorEvent>;
    pub fn events_after(&self, thread_id: &str, cursor: Option<&str>, limit: usize) -> Result<OperatorPage, CursorError>;
    pub fn thread(&self, thread_id: &str) -> anyhow::Result<OperatorThreadView>;
    pub fn list_threads(&self, limit: usize) -> anyhow::Result<Vec<OperatorThreadSummary>>;
    pub fn recover_interrupted(&self) -> anyhow::Result<usize>;
}
```

`events_after` iteratively reads globally ordered pages until it collects `limit` matching events or reaches EOF. It filters by `thread_id` while advancing the returned cursor across nonmatching events so clients neither miss later matching rows nor reread unrelated rows forever.

- [ ] **Step 5: Implement deterministic thread materialization**

Fold events by append order. Deduplicate repeated `event_id` values reader-side. Ignore events for unknown schema versions and increment `skipped_events`. A turn terminal state is one of `completed`, `interrupted`, or `blocked`; operator cancellation appends `turn_cancel_requested` before signalling the runner and ends as `turn_interrupted` with reason `OPERATOR_CANCELLED`. Later nonterminal events for a closed turn are counted as invalid and not projected.

- [ ] **Step 6: Run tests and commit**

Run: `cargo test -p heiwa-session --test operator_service`

Expected: PASS.

```bash
git add crates/heiwa_session Cargo.lock
git commit -m "Add sole-writer operator session service"
```

---

### Task 3: Legacy Import And Rebuildable FTS/Lance Projections

**Files:**

- Create: `crates/heiwa_session/src/operator_index.rs`
- Modify: `crates/heiwa_session/src/operator.rs`
- Modify: `crates/heiwa_session/src/lib.rs`
- Modify: `crates/heiwa_session/src/migration.rs`
- Modify: `crates/heiwa_session/tests/operator_service.rs`
- Modify: `crates/heiwa_session/tests/transcript_migration.rs`

**Interfaces:**

- Produces: `import_legacy_sessions(&Path) -> Result<ImportReport>`.
- Produces: `rebuild_operator_indexes(&OperatorSessionService, &dyn EmbeddingSink) -> Result<IndexReport>`.
- Keeps: `load_transcript`, `append_entry`, `save_transcript` as compatibility projections over operator events.

- [ ] **Step 1: Write failing idempotent import test**

Add a legacy v1 file with user, assistant, tool, and evidence blocks. Call import twice and assert first report imports four entries, second imports zero, exactly one `legacy_session_imported` event exists, and the source JSON bytes remain unchanged.

Use deterministic IDs:

```rust
fn legacy_event_id(session_id: &str, entry_id: u64, role: &str) -> String {
    let namespace = uuid::Uuid::NAMESPACE_URL;
    uuid::Uuid::new_v5(&namespace, format!("heiwa:legacy:{session_id}:{entry_id}:{role}").as_bytes()).to_string()
}
```

- [ ] **Step 2: Write failing index rebuild test with fake embedder**

Define an injected boundary:

```rust
pub trait EmbeddingSink: Send + Sync {
    fn upsert_text(&self, thread_id: &str, event_id: &str, text: &str) -> anyhow::Result<()>;
}
```

Test that rebuilding indexes from one user message, one assistant completion, and one restricted tool event inserts all safe text into FTS, sends only eligible user/assistant text to the fake embedding sink, and produces identical counts on a second rebuild.

- [ ] **Step 3: Run tests to verify RED**

Run: `cargo test -p heiwa-session --test operator_service legacy -- --nocapture`

Expected: FAIL because import/rebuild APIs do not exist.

- [ ] **Step 4: Implement deterministic import**

Fingerprint each legacy file with SHA-256. Before import, scan materialized events for `legacy_session_imported.payload.source_fingerprint`. Map `TranscriptBlock::User` to `user_message`, `Assistant` to `assistant_completed`, `Tool` to `tool_call_completed`, and `Evidence` to `receipt_linked`. Create one synthetic turn per adjacent user-led block group; preserve original timestamps, including `0` for unknown legacy timestamps.

Append the import marker only after every entry append succeeds. Deterministic event IDs make crash/retry duplicates harmless in the reader.

- [ ] **Step 5: Convert compatibility transcript functions**

`load_transcript(session_id)` imports legacy data if required, materializes operator events, and converts message/tool/receipt events back into `TranscriptEntry`. `append_entry` maps a block into a typed event through `OperatorSessionService`; `save_transcript` appends only new blocks and rejects truncation with `legacy transcript truncation is unavailable after operator-stream cutover`.

Stop all production writes to `<sessions_dir>/<session_id>.json` after successful import. Keep file reads solely for import/recovery.

- [ ] **Step 6: Implement FTS/Lance rebuild**

Move existing FTS schema/sync functions into `operator_index.rs`. Key message rows by `(thread_id, event_id)`, not legacy numeric entry ID. `ProductionEmbeddingSink` maps event IDs to stable u64 keys by the first eight bytes of SHA-256 and calls `heiwa_embed::embed_and_store`.

Rebuild must transactionally clear/reinsert SQLite rows. Lance/embedding failures increment `embedding_failures` and return a degraded `IndexReport`; they do not alter journal truth.

- [ ] **Step 7: Run session tests and commit**

Run: `cargo test -p heiwa-session --all-features`

Expected: all transcript compatibility, migration, search, and operator tests PASS.

```bash
git add crates/heiwa_session Cargo.lock
git commit -m "Migrate session projections to operator events"
```

---

### Task 4: Per-Call DREX Value Routing

**Files:**

- Create: `apps/heiwa_core/src/drex/call.rs`
- Create: `apps/heiwa_core/tests/drex_call_routing.rs`
- Modify: `apps/heiwa_core/src/drex/mod.rs`
- Modify: `apps/heiwa_core/src/drex/router.rs`

**Interfaces:**

- Produces: `ModelCallRequest`, `ModelCallCandidate`, `ModelCallPlan`, `CandidateRejection`, `CostTruth`.
- Produces: `plan_model_call(&ModelCallRequest, &[ModelCallCandidate], &DrexPolicy) -> Result<ModelCallPlan>`.

- [ ] **Step 1: Write failing quality-before-cost tests**

Create tests with a quality-class-1 free model and quality-class-3 subscription model. For `minimum_quality_class: 3`, assert the class-3 model wins. Add a second class-3 direct API model at `$0.08`; assert a class-3 `target_only` subscription route with marginal cost `0.0` wins. Add sovereign, disconnected, quota-exhausted, context, capability, and explicit-override rejection tests.

Core request surface:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub intent: String,
    pub stage: String,
    pub raw_text: String,
    pub privacy: String,
    pub required_capabilities: Vec<String>,
    pub required_context_tokens: u32,
    pub minimum_quality_class: u8,
    pub minimum_success_rate: f64,
    pub maximum_marginal_cost_usd: Option<f64>,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub allowed_models: Vec<String>,
    pub excluded_models: Vec<String>,
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p heiwa-core --test drex_call_routing`

Expected: FAIL because call-routing types do not exist.

- [ ] **Step 3: Implement candidate and cost-truth types**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostTruth { LocalZeroCost, TargetOnly, ProxyEstimate, ExactProviderReport, CannotConfirm }

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallCandidate {
    pub tier: heiwa_protocol::ModelTier,
    pub connected: bool,
    pub adapter_capable: bool,
    pub quota_available: bool,
    pub marginal_cost_usd: Option<f64>,
    pub cost_truth: CostTruth,
}
```

`ModelCallPlan` contains selected candidate, admitted candidate IDs, structured rejections, policy version, and selection reason.

- [ ] **Step 4: Implement hard admission then lexicographic selection**

Filter disconnected, adapter-ineligible, quota-exhausted, excluded, over-budget, insufficient context/capability/quality/success, sovereign-remote, and invalid explicit routes. Compare admitted candidates with:

```rust
fn compare_candidates(
    left: &ModelCallCandidate,
    right: &ModelCallCandidate,
) -> std::cmp::Ordering
```

Do not add `ordered-float`; implement comparison with `f64::total_cmp`. Primary comparison is known marginal cost (`None` sorts after known values), then higher capability class, lower p95 latency, then higher success rate. Local-zero and target-only subscription routes may carry marginal `0.0`, but retain distinct truth classes in evidence.

- [ ] **Step 5: Preserve existing DREX vector/gate metadata**

Call existing `evaluate_drex` to produce authority/scorecard data. `plan_model_call` owns model selection; existing `plan_route` becomes a compatibility wrapper that builds candidates and delegates rather than mixing capability score and cost into one opaque `model_score`.

- [ ] **Step 6: Run core tests and commit**

Run: `cargo test -p heiwa-core --test drex_call_routing && cargo test -p heiwa-core drex`

Expected: PASS.

```bash
git add apps/heiwa_core
git commit -m "Route every model call by quality and marginal value"
```

---

### Task 5: One Routed Model-Call Executor Across Shell And Loops

**Files:**

- Create: `apps/heiwa_shell/src/model_calls.rs`
- Create: `scripts/check_model_call_boundary.sh`
- Modify: `apps/heiwa_shell/src/lib.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_loop/src/lib.rs`
- Modify: `crates/heiwa_loop/tests/loop_execution.rs`
- Modify: `scripts/check_agent_baseline.sh`

**Interfaces:**

- Consumes: `plan_model_call` and `OperatorSessionService`.
- Produces: `ModelCallExecutor::execute(ModelCallExecution) -> Result<ModelCallResult>`.
- Produces: `LoopModelCaller` trait consumed by `LoopController`.

- [ ] **Step 1: Write failing executor/fallback tests**

Use fake adapters where primary emits `StreamEvent::Error("rate_limited")` and secondary emits tokens plus usage. Assert events occur in order:

```text
route_planned(primary)
route_attempted(primary)
route_failed(primary, rate_limited)
route_planned(secondary)
route_attempted(secondary)
route_completed(secondary)
```

Assert remaining budget is reduced only by reported/estimated completed attempts and retry count stops at three.

- [ ] **Step 2: Define execution boundary**

```rust
pub struct ModelCallExecution {
    pub request: ModelCallRequest,
    pub candidates: Vec<ModelCallCandidate>,
    pub messages: Vec<heiwa_provider::adapter::Message>,
    pub remaining_budget_usd: Option<f64>,
    pub max_attempts: usize,
    pub cancel: tokio::sync::watch::Receiver<bool>,
}

pub struct ModelCallResult {
    pub provider: String,
    pub model_id: String,
    pub text: String,
    pub usage: heiwa_provider::adapter::TokenUsage,
    pub attempts: usize,
}
```

`ModelCallExecutor` receives an adapter resolver closure and `Arc<OperatorSessionService>` in its constructor.

- [ ] **Step 3: Implement event-backed execution and fallback**

Before adapter invocation, append `route_planned` and `route_attempted`. On normalized availability, auth, quota, timeout, or provider failure, append `route_failed`, add selected model to `excluded_models`, recompute remaining budget, and replan. On success append `route_completed` with honest cost truth, usage, and latency. If any append fails, abort before another adapter or side effect.

Use `tokio::select!` between stream receive and `cancel.changed()`. On cancellation abort the adapter task and return `ModelCallError::Cancelled`.

- [ ] **Step 4: Refactor bounded loops to request calls**

Replace the current second `LoopController::run` parameter, `adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync>`, with `caller: Arc<dyn LoopModelCaller>`:

```rust
#[async_trait::async_trait]
pub trait LoopModelCaller: Send + Sync {
    async fn call(&self, request: LoopCallRequest) -> anyhow::Result<LoopCallResult>;
}
```

Each loop iteration builds a fresh `call_id`, stage `loop_iteration`, remaining loop budget, and prior failed models. Shell supplies an implementation backed by `ModelCallExecutor`.

- [ ] **Step 5: Replace direct shell provider sends**

Replace every `adapter.send` in `apps/heiwa_shell/src/main.rs` with `ModelCallExecutor`. Keep `adapter.send` only inside `apps/heiwa_shell/src/model_calls.rs` and provider adapter implementations.

- [ ] **Step 6: Add boundary enforcement script**

`scripts/check_model_call_boundary.sh` runs `rg -n '\.send\(&.*messages|adapter.*\.send\(' apps/heiwa_shell crates/heiwa_loop --glob '*.rs'` and fails if any match is outside `apps/heiwa_shell/src/model_calls.rs`. Add it to `check_agent_baseline.sh`.

- [ ] **Step 7: Run focused tests and commit**

Run: `cargo test -p heiwa-shell model_call && cargo test -p heiwa-loop && bash scripts/check_model_call_boundary.sh`

Expected: PASS; boundary script reports `model_call_boundary=ok`.

```bash
git add apps/heiwa_shell crates/heiwa_loop scripts Cargo.lock
git commit -m "Centralize routed model call execution"
```

---

### Task 6: Durable Operator Turn Runner

**Files:**

- Create: `apps/heiwa_shell/src/operator.rs`
- Modify: `apps/heiwa_shell/src/lib.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/src/agentic.rs`

**Interfaces:**

- Consumes: session service and model-call executor.
- Produces: `OperatorTurnRunner`, `ActiveTurnRegistry`, `OperatorStreamFrame`.
- Keeps: `execute_repl_turn` and `execute_repl_turn_streaming` as compatibility wrappers.

- [ ] **Step 1: Write failing durable-turn tests**

Test deterministic and model-backed turns. Assert `turn_started` and `user_message` are fsynced before the fake executor is entered. Assert completed path appends `assistant_started`, route events, `assistant_completed`, receipt link, and `turn_completed`. Assert execution is never invoked when journal append returns an error.

- [ ] **Step 2: Implement active-turn cancellation registry**

```rust
#[derive(Default, Clone)]
pub struct ActiveTurnRegistry {
    turns: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
}

impl ActiveTurnRegistry {
    pub fn register(&self, turn_id: String) -> tokio::sync::watch::Receiver<bool>;
    pub fn signal_cancel(&self, turn_id: &str) -> bool;
    pub fn remove(&self, turn_id: &str);
}
```

- [ ] **Step 3: Implement turn runner**

`submit` calls `start_turn`; duplicate submissions return without a second spawn. New turns register cancellation and spawn `run_turn`. `run_turn` uses one `ModelCallRequest` per inference stage, passes route policy down, emits transient `assistant_delta` frames through a Tokio broadcast channel, and appends final/terminal events through session service.

Expose `OperatorTurnRunner::request_cancel(turn_id)`. It must append `turn_cancel_requested` before calling `ActiveTurnRegistry::signal_cancel`; if the append fails, return the error and leave execution running. The running turn observes the signal, stops further model/tool work, and appends `turn_interrupted` with reason `OPERATOR_CANCELLED`.

Tool calls append `tool_call_started` before execution and `tool_call_completed` after the existing approval/evidence path returns. Output above 16 KiB is written as an artifact and only a bounded preview enters event payload.

- [ ] **Step 4: Make compatibility wrappers thin**

`execute_repl_turn_streaming` subscribes to the new runner and maps `route_planned` to `ReplStreamEvent::Route`, transient deltas to `Token`, and terminal events to `Done/Error`. `execute_repl_turn` continues collecting this stream. Remove direct transcript JSON writes and legacy `EvidenceClient::journal` calls for operator events.

- [ ] **Step 5: Run shell unit tests and commit**

Run: `cargo test -p heiwa-shell operator --lib`

Expected: PASS.

```bash
git add apps/heiwa_shell/src
git commit -m "Run shell turns through durable operator events"
```

---

### Task 7: Authenticated Operator HTTP API And Compatibility

**Files:**

- Create: `apps/heiwa_shell/tests/operator_api.rs`
- Modify: `apps/heiwa_shell/src/cmd/app.rs`
- Modify: `apps/heiwa_shell/tests/app_api.rs`

**Interfaces:**

- Consumes: `OperatorTurnRunner`, `OperatorSessionService`, `heiwa_core::auth::extract_auth_subject`.
- Produces the HTTP routes from the design spec.

- [ ] **Step 1: Write failing auth and contract tests**

Start a temporary app runtime with `HEIWA_MACHINE_AUTH_TOKEN=test-machine-token` and temp evidence/session roots. Assert unauthenticated list/submit/cancel return `401`; bearer-authenticated requests return `200/202`; duplicate `client_request_id` returns the same `turn_id`; malformed cursor returns `400` with `error.code == "invalid_cursor"`.

- [ ] **Step 2: Add request auth helper**

```rust
fn operator_auth_subject(request: &str) -> Result<heiwa_core::auth::AuthSubject, u16> {
    let config = heiwa_core::config::RuntimeConfig::from_env();
    let cookie = header_value(request, "cookie");
    let authorization = header_value(request, "authorization");
    heiwa_core::auth::extract_auth_subject(cookie.as_deref(), authorization.as_deref(), &config)
        .map_err(|_| 401)
}
```

Call this before reading operator data, submitting work, cancelling, or upgrading operator WebSockets. Return `500 auth_not_configured` when both machine token and signing secret are empty.

- [ ] **Step 3: Add dynamic operator routes**

Implement exact endpoints:

```text
GET  /api/v1/operator/threads
POST /api/v1/operator/threads
GET  /api/v1/operator/threads/{thread_id}
GET  /api/v1/operator/threads/{thread_id}/events
POST /api/v1/operator/threads/{thread_id}/turns
POST /api/v1/operator/turns/{turn_id}/cancel
```

Parse segments with a helper that rejects empty IDs, `..`, percent-decoded slashes, and IDs over 128 bytes. Turn submission validates `client_request_id`, prompt length, route policy enum, quality class, and nonnegative budgets before returning `202`.

The cancel route calls `OperatorTurnRunner::request_cancel`; it never signals the registry directly, preserving the durable-intent-before-side-effect gate.

- [ ] **Step 4: Inject auth in `heiwa app api` CLI calls**

`call_local_app_api` loads `RuntimeConfig::from_env().machine_auth_token`, adds `Authorization: Bearer <token>` to the wire request, and returns `auth_not_configured` before network when absent. Dry-run JSON reports `auth: "machine_token_configured"` or `auth: "missing"`; it never prints token bytes.

- [ ] **Step 5: Keep compatibility endpoints backed by runner**

`/api/v1/repl` and `/api/v1/repl/stream` authenticate, submit to thread `default`, and preserve response/SSE shapes. Existing read-only nonoperator endpoints remain unchanged in this task.

- [ ] **Step 6: Run API tests and commit**

Run: `cargo test -p heiwa-shell --test operator_api --test app_api`

Expected: PASS.

```bash
git add apps/heiwa_shell/src/cmd/app.rs apps/heiwa_shell/tests
git commit -m "Expose authenticated operator HTTP API"
```

---

### Task 8: Authenticated Operator WebSocket And Native Tauri Bridge

**Files:**

- Create: `apps/heiwa_app/desktop/src-tauri/src/operator_stream.rs`
- Modify: `apps/heiwa_shell/src/cmd/app.rs`
- Modify: `apps/heiwa_app/desktop/src-tauri/Cargo.toml`
- Modify: `apps/heiwa_app/desktop/src-tauri/src/proxy.rs`
- Modify: `apps/heiwa_app/desktop/src-tauri/src/lib.rs`

**Interfaces:**

- Produces: `WS /ws/v1/operator?thread_id=&after=` frames.
- Produces Tauri command `operator_subscribe(thread_id, after, Channel<Value>)`.

- [ ] **Step 1: Write failing WebSocket auth/replay tests**

Test rejection before `101 Switching Protocols` without auth, authenticated replay from cursor, `caught_up`, heartbeat, and live delivery after a separate `OperatorSessionService` appends an event. Replace stream file and assert `invalid_cursor` frame then close.

- [ ] **Step 2: Implement authenticated operator event loop**

Pass full request target into `handle_websocket`. Authenticate before handshake. Every 200 ms call lock-free `events_after(thread_id, cursor, 100)`, emit each durable event with cursor, emit `caught_up` after initial replay, and heartbeat every 30 seconds. Broadcast transient deltas from `OperatorTurnRunner`; durable replay remains authority.

- [ ] **Step 3: Inject auth into native HTTP proxy**

Replace `reqwest::get` with a client request builder and:

```rust
fn machine_auth_token() -> Result<String, ProxyError> {
    let token = std::env::var("HEIWA_MACHINE_AUTH_TOKEN")
        .or_else(|_| std::env::var("HEIWA_AUTH_TOKEN"))
        .unwrap_or_default();
    if token.trim().is_empty() { return Err(ProxyError::AuthNotConfigured); }
    Ok(token)
}
```

Add `Authorization: Bearer` natively. Extend proxy tests to inspect the received header. Never return the token through Tauri IPC or error strings.

- [ ] **Step 4: Implement native WebSocket-to-channel bridge**

Add `futures-util = "0.3"` and `tokio-tungstenite = { version = "0.24", features = ["rustls-tls-native-roots"] }`, reusing the version already present in `Cargo.lock`. Build an authenticated client request with bearer header, connect, and forward decoded JSON through `tauri::ipc::Channel<serde_json::Value>`. Reconnect with the last durable cursor after a bounded 250 ms, 1 s, 3 s backoff.

Register command:

```rust
#[tauri::command]
pub async fn operator_subscribe(
    thread_id: String,
    after: Option<String>,
    on_event: tauri::ipc::Channel<serde_json::Value>,
) -> Result<(), crate::proxy::ApiErrorPayload>;
```

- [ ] **Step 5: Run shell/Desktop Rust tests and commit**

Run: `cargo test -p heiwa-shell operator_websocket && cargo test -p heiwa-desktop --all-targets`

Expected: PASS. Loopback stub tests may require running outside restricted sandbox.

```bash
git add apps/heiwa_shell/src/cmd/app.rs apps/heiwa_app/desktop/src-tauri Cargo.lock
git commit -m "Stream authenticated operator events to Desktop"
```

---

### Task 9: Desktop Operator Store And Renderer Cutover

**Files:**

- Create: `apps/heiwa_app/desktop/src/operator/types.ts`
- Create: `apps/heiwa_app/desktop/src/operator/store.ts`
- Create: `apps/heiwa_app/desktop/src/operator/client.ts`
- Create: `apps/heiwa_app/desktop/src/operator/store.test.ts`
- Modify: `apps/heiwa_app/desktop/src/runtime.ts`
- Modify: `apps/heiwa_app/desktop/src/main.ts`
- Modify: `apps/heiwa_app/desktop/package.json`
- Modify: `apps/heiwa_app/desktop/package-lock.json`

**Interfaces:**

- Consumes authenticated Tauri HTTP/stream commands.
- Produces `OperatorStore.reduce(frame)`, `OperatorClient.submitTurn`, `OperatorClient.start`.

- [ ] **Step 1: Add Vitest and failing reducer tests**

Run: `npm install --save-dev vitest@^3.2.0` from `apps/heiwa_app/desktop` and add `"test": "vitest run"`.

Test exact behavior:

```ts
import { describe, expect, it } from "vitest";
import { OperatorStore } from "./store";
import type { OperatorEventFrame } from "./types";

const frame = (id: string, eventType: string, payload: Record<string, unknown>): OperatorEventFrame => ({
  type: "event", cursor: `cursor-${id}`,
  event: { schema_version: 1, event_id: id, thread_id: "default", turn_id: "turn-1", event_type: eventType, occurred_at: "2026-07-18T00:00:00Z", payload },
});

describe("OperatorStore", () => {
  it("deduplicates replayed events", () => {
    const store = new OperatorStore();
    store.reduce(frame("e1", "user_message", { text: "hello" }));
    store.reduce(frame("e1", "user_message", { text: "hello" }));
    expect(store.snapshot().messages).toHaveLength(1);
  });

  it("keeps transient deltas disposable and final output durable", () => {
    const store = new OperatorStore();
    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "hel" });
    store.reduce(frame("e2", "assistant_completed", { text: "hello", provider: "ollama", model: "qwen" }));
    expect(store.snapshot().messages.at(-1)?.body).toBe("hello");
    expect(store.snapshot().transientByTurn["turn-1"]).toBeUndefined();
  });
});
```

- [ ] **Step 2: Implement wire types and pure reducer**

Types cover durable event, cursor, transient delta, caught-up, heartbeat, invalid-cursor, thread summary, route policy, and turn submission. Store state contains `seenEventIds`, `cursor`, `messages`, `turns`, `routesByCall`, `toolCalls`, `approvals`, `artifacts`, `receipts`, `blockers`, and `transientByTurn`.

Reducer never performs I/O and never stores auth material.

- [ ] **Step 3: Implement operator client**

`OperatorClient.start(threadId)` first replays HTTP history from no cursor, reduces it, then invokes native `operator_subscribe` from returned cursor. `invalid_cursor` clears only disposable projection and replays thread start. `submitTurn` generates `crypto.randomUUID()` as `client_request_id` and defaults route policy to `auto`.

- [ ] **Step 4: Cut renderer conversation over to store**

Remove renderer-local `messages` and `subagents` authority. `renderMessages`, worker/route inspector counts, composer submission, and WebSocket updates read from `operatorStore.snapshot()`. Remove direct browser `new WebSocket('/ws/v1/events')` for conversation/subagent events; keep the existing approvals/goals socket only until those events move into the operator stream.

Extract only operator-specific code from `main.ts`; do not redesign Calendar, dock, pane, or CSS surfaces.

- [ ] **Step 5: Run TypeScript tests/build and commit**

Run: `npm test && npm run typecheck && npm run build`

Expected: Vitest PASS, TypeScript PASS, Vite build PASS.

```bash
git add apps/heiwa_app/desktop
git commit -m "Render Desktop from durable operator stream"
```

---

### Task 10: Full Recovery, Routing, Documentation, And Runtime Verification

**Files:**

- Modify: `docs/architecture/app-foundation.md`
- Modify: `docs/local-self-operation.md`
- Modify: `docs/product-contract.md` only if implementation changes a stated current/target maturity claim
- Modify: `docs/superpowers/plans/2026-07-18-operator-stream-foundation.md` checkboxes while executing

**Interfaces:**

- Validates every acceptance gate from the approved design.

- [ ] **Step 1: Remove stale backend wording and document live contract**

Replace remaining STDB language in `docs/architecture/app-foundation.md` with local JSONL truth, Lance derived recall, and GitHub sync planned/redaction-gated. Add operator stream ownership, authenticated API/WS, Desktop native auth bridge, per-call DREX routing, and current route-cost truth classes.

- [ ] **Step 2: Run focused suites**

```bash
cargo test -p heiwa_evidence
cargo test -p heiwa-session --all-features
cargo test -p heiwa-core --test drex_call_routing
cargo test -p heiwa-loop
cargo test -p heiwa-shell --test operator_api --test app_api
cargo test -p heiwa-desktop --all-targets
npm --prefix apps/heiwa_app/desktop test
npm --prefix apps/heiwa_app/desktop run build
bash scripts/check_model_call_boundary.sh
```

Expected: every command PASS.

- [ ] **Step 3: Run workspace and repo gates**

```bash
cargo test --workspace --all-features
bash scripts/check_agent_baseline.sh
git diff --check
```

Expected: all tests/gates PASS and no whitespace errors.

- [ ] **Step 4: Start checkout runtime on `7475` with isolated state**

Use a temporary test root and explicit auth token:

```bash
HEIWA_EVIDENCE_DIR=/private/tmp/heiwa-operator-e2e/evidence \
HEIWA_MACHINE_AUTH_TOKEN=operator-e2e-token \
cargo run -q -p heiwa-shell --bin heiwa -- app start --port 7475 --no-open
```

Record exact PID/session and stop it before final reporting.

- [ ] **Step 5: Probe auth, replay, and idempotency**

Use bearer-authenticated `curl` requests to create/list thread `default`, submit one prompt twice with the same `client_request_id`, and assert one `turn_id`. Connect an authenticated WebSocket client from the Desktop/native test path, verify shell-created events arrive without refresh, reconnect from prior cursor, and verify no missing/duplicate events.

- [ ] **Step 6: Prove restart recovery and invalid cursor**

Start a controlled test turn with a blocking fake adapter, terminate only the checkout runtime, restart it on `7475`, and assert one `turn_interrupted` with `RUNTIME_RESTART`. Replace a copied temporary journal, submit its old cursor, and assert structured `invalid_cursor` followed by successful replay from thread start.

- [ ] **Step 7: Prove per-call fallback and index rebuild**

Run fixture candidates where primary is quota-exhausted or rate-limited and secondary is eligible. Verify route event order, quality floor, remaining budget, and honest cost truth. Delete only temporary fixture indexes, rebuild from temporary journal, and compare FTS/Lance result IDs before/after.

- [ ] **Step 8: Stop and clean temporary verification state**

SIGTERM the exact `7475` process, confirm port closes, then remove `/private/tmp/heiwa-operator-e2e`. Do not touch installed `7474` or durable `~/.heiwa` state.

- [ ] **Step 9: Post-feature review**

Inspect full diff for duplicated append/routing/auth mechanics, direct provider sends outside `model_calls.rs`, raw tokens in tests/output, frontend-owned authority, untested cursor branches, compatibility regressions, and doctrine drift.

- [ ] **Step 10: Commit verification/docs**

```bash
git add docs scripts/check_agent_baseline.sh
git commit -m "Document and verify operator stream foundation"
```

Final handoff reports commits, focused/workspace/baseline results, `7475` probe evidence, clean process cleanup, and honest remaining blockers. Do not promote, push, or merge without the separately required authority.
