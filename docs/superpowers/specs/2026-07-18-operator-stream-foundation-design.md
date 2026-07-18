# Operator Stream Foundation Design

Date: 2026-07-18
Status: Design approved; written spec awaiting operator review
Plane: Intake + Execution + Evidence

## Summary

Heiwa Desktop and the `heiwa` shell will share one durable, live operator
stream. Every user turn and meaningful execution event will be appended to the
local evidence journal before it is projected into the Desktop, REPL, TUI, or
search indexes. The renderer will stop owning conversation truth.

DREX will route every model call independently. A turn may use different
models for classification, planning, execution, review, or fallback. Each call
must select the highest-value eligible route at the lowest acceptable marginal
cost while respecting capability, privacy, quality, quota, latency, and
reliability constraints. Routing decisions and fallbacks will be visible as
typed operator events with honest cost metadata.

"Any model" means any discovered model that is currently authenticated,
healthy, policy-eligible, and backed by an adapter capable of executing the
requested call. Discovery alone must never be presented as execution support.

This is the first foundation slice. Terminal-daemon convergence follows it.
The first complete sub-app workflow follows the terminal daemon. Broader use
cases and UI/UX enrichment wait until those foundation gates pass.

## Problem

Current product surfaces have three related but divergent persistence paths:

- `heiwa_session` persists transcript JSON under `~/.heiwa/sessions/`, rebuilds
  an SQLite FTS index, and sends transcript text to the embedding backend.
- `heiwa_evidence` owns locked, fsynced, corruption-tolerant JSONL journal
  streams under `~/.heiwa/evidence/`.
- Heiwa Desktop keeps messages, subagent tasks, and several feature states in
  renderer memory.

The shell already persists more truth than the Desktop, but the session JSON
and evidence journal can disagree. A Desktop restart loses renderer state. A
separate shell process does not automatically appear in the Desktop. Existing
WebSocket behavior is a partial notification path rather than a replayable
product contract.

Routing also remains turn-oriented at several call sites. The current DREX
planner has useful inputs—live model tiers, quota admission, privacy, context
fit, success rate, capability class, and cost—but the operator contract does
not yet guarantee that every model call is independently planned, executed,
and evidenced.

## Goals

1. Make local JSONL the canonical operator conversation and execution record.
2. Give Desktop, REPL, TUI, and API clients one thread/event contract.
3. Provide replayable live synchronization across processes and restarts.
4. Persist full typed operator events without exposing hidden chain of thought.
5. Route every model call independently through DREX.
6. Optimize for the cheapest route that clears explicit quality, capability,
   privacy, quota, and reliability gates.
7. Preserve existing REPL and streaming endpoints during migration.
8. Keep SQLite FTS and Lance rebuildable rather than authoritative.
9. Fail closed before unrecorded work or external side effects begin.

## Non-Goals

- Redesigning the Desktop visual language.
- Adding a frontend framework or component library.
- Building the terminal daemon in this slice.
- Shipping Calendar, Mail, Finance, Social, or connector breadth.
- Syncing operator events to GitHub. Operator events remain local-only.
- Persisting hidden model reasoning or token-by-token output deltas.
- Making normal users manually select a model for every turn.
- Replacing provider-owned authentication, quota, or inference internals.

## Architecture

### Canonical stream

Canonical operator truth is:

```text
~/.heiwa/evidence/operator_events.jsonl
```

The stream uses the existing `heiwa_evidence` versioned envelope and
cross-process append lock. It is append-forever and must never pass through
keyed compaction.

The append order provides total ordering. The initial replay cursor contains a
versioned, server-owned encoding of the stream fingerprint and byte offset.
Byte offsets stay stable because the operator stream is never rewritten.
Clients must treat a cursor as an opaque string and must not derive business
meaning from it.

On an unknown cursor version, stream-fingerprint mismatch, or offset that does
not land on a valid event boundary, the server returns a structured
`invalid_cursor` error. The client then replays that thread from its start.
This lifecycle keeps the contract compatible with a later move to segmented
files or a different cursor encoding.

### Service ownership

`crates/heiwa_evidence/` owns:

- typed `OperatorEvent` persistence
- dumb, fsynced envelope append after domain validation
- replay after an opaque cursor
- skipped/corrupt-line accounting
- startup recovery inputs

`crates/heiwa_session/` owns:

- sole domain-writer authority for `operator_events.jsonl`
- operator-thread domain validation
- thread and turn materialization
- legacy transcript import
- idempotent turn submission
- SQLite FTS and Lance projection/rebuild
- compatibility conversion to and from `TranscriptBlock`

`apps/heiwa_shell/` owns:

- HTTP and WebSocket operator contracts
- REPL/TUI use of the session service
- model-call orchestration through DREX
- runtime tailing and client fan-out
- compatibility endpoints

`apps/heiwa_app/desktop/` owns:

- API/WebSocket client behavior
- disposable local projection/cache
- event reduction into conversation and inspector views
- reconnect, cursor resume, and duplicate suppression

The Desktop does not write `~/.heiwa` files, open SQLite/Lance directly, or
route models independently from the runtime.

Only `heiwa_session` may ask `heiwa_evidence` to append an operator event.
`heiwa_evidence` does not scan the stream or own an event-id index. Turn
idempotency uses `client_request_id` in the session service; event IDs provide
reader-side delivery deduplication.

### Derived indexes

SQLite FTS and Lance are rebuilt from `operator_events.jsonl`:

- SQLite FTS supports exact/operator search and bounded session lists.
- Lance supports semantic recall over eligible event text.
- Sensitive payload classes can be excluded from embedding while remaining in
  the local journal.
- Index failure degrades search, not durable append or execution history.

## Event Contract

### Shared fields

Every durable operator event contains:

```text
schema_version
event_id
thread_id
turn_id              optional for thread-level events
run_id               optional for execution events
call_id              optional for model/tool calls
event_type
occurred_at
actor
risk_class
sensitivity
parent_event_id      optional
correlation_id       optional
source_refs          list
evidence_refs        list
payload              tagged, event-specific value
```

Journal-envelope `v` and operator-event `schema_version` are distinct. Envelope
`v` versions JSONL framing (currently `1`); `schema_version` versions the typed
operator contract carried inside `record`.

`event_id` is the delivery idempotency key. `turn_id` groups one operator turn.
`call_id` identifies one model or tool call inside that turn. Source and
evidence references point to durable records; large output belongs in an
artifact rather than inline event JSON.

### Durable event families

- `thread_created`
- `turn_started`
- `user_message`
- `route_planned`
- `route_attempted`
- `route_completed`
- `route_failed`
- `assistant_started`
- `assistant_completed`
- `tool_call_started`
- `tool_call_completed`
- `approval_requested`
- `approval_decided`
- `artifact_created`
- `test_result`
- `receipt_linked`
- `blocker`
- `turn_completed`
- `turn_interrupted`
- `legacy_session_imported`

Text deltas are transient WebSocket frames. `assistant_completed` contains the
durable final response. If a process dies after `assistant_started`, startup
recovery appends `turn_interrupted` rather than inventing a completion.

Tool output above the inline limit becomes a referenced artifact. The event
stores tool identity, target, mode, risk, status, bounded preview, and artifact
or receipt references.

Every typed payload passes a sensitive-material gate before append, using the
same policy semantics as the existing receipts/capability `find_sensitive`
gate. Raw secrets, bearer tokens, credential-file contents, and provider auth
material are rejected rather than classified and persisted. The `sensitivity`
field controls handling of safe persisted material; it is not secret
protection.

## Per-Call Routing Contract

### Planning unit

The routing unit is a model call, not a whole thread and not necessarily a
whole turn. One turn may contain multiple calls with different requirements:

- deterministic or local intent classification
- local or low-cost drafting
- specialized coding/research execution
- higher-quality review
- repair or fallback after failure

Every call receives a `ModelCallRequest` containing:

- `thread_id`, `turn_id`, and `call_id`
- intent and execution stage
- required modalities and capabilities
- required context tokens
- privacy and locality requirements
- minimum quality class
- latency target
- maximum marginal cost or budget class
- allowed providers/models, when explicitly constrained
- excluded providers/models from prior failed attempts

Turn submission accepts an optional `TurnRoutePolicy` containing:

- mode: `auto` (default), `local_only`, `remote_only`, or `explicit`
- preferred or explicit provider/model
- allowed and excluded provider/model sets
- minimum quality class
- maximum marginal cost or turn budget
- privacy/locality requirement

Internal planners may narrow those constraints for one `call_id` through a
`CallRouteOverride`, but may not widen privacy, approval, or spending limits.
Existing shell model/provider pins map into this policy instead of bypassing
DREX.

### Candidate admission

DREX builds candidates from live provider/account and local-model discovery.
Before scoring, it removes candidates that fail any hard gate:

- provider disconnected, unhealthy, or in cooldown
- model lacks required capability or context
- privacy requires local/sovereign execution
- quota or call budget is exhausted
- explicit operator policy excludes the route
- adapter cannot execute the requested call class

An explicit model override is supported per turn or per call, but it is still
subject to privacy, safety, availability, and capability gates. Normal UI keeps
automatic routing as the default.

### Value selection

Selection is lexicographic rather than a vague cheapest-model rule:

1. Satisfy safety, privacy, capability, context, and approval requirements.
2. Clear the minimum quality and observed-reliability floor.
3. Among acceptable candidates, minimize marginal cost.
4. Break near-equal cost ties with expected quality, latency, locality, and
   observed success rate.

Local models use `local_zero_cost`. Subscription-backed CLI routes use
`target_only`, `proxy_estimate`, or `cannot_confirm` unless the provider
reports an exact marginal charge. Direct APIs may report exact provider usage
when the adapter returns it. Heiwa must never present subscription proxy prices
as confirmed spend.

### Fallback

Each failed attempt appends `route_failed` with a normalized failure class.
DREX replans the next call using the remaining eligible candidates and the
remaining turn budget. Fallback may change provider/model. Retry count and
budget are bounded. Privacy and approval gates cannot weaken during fallback.

### Evidence

`route_planned` records:

- admitted candidate identifiers
- rejected candidate identifiers with bounded reason codes
- selected provider/model/rate group
- required capabilities and quality floor
- selection reason and policy version
- cost truth class and estimate source
- quota/budget snapshot

`route_completed` records provider-reported usage, latency, outcome, and
receipt reference. Candidate details exposed to UI are a safe projection; raw
secrets and provider-owned internal prompts are never included.

## Turn Flow

1. Client submits a prompt with a client-generated idempotency key.
2. Session service resolves or creates the operator thread.
3. Journal fsyncs `turn_started` and `user_message`.
4. DREX builds the first `ModelCallRequest` and appends `route_planned`.
5. Runtime appends `route_attempted`, invokes the provider, and emits transient
   output deltas.
6. Runtime appends route, tool, approval, artifact, test, receipt, and blocker
   events as work progresses.
7. Final assistant output is appended as `assistant_completed`.
8. `turn_completed`, `turn_interrupted`, or `blocker` closes the turn.
9. Desktop and attached shells reduce the same durable events into their
   displays.

Execution cannot begin until the intake events are durable. Approval-gated or
external side effects cannot begin unless their pre-action event and approval
decision are durable.

## API and Live Synchronization

### HTTP

```text
GET  /api/v1/operator/threads
POST /api/v1/operator/threads
GET  /api/v1/operator/threads/{thread_id}
GET  /api/v1/operator/threads/{thread_id}/events?after={cursor}&limit={limit}
POST /api/v1/operator/threads/{thread_id}/turns
POST /api/v1/operator/turns/{turn_id}/cancel
```

Turn submission returns `202 Accepted` with `thread_id`, `turn_id`, initial
cursor, and stream URL. Reusing an idempotency key returns the existing turn
identity instead of appending a duplicate.

The turn request body contains `client_request_id`, `prompt`, and optional
`route_policy`. Omitting `route_policy` selects automatic per-call routing.

All operator HTTP endpoints require the existing local runtime authentication
implemented by `heiwa_core::auth`: either the machine bearer token or a valid
signed Heiwa session. Unauthenticated reads, turn submission, and cancellation
return `401`. The native Desktop bridge supplies credentials without embedding
the machine token in renderer assets.

### WebSocket

```text
WS /ws/v1/operator?thread_id={thread_id}&after={cursor}
```

Frames are:

- durable `event` with new cursor
- transient `assistant_delta`
- `caught_up`
- heartbeat
- structured error

The WebSocket authenticates with the same machine-token or signed-session
contract before replay or live delivery. Localhost reachability alone grants no
operator authority.

Disconnect does not cancel execution. On reconnect, the client requests events
after its last durable cursor. Duplicate delivery is safe because the reducer
deduplicates by `event_id`.

Separate shell processes append through the same session service and evidence
transport. The app runtime tails the journal and broadcasts new events. A
simple bounded polling tailer is preferred initially over a filesystem-watcher
dependency; cursor replay preserves correctness regardless of notification
latency. The polling reader stays lock-free: newline framing makes an
incomplete tail detectable, while the sidecar lock remains write-only so
pollers do not contend with appenders.

### Compatibility

- `POST /api/v1/repl` submits through the operator service and waits for the
  terminal turn event before returning its existing response shape.
- `POST /api/v1/repl/stream` projects operator events into the existing stream
  shape.
- Existing REPL/TUI commands use the same default thread and service.

## Recovery and Failure Semantics

- Intake append failure returns `503` and no model execution begins.
- Duplicate submissions return the existing turn.
- Client disconnect leaves execution running and replayable.
- Runtime restart appends `turn_interrupted` for every unclosed turn with
  reason `RUNTIME_RESTART`.
- Corrupt journal lines are skipped, counted, and exposed through runtime
  health and operator-stream diagnostics.
- FTS/Lance errors set search/index health to degraded but do not discard
  durable events.
- Evidence failure during execution closes the execution gate, performs
  bounded append retries, and prevents later side effects. The runtime reports
  an explicit evidence gap and never claims an unpersisted completion.
- Provider failure produces `route_failed`; bounded fallback replans from
  remaining eligible candidates.
- Cancel requests append intent before cancellation and end with a durable
  terminal event.

## Legacy Migration

On first access after upgrade, `heiwa_session` scans existing v0/v1 transcript
JSON files. For each session:

1. Derive deterministic event IDs from session ID, entry ID, and block role.
2. Import transcript blocks in existing entry order.
3. Append one `legacy_session_imported` marker with source fingerprint and
   imported entry count.
4. Rebuild SQLite FTS and Lance projections from imported events.
5. Preserve legacy JSON files unchanged as read-only recovery material.

The import marker and deterministic IDs make reruns idempotent. After successful
cutover, production code stops writing legacy transcript JSON. No migration
deletes operator data.

Tests use injected evidence, session, index, and embedding roots under temporary
directories. They must never read or mutate the operator's real `~/.heiwa`
corpus.

## Desktop Refactor Boundary

The current renderer is split only where the operator foundation needs clear
boundaries:

- operator event types and decoding
- operator API/WebSocket client
- pure event reducer/store
- conversation and execution-event views
- boot/reconnect controller

The pure reducer receives events and produces display state. It owns no I/O.
The API client owns no presentation. Existing Calendar, pane, dock, and
placeholder views remain visually stable unless contract changes require a
small correction.

No frontend router, state framework, or component library is added.

## Verification

### Unit and contract tests

- typed event serialization and schema compatibility
- concurrent append order and untorn lines
- replay-after-cursor boundaries
- corrupt-line handling and diagnostics
- duplicate event and turn idempotency
- thread/turn materialization
- legacy v0/v1 import idempotency
- FTS/Lance rebuild from journal
- incomplete-turn restart recovery
- DREX per-call hard-gate filtering
- cheapest acceptable candidate selection
- explicit override policy enforcement
- bounded fallback and remaining-budget handling
- honest cost truth classification
- HTTP and WebSocket contract behavior
- Desktop reducer replay and duplicate suppression

### End-to-end gates

1. Start checkout runtime on `7475`.
2. Submit a Desktop turn and observe it from an attached shell without refresh.
3. Submit a shell turn and observe it in Desktop without refresh.
4. Restart Desktop and recover the same thread exactly.
5. Kill checkout runtime mid-turn, restart it, and observe
   `turn_interrupted: RUNTIME_RESTART`.
6. Reconnect from a prior cursor with no missing or duplicated display events.
7. Submit the same idempotency key twice and observe one turn.
8. Force primary route failure and verify bounded replanning to an eligible
   lower-cost or higher-quality fallback within policy.
9. Rebuild FTS/Lance from the journal and reproduce search results.
10. Inject a corrupt line and verify valid history remains visible with
    degraded diagnostics.

### Repository gates

- Desktop TypeScript typecheck and build
- `cargo test -p heiwa_evidence`
- `cargo test -p heiwa-session`
- `cargo test -p heiwa-desktop --all-targets`
- focused `heiwa-shell` API/routing tests
- `cargo test --workspace --all-features`
- `bash scripts/check_agent_baseline.sh`
- alternate-port health and operator API probes
- clean diff review for duplicated mechanics, boundary breaks, missing tests,
  and evidence gaps

Installed port `7474` and the installed binary remain untouched until explicit
local promotion approval.

## Foundation Sequence

1. Implement and verify this operator stream and per-call routing foundation.
2. Converge `herdr`/Deno pane operations on the Heiwa session/terminal daemon,
   emitting operator events through this stream.
3. Implement one complete sub-app workflow across Intake, Execution, Evidence,
   approval, and receipt boundaries.
4. Expand use cases, feature depth, and UI/UX only after those gates are robust.

## Success Criteria

The foundation is complete when Desktop, REPL, TUI, and API clients display the
same durable thread and execution history; live updates survive disconnects and
restarts; incomplete work is closed honestly; indexes rebuild from text truth;
and every model call has a policy-constrained, evidence-backed route that
maximizes expected value among the lowest-cost acceptable candidates.
