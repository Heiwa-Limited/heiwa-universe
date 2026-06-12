# Heiwa ↔ OSS Lifts — Ground-up view

**Date:** 2026-06-12
**Branch:** `heiwa-desktop-skeleton`
**Replaces:** the earlier `oss-lift-integration-plan.md` (track-based, scrapped)

## The real foundation (already in the spine)

Heiwa is not a sequence of integrated libraries. It is a working personal-life runtime on the user's Mac. The math layer lives in `~/oss-repos/`. The runtime lives in `heiwa-universe/`. Reading both before planning was the missing step.

### What already exists in `heiwa-universe/`

| Surface | Path | What it does today |
|---|---|---|
| **DREX router** | `apps/heiwa_core/src/drex/{router,scorer,policy,vector}.rs` | Routes an `intent/risk/privacy/runtime/vram/context` vector to a `ModelTier` from the `model_tiers` SpacetimeDB table. **3 routing tests already pass** (`drex_provider_routing.rs`). |
| **ModelTier** | `packages/heiwa_bindings/rust/generated/...` (STDB) | Live evolving model/provider matrix. `capability_class`, `vram_requirement_mb`, `quantization_type`, `kv_cache_strategy`, `last_success_rate`, `latency_p_95_ms` — exactly the columns headroom's `ModelInfo` formalizes. |
| **HeiwaClawGateway** | `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/` | Real provider loop. Adapters: `claude`, `codex`, `gemini`, `acp`, `reflex`, `cli_adapter`, `base`. The "provider-owned loop" surface from the product vision. |
| **Proposal dispatch** | `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py` | `db.get_routable_proposals()` → `get_eligible_nodes(requires, privilege_tier)` → assign. **This is the wire we need to light up for user-facing intents.** |
| **ExecutionHookManager** | `packages/heiwa_sdk/heiwa_sdk/hooks.py` | Pre-execution gate, `HEIWA_ROLLOUT_MODE=observe\|enforce`. Fail-closed in enforce mode. |
| **Approvals** | `apps/heiwa_shell/src/cmd/approvals.rs` + `~/.heiwa/state/approvals/requests\|decisions/` | File-based gate. `list`, `show`, `decide`. Reads pending, writes decisions. |
| **Calendar** | `apps/heiwa_shell/src/cmd/calendar.rs` + `~/.heiwa/state/calendar/holds\|receipts/` | Local-only `focus/travel/soft` holds. Connector lane status readout. |
| **Goal** | `apps/heiwa_shell/src/cmd/goal.rs` | `start\|list\|show\|step\|complete\|abandon` — bounded loop workflow with `DEFAULT_JUDGE_MODEL=ollama/qwen3.5:9b`. |
| **Life** | `apps/heiwa_shell/src/cmd/life.rs` | Source-probe + freshness-rollup for the "Today" surface. |
| **Compress** | `apps/heiwa_shell/src/cmd/compress.rs` | Existing compression command — need to see what it does; may overlap with headroom lift. |
| **Mail** | `apps/heiwa_shell/src/cmd/mail.rs` | Mail scans + privacy-aware routing (latest commit). |
| **Connectors** | `apps/heiwa_shell/src/cmd/connectors.rs` | Connector lane status (Apple/Google/MSG/etc). |
| **Receipts** | `crates/heiwa_receipts/src/lib.rs` | SQLite tamper-evident chain (`prev_hash`/`entry_hash`). Schema v2. Currently a crate; not yet a `heiwa receipts` shell command? Need to verify. |
| **DREX tests** | `apps/heiwa_core/tests/drex_*.rs` (5 files) | Including `drex_scoring.rs`, `drex_persistence.rs`, `drex_provider_routing.rs`. |
| **Cockpit (TS)** | `apps/heiwa_app/clients/cockpit/src/` | In-progress UI per working tree. New `routes/Run.tsx` file present. |

### The missing loop

The user types into a TUI / shell: `heiwa schedule "remind me Friday 3pm to call mom"` — and Heiwa:

1. Parses the natural language into `{intent, recurrence, target_time, action}` *(no module does this today)*
2. Checks the calendar for a free slot at Friday 3pm *(`heiwa_calendar` only holds locally; no read model from Apple/Google yet)*
3. Stages an approval request *(the `approvals` command exists but the staging path is not invoked from a user-intent path)*
4. On approval, dispatches via DREX → HeiwaClawGateway → provider adapter *(works for proposals but the user-intent path doesn't reach `proposal_dispatch`)*
5. Records a receipt with a citation to the source action *(receipts work; citation layer is missing)*
6. Surfaces the action in the next "Today" brief *(life command reads but doesn't write pending actions)*

**The first commit lights up step 1 with a minimal viable shape, in the place the spine already expects it.**

## How the OSS repos become Heiwa code (no "track" framing)

Each repo is used the moment the loop needs it. The user sees this as one evolving commit log, not a project plan.

| OSS repo | When Heiwa needs it | The work |
|---|---|---|
| `cal.diy` slots/date-ranges/availability | step 2 (free-slot probe) | Port the algorithm (tests included) to Rust inside the existing `heiwa_shell` calendar command. Don't add a new package. |
| `dateparser` + `recurrent` (BSD-3, MIT) | step 1 (NL→time) | Vendor as Python deps in the SDK; write a thin `heiwa_sdk/intent/parse_time.py` that returns a `tz-aware datetime + RRULE`. |
| `headroom` (Apache-2.0) | step 5 (citation layer) | Lift `compress.py` algorithm into a new `heiwa_receipts::compress` module. Add `citation.rs` as a sibling, NOT a column on the existing chain. |
| `pipali` (Apache-2.0) | step 4 (scheduler) | When a recurring intent is approved, port pipali's `automation/index.ts` cron-with-jitter pattern to a new `crates/heiwa_automations` crate. **Wait for the calendar read model first** — automations need to know free/busy. |
| `Rapid-MLX` aliases + device-tiers | the evolving model matrix | Vendor the JSON into `heiwa_provider` data dir. The DREX router already takes `available_vram_mb`; the local-tier detection is the missing half. |
| `impeccable` 41-rule catalog | visual regressions | Wire `apps/heiwa_app/scripts/design-regression.mjs` into CI on the new `cockpit` routes. Run as a build artifact gate. |
| `mem0` + `letta` | long-term memory | Only after the loop produces real receipts — receipts are the memory substrate; mem0/letta shape the index on top. |
| `litellm` pricing | cost layer in DREX | Pattern-only — already have `cost_per_turn` on `ModelTier`. |
| `RouteLLM` | router framework | Reference only — DREX is the router. |
| `mlx-lm` | local inference substrate | Reference only — Rapid-MLX wraps it. |
| `inbox-zero` (AGPL, patterns only) | mail read model | Reference for the `heiwa mail` extension; never vendor. |
| `huginn`, `activepieces` (per-dir license) | automations | Reference for event-watcher patterns when the automations crate lands. |
| `browser-use` (MIT) | computer-use surface | Reference for the subagent dispatch view in the cockpit (`routes/Run.tsx`). |
| `ruflo` (MIT) | MCP registry pattern | The existing `crates/heiwa_mcp/src/{lib.rs,local_tools.rs,tools.rs}` is the registry; ruflo's connection-pool concept waits for remote MCP servers. |
| `LLMLingua` (MIT) | prompt compression | Reference; headroom's detector is good enough for v1. |
| `caldav` (Apache-2.0) | calendar connector | Vendor as a real Python dep when the read model is built. |

**No repo gets integrated speculatively.** A repo is touched the day a real commit needs it.

## First real commit: `heiwa schedule <text>` — NL→intent→approval

**Branch:** `heiwa-desktop-skeleton` (continue on current)

**Files added/touched:**

1. `packages/heiwa_sdk/heiwa_sdk/intent/parse_time.py` (NEW, ~150 LOC)
   - Wraps `dateparser` (BSD-3, already in `~/oss-repos/dateparser`) + a small RRULE builder
   - Returns `ParsedTime { dt: datetime, tz: str, rrule: Optional[str], confidence: float }`
   - Pure function, no LLM call
   - Tests in `parse_time.test.py`

2. `packages/heiwa_sdk/pyproject.toml` (MODIFY)
   - Add `dateparser` and `recurrent` as deps

3. `apps/heiwa_shell/src/cmd/schedule.rs` (NEW, ~250 LOC)
   - `heiwa schedule "remind me Friday 3pm to call mom"`
   - Calls `parse_time` → builds a `Goal` with title/description/recurrence
   - Writes a `~/.heiwa/state/dispatch/requests/<id>.json` in `operator_dispatch_request_v1` shape (the dirs in this plan were wrong; `approvals.rs` is truth: requests live under `state/dispatch/requests/`, decisions under `state/dispatch/approvals/decisions/`)
   - Writes a calendar `hold` of kind `focus` (or `soft` for a reminder)
   - Prints a `heiwa approvals list` summary

4. `apps/heiwa_shell/src/cmd/mod.rs` (MODIFY — add `pub mod schedule;`)

5. `apps/heiwa_shell/Cargo.toml` (MODIFY — add `chrono-tz` or use existing `chrono` for tz)

6. `apps/heiwa_shell/tests/schedule.rs` (NEW, ~50 LOC)
   - Pure-Rust test of the schedule command's argv parsing + payload shape, using a temp dir for `~/.heiwa/state/`

### Acceptance

- `cargo test -p heiwa_shell schedule` passes
- `cargo build -p heiwa_shell` passes
- Hand-run: `heiwa schedule "remind me Friday 3pm to call mom"` produces a pending approval and a calendar hold
- Hand-run: `heiwa approvals list` shows the new request; `heiwa approvals decide <id> --approve` is the existing path

### What this does NOT do (out of scope for first commit)

- Does not actually call a provider / DREX on approval — that wires up in the next commit, after we have at least one end-to-end approval→receipt flow without it (just a deterministic "logged the approval" path)
- Does not parse the *action* ("call mom") — only the time + recurrence. Action parsing comes after, when the calendar read model exists to scope what actions are valid
- Does not yet use DREX routing — the schedule command is deterministic, not LLM-routed. DREX gets involved when the user types a *free-form* request the NL parser can't handle (which is when `parse_time` returns `confidence < 0.7`)

## What unblocks the second commit

- The `heiwa receipts` shell command (or the receipts crate's CLI) — to confirm a receipt gets written on approval
- A `heiwa_calendar probe` subcommand — to read holds back via the existing state dir
- The first time we need to compress a receipt body — that's when `headroom`'s `compress.py` lands as `crates/heiwa_receipts::compress`

## What unblocks the third commit

- DREX called from the schedule path when `parse_time` confidence is low
- The first provider-backed action via HeiwaClawGateway (probably `acp` adapter as a smoke test)

## Verification (per AGENTS.md)

- After each commit: `cargo build` + `cargo test` on the touched crate(s)
- Hand-run the new command with `--json` and check the produced payload against the `approvals` command's expected shape
- `heiwa app update --source checkout` to promote per the local-first rule
- Smoke-test on `7475` per the agentic-runtime-workflow rule

## What we will explicitly NOT do

- ❌ Vendoring any AGPL code
- ❌ Adding a model picker to the UI
- ❌ Creating new top-level packages before their crate skeleton exists
- ❌ Touching the existing `heiwa_receipts` schema or hash chain
- ❌ Speculative OSS integration without a real commit that needs it
- ❌ A "tracks" or "phases" framing — work is one evolving log

## Open question for you

1. The first commit scope: am I right that the smallest real end-to-end loop is the **approval-staging** path (no provider call yet), not the **provider call** path?
2. Should the schedule command require `--dry-run` for the first few days, since it'll be the first new command that writes to `~/.heiwa/state/`?
3. `dateparser` (BSD-3) and `recurrent` (MIT) are in `~/oss-repos/` — should I add them as PyPI deps in `pyproject.toml`, or vendor the source?
4. The new `crates/heiwa_automations` (pipali port) — should it land *before* the first commit if you want scheduling-with-recurrence from day one, or after when the calendar read model exists?

## Commit 1 — EXECUTED 2026-06-12

Decisions on the open questions:
1. **Approval-staging path confirmed** as the smallest loop. No provider call; the only writes are the two staging primitives the spine already gates.
2. **No mandatory `--dry-run`.** The command only stages (draft hold + pending approval) — approval *is* the gate. `--dry-run` is supported as an optional flag, consistent with `approvals decide`.
3. **PyPI deps** (`dateparser` BSD-3, `recurrent` MIT) — added to root + SDK pyprojects, installed via uv. Vendoring adds maintenance for zero license benefit.
4. **`heiwa_automations` lands after the calendar read model.** Recurring intents already carry their RRULE in the approval payload + hold note; the crate is needed only when something must *fire*.

Amendments vs. the plan as written:
- Approvals dirs corrected (see above).
- Added `--at YYYY-MM-DDTHH:MM` explicit escape hatch — deterministic path with no Python; also what the hermetic Rust integration tests use.
- NL parsing subprocesses into `packages/heiwa_sdk/heiwa_sdk/intent/parse_time.py` (standalone-runnable; resolution: `HEIWA_PARSE_TIME`/`HEIWA_PYTHON` env → dev checkout → PATH python3).
- `recurrent` drops the time-of-day unless the text says "at" ("every monday 9am" vs "at 9am"); parse_time repairs the RRULE with a regex-extracted BYHOUR/BYMINUTE.

Evidence: 7 Python tests + 7 Rust unit tests + 3 integration tests pass; hand-run produced `req_…  schedule-intent -> calendar:hold-…  risk=stage` in `heiwa approvals list` and the hold in `heiwa calendar hold list`.

---

## Commit 2 — `heiwa approvals decide` closes the schedule loop (2026-06-12)

**Wire closed:** `heiwa schedule` stages a draft hold + a pending approval. `heiwa approvals decide --approve|deny` now reads the request, computes the effects it *would* have, and applies them. Approve flips the hold `draft → confirmed` (with `confirmed_at` + `confirmed_by_decision` provenance) and writes a `calendar_hold_status_changed` receipt. Deny drops the draft hold (only drafts — confirmed holds require explicit cancellation to preserve the audit trail) and writes a `calendar_hold_dropped` receipt.

**Files changed:**
- `apps/heiwa_shell/src/cmd/calendar.rs` — added `update_hold_status(hold_id, new_status, by_decision)` and `drop_draft_hold(hold_id, by_decision)`. Both mirror the `create_hold` write pattern: validate → mutate JSON → write to `~/.heiwa/state/calendar/holds/` → emit a `calendar/receipts/rcpt-…-status-….json` or `rcpt-…-dropped.json` receipt.
- `apps/heiwa_shell/src/cmd/approvals.rs` — `decide` now calls `compute_effects(id, approve)` (pure read) and `apply_effects(id, plan, approve)` (writes via the new `calendar` functions). The decision JSON gets two new additive fields: `effects` (the plan) and `applied_effects` (the per-effect results, with the new hold JSON inlined for `hold_confirm`). `--dry-run` previews without writing.
- `apps/heiwa_shell/tests/approvals_decide.rs` — 3 hermetic integration tests: dry-run preview, approve flow, deny flow. All use a temp `HOME` and `heiwa schedule --at` for hermetic staging.

**What this commit does NOT do:**
- ❌ Does not touch `crates/heiwa_receipts` (its schema is for cost-bearing model calls; approval decisions are not model calls).
- ❌ Does not call a provider / DREX / HeiwaClawGateway — pure state mutation in the local spine.
- ❌ Does not introduce a new package, schema migration, or a new top-level subcommand.
- ❌ Does not change the existing decision JSON shape consumed by `approvals list` / `approvals show` — `effects` and `applied_effects` are additive.

**Discovery during the commit:** the actual decisions path is `~/.heiwa/state/dispatch/approvals/decisions/` (one level deeper than the plan said, and one level deeper than the original commit-1 plan-doc claim). `requests_dir()` returns `dispatch/requests/` and `decisions_dir()` returns `dispatch/approvals/decisions/`. Both helpers in `approvals.rs` are the source of truth; the test path constants now reference them via comment so future readers find the right place.

**Evidence:** 3 new integration tests + 29 pre-existing shell tests all pass. Hand-run on the live `~/.heiwa/state/`: dry-run previewed the effect, then real approve flipped `hold-20260619-31cc2263` to `status: "confirmed"`, wrote the decision with both `effects` and `applied_effects`, and emitted the `calendar_hold_status_changed` receipt. The schedule→approvals→calendar loop is now closed end-to-end with the same evidence the cockpit POST lane produces.

## Next commit (predicted, not committed)

**Goal:** DREX escalation when `parse_time` confidence is low. Right now `heiwa schedule` errors out below the confidence floor (0.5) and tells the user to use `--at`. The next commit lowers that error to a *fallback* — if confidence is below threshold, the schedule command should ask DREX (via HeiwaClawGateway) to clarify or pick a default, and the user can confirm the slot before any state is written.

**Files this will touch:**
- `apps/heiwa_shell/src/cmd/schedule.rs` — change the `confidence < MIN_CONFIDENCE` branch to call into the HeiwaClawGateway clarify path
- `apps/heiwa_shell/src/main.rs` (or wherever the gateway is initialized) — ensure the gateway handle is reachable from `cmd::schedule`
- `apps/heiwa_core/src/drex/` — extend `plan_route` with a `Clarify` outcome that returns a slot suggestion, not an error

**Why not commit it now:** the user explicitly said commit 2 is "approval→receipt on `decide --approve`, which is where the `heiwa_receipts` chain joins the loop." That join is now in the calendar-receipt lane (the local spine), not the `heiwa_receipts` SQLite chain. The two are separate by design. DREX escalation is the next logical wire but it's a larger surface — defer to its own commit when we have a real use case (the user's first ambiguous schedule request) to drive the design.

## Open questions

- The `external_promotion: "approval_required"` field on confirmed holds is a future placeholder for the Google/Apple write-back lane (per the cockpit's `lanes` block in `calendar::summary_payload`). When that lane lands, the field becomes meaningful; right now it's documentation. **Should the next commit set `external_promotion` to a more specific value like `"approved_no_external_write"` on confirm, or keep it as `"approval_required"` until the write-back lane exists?**
- The drop-on-deny path is destructive (deletes the hold file). For confirmed holds we route through `update_hold_status(.., "cancelled", ..)` instead. **Should there be a confirmation step ("deny will drop the draft hold; continue?") or is the dry-run preview enough?**


## Commit 3 — EXECUTED 2026-06-12

`heiwa mail triage`: metadata-only summaries + suggested actions over the
priority read model. Draft-tier messages get a suggested reply (local Ollama
gemma4, deterministic template fallback — prompt carries sender+subject only,
never a body, never leaves the machine) staged as `mail-reply-draft` approvals
with deterministic per-message request ids (idempotent re-runs; denied
suggestions never re-stage). approve -> draft lands in
`~/.heiwa/state/mail/outbox/` as `ready_for_manual_send` + receipt; deny ->
dismissal receipt. Delete/archive is *suggested* for bulk senders but never
staged — no write bridge exists (Gmail scope is read-only, Apple bridge is
metadata-only) and pretending otherwise would be theater.

Real-machine blocker surfaced: no mail source is actually connected on this
Mac (Apple Mail unconfigured, Gmail connector `needs_auth`). Unblock with:
`heiwa connect gmail --client-secret <path>` then `--authorize` (user step).


## Commit 4 — EXECUTED 2026-06-12

Mail draft generation now asks the same DREX + quota router the REPL uses
(`route_task_with_quota`) instead of hardcoding a model. Only local routes are
accepted for mail (metadata is personal; sovereign stays local-first) — remote
winners are declined and the default local model is used. Successful
generations feed real token usage back into the quota ledger
(`record_local_quota_run` -> `~/.heiwa/state.db` quota_state + run_history),
so "auto" routing's budget view includes background mail work, not just REPL
turns. Verified in sandbox: draft_source `ollama:gemma4:latest (drex-routed)`,
ledger row `mail-triage-* | ollama | 100 in / 279 out | SUCCESS`.
