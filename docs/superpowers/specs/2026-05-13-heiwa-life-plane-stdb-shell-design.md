# Heiwa Life Plane: STDB And Shell Design

Status: design contract for the first personal-life slice.

Repo authority:

- `HEIWA.md`: architecture truth.
- `docs/product-contract.md`: product boundary.
- `docs/architecture.md`: stack split.

## Decision

Use SpacetimeDB as canonical state for life memory, briefs, schedules, approvals,
and evidence.

Do not make SQLite the canonical life memory system. SQLite or JSON under
`~/.heiwa/state/` is allowed only as offline cache, local spool, or importer
scratch. Versioned product schema belongs in the STDB module and protocol
schemas, not ignored repo `memory/`.

Compression:

> Rust executes, SpacetimeDB adjudicates, TypeScript presents, Python imports,
> shell commands operate.

## Imported Signal

This design imports structure from Devon home sources without committing raw
private life data to the repo.

Source classes:

- `~/plans/ultimate_devon/*.md`: current state, schedules, finance/EI, cadence,
  background machine-plane, scorecards.
- Claude scheduled tasks: AM brief, weekly review, token rotation.
- Codex automations: daily life OS review and paused Heiwa ops monitors.
- Calendar summaries: work, home, and personal recurring blocks.
- Heiwa repo/runtime checks: doctor, provider status, Railway/GitHub drift,
  dirty worktree state.

Repo artifacts define schemas, commands, and adapters. Runtime imports private
rows on Devon's machine or through approved connected clients.

## Canonical Data Model

STDB owns durable rows.

Minimum tables:

| Table                      | Visibility  | Purpose                                                                |
| -------------------------- | ----------- | ---------------------------------------------------------------------- |
| `life_sources`             | private     | Source metadata, hashes, freshness, local raw refs                     |
| `life_memory_events`       | private     | Append-only structured events across body/work/social/money/mind/stack |
| `life_schedule_windows`    | private     | Calendar and shift windows with prep/travel/sleep flags                |
| `life_briefs`              | private     | AM/PM/weekly/money/stack briefs with cited freshness                   |
| `life_action_items`        | private     | Today queue, ROI rank, approval tier, source links                     |
| `life_readmodel_snapshots` | public-safe | Sanitized current state for `heiwa` and Heiwa.app                      |
| `life_automation_sources`  | private     | Claude/Codex/launchd/Heiwa scheduler jobs and status                   |

Existing STDB tables remain reused:

- `sources`, `beliefs`, and `pages` for knowledge-plane claims.
- `approval_requests` and `approval_decisions` for staged side effects.
- route/run/evidence tables for receipts.
- treasury/quota tables for model/provider budget.

## Event Shape

Canonical life event:

```json
{
  "schema_version": "heiwa_life_memory_event_v1",
  "event_id": "lifeevt_...",
  "occurred_at": "2026-05-13T16:25:00-07:00",
  "recorded_at": "2026-05-13T16:26:00-07:00",
  "domain": "body",
  "event_type": "hiit",
  "fields": {
    "pushups": 60,
    "squats": 60,
    "sprint_s": 60,
    "sets": 3
  },
  "source": {
    "kind": "user_entry",
    "uri": "heiwa://life/quick-log",
    "content_hash": "sha256:...",
    "captured_by": "heiwa-shell"
  },
  "confidence": 0.98,
  "approval_tier": "T0",
  "privacy_level": "private"
}
```

Fields are schema-validated JSON, not unbounded memory blobs. Fuzzy retrieval can
index events later, but SQL/domain queries must work first.

## Stack Responsibilities

### Rust

Rust owns runtime behavior:

- `heiwa life status`
- `heiwa life today`
- `heiwa life log`
- `heiwa life brief`
- `heiwa life approvals`
- `heiwa life freshness`
- importer orchestration and STDB reducer calls
- shell command authorization and receipts
- provider subprocesses and local model calls

Rust may read local cache/spool when offline, then sync to STDB when connected.

### Python

Python is compatibility/import glue, not the control plane:

- parse existing markdown source docs
- normalize Claude scheduled-task JSON
- normalize Codex automation TOML
- normalize calendar-export summaries when available
- emit protocol-schema records for Rust to validate and upsert

Python must not become the canonical scheduler or memory store.

### TypeScript

TypeScript owns visual clients:

- Heiwa.app life sidebar and now-context panels
- brief/action/approval cards
- source freshness warnings
- safe readmodel subscriptions

TypeScript must not hold raw provider secrets or bypass runtime policy.

### SpacetimeDB

STDB owns:

- canonical row mutation
- append-only event persistence
- reducers for validated ingest/update
- subscriptions to readmodels and approvals
- evidence linkages and freshness

Reducers should stay deterministic. External I/O remains Rust/Python/runtime.

### Shell Commands

Shell commands are first-class operator surface, not an afterthought.

Command lanes:

- `heiwa ...` verbs for product workflows.
- `!<command>` escaped shell in REPL/cockpit, only under an active tool lease.
- local scripts for importer checks and dry runs.

Shell side effects must keep the existing approval/risk model:

- read-only local checks are T0
- drafts/previews are T1/T2
- external sends/bookings/payments/publish/secret changes are T3
- destructive host actions require explicit approval

## P0 Command Contract

```bash
heiwa life status
heiwa life today
heiwa life freshness
heiwa life approvals
heiwa life brief --am
heiwa life brief --pm
heiwa life log body hiit --sets 3 --pushups 60 --squats 60 --sprint-s 60
heiwa life import home --dry-run
heiwa life import claude --dry-run
heiwa life import codex --dry-run
heiwa life sync --dry-run
```

Dry-run output must show:

- rows that would be inserted or updated
- source paths or connector ids
- freshness timestamps
- approval tier
- redaction status
- STDB connection mode: connected/offline-spool

## Heiwa.app Surface

Initial view:

- left nav: Chat, Calendar, Memory, Skills, Inbox, Files, Settings/Profile
- right now-context: identity/time/location, quick-log, today top 3, live signals
- center chat: user input, inline cards, approve/reject, shell receipts

The app reads from STDB/readmodel subscriptions through runtime-authenticated
client paths. It should not query random home files directly.

## Anti-Goals

- No raw personal life data committed to repo.
- No SQLite-as-canonical memory claim.
- No Claude dashboard as canonical truth.
- No Notes/iMessage claims unless access is verified.
- No hidden calendar/email/message side effects.
- No bank/trading automation through unofficial scraping.
- No provider parity claims before tests prove workflow depth.

## Acceptance

- STDB schema exists for the life plane.
- Protocol schemas validate importer payloads.
- `heiwa life ...` command contract exists and starts with dry-run/local mode.
- Python importer emits schema-valid records without secrets.
- TypeScript consumes sanitized readmodel records.
- Shell commands remain risk-gated and receipt-producing.
- Private source facts stay local unless explicitly synced through approved policy.
