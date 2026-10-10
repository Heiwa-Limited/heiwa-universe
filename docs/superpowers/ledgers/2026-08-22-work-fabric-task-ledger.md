# Work Fabric — Task Ledger

Contract: `docs/superpowers/specs/2026-08-22-heiwa-work-fabric-design.md`
Plan: `docs/superpowers/plans/2026-08-22-work-fabric-a1a-durable-work-core.md`
Started: 2026-08-22

Status is what is true at HEAD, not what is intended. A row moves to done only
when its verification runs.

## Release A1-a — Durable Work core

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | `work_id` on the operator event | done | `cargo test -p heiwa_evidence` |
| 2 | `work_created` / `work_linked` types and scope validation | done | `cargo test -p heiwa-session` |
| 3 | `heiwa_work` crate and the Work aggregate | done | `cargo test -p heiwa_work` |
| 4 | Work event builders and readers | done | `cargo test -p heiwa_work` |
| 5 | Projector fold, damage counted | done | `cargo test -p heiwa_work` |
| 6 | Migration: adopt before generate | done | `cargo test -p heiwa_work` |
| 7 | Snapshot and epoch-guarded deltas | done | `cargo test -p heiwa_work` |
| 8 | Integration through the real journal | done | `cargo test -p heiwa_work --test work_core` |
| 9 | `heiwa work` command | done | `cargo test -p heiwa-shell --bin heiwa cmd::work` |
| 10 | CI grouping and ledger | done | `bash scripts/ci_rust_test_group.sh --check` |

## Release A1-b — Workspace Coordinator

Plan: `docs/superpowers/plans/2026-08-24-work-fabric-a1b-workspace-coordinator.md`

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | Single git process boundary | done | `cargo test -p heiwa_workspace` |
| 2 | Repository snapshot | done | `cargo test -p heiwa_workspace` |
| 3 | Canonical roots and symlink refusal | done | `cargo test -p heiwa_workspace` |
| 4 | Isolated worktree lifecycle | done | `cargo test -p heiwa_workspace` |
| 5 | Writer lease on the evidence stream | done | `cargo test -p heiwa_workspace` |
| 6 | Refusal boundary for uncommitted work | done | `cargo test -p heiwa_workspace` |
| 7 | Bounded diff projection | done | `cargo test -p heiwa_workspace` |
| 8 | Test projection | done | `cargo test -p heiwa_workspace` |
| 9 | Workspace operator events | done | `cargo test -p heiwa_workspace -p heiwa-session` |
| 10 | Integration through journal and repository | done | `cargo test -p heiwa_workspace --test workspace_core` |
| 11 | `heiwa workspace` command | done | `cargo test -p heiwa-shell --bin heiwa cmd::workspace` |
| 12 | CI grouping and ledger | done | `bash scripts/ci_rust_test_group.sh --check` |

## A1 cohesion repair — 2026-08-24

Independent post-feature review found invariants that the component tests did
not compose across journal, Work, and Workspace boundaries.

| # | Repaired invariant | Status | Verification |
|---|---|---|---|
| 1 | One capability has one atomic lease winner across transports | done | `cargo test -p heiwa_evidence --test state` |
| 2 | Corrupt lease state fails closed and expired leases close before succession | done | `cargo test -p heiwa_evidence --test state` |
| 3 | Work, workspace leases, and workspace events share the resolved evidence root | done | `cargo test -p heiwa-shell --bin heiwa cmd::work` |
| 4 | Failed workspace preparation removes its clean worktree and revokes its lease | done | `cargo test -p heiwa-shell --bin heiwa cmd::workspace` |
| 5 | Workspace preparation requires durable Work and appends `workspace_prepared` | done | `cargo test -p heiwa-shell --bin heiwa cmd::workspace` |
| 6 | Work folding preserves global operator-cursor order across threads | done | `cargo test -p heiwa-shell --bin heiwa cmd::work` |
| 7 | Work IDs are safe path/ref components; client deltas cannot cross Work or regress revision | done | `cargo test -p heiwa_work -p heiwa_workspace` |

## Release A1-c — Work-bound execution and tri-surface delivery

Plan: `docs/superpowers/plans/2026-08-25-work-fabric-a1c1-work-bound-turns.md`

Release A1-c is **complete as scoped**, verified by
`scripts/check_work_fabric_a1_acceptance.sh`. A1-c1 closed the durable identity
gap between Work and the existing operator/Action Gate runtime. A1-c2
(`docs/superpowers/plans/2026-08-26-work-fabric-a1c2-worker-and-pane-identity.md`)
adds a provider-owned worker running inside the prepared worktree and a durable
pane bound to it. A1-c3 makes the surfaces agree and records restart truth.
Surface agreement is contract level — CLI and app API — not desktop UI.

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | Work-scoped turn admission requires durable Work/thread membership | done | `cargo test -p heiwa-session --test operator_service work_scoped` |
| 2 | Retry identity binds prompt, route policy, and Work | done | `cargo test -p heiwa-session --locked` |
| 3 | Route, approval, tool, artifact, receipt, cancellation, and terminal events preserve Work scope | done | `cargo test -p heiwa-shell operator_work_scoped` |
| 4 | Bounded redacted Work-session projector over global cursor order | done | `cargo test -p heiwa_work --test work_session` |
| 5 | `heiwa work show <work-id>` renders the canonical session projector | done | `cargo test -p heiwa-shell cmd::work` |
| 6 | Provider-owned worker runs inside the prepared Work workspace | done | `cargo test -p heiwa-shell --bin heiwa cmd::worker` |
| 7 | Durable terminal pane binds to Work and worker identity | done | `cargo test -p heiwa_worker --test runs` |
| 8 | Home, Work, and Agent surfaces agree on Work/revision/cursor | done | `cargo test -p heiwa_work --test surface_agreement` |
| 9 | Restart recovery exposes stale/closed worker and pane truth without repeating effects | done | `cargo test -p heiwa-shell --test work_fabric_a1` |
| 10 | Additive exact-HEAD `scripts/check_work_fabric_a1_acceptance.sh` | done | `bash scripts/check_work_fabric_a1_acceptance.sh` |

### A1-c2 review repair — 2026-08-28

| # | Repaired invariant | Status | Verification |
|---|---|---|---|
| 1 | Prepared worker ownership and per-process run identity are distinct | done | `cargo test -p heiwa-shell --bin heiwa cmd::worker` |
| 2 | Repeated runs survive both the run fold and canonical Work-session projection | done | `cargo test -p heiwa-shell --bin heiwa cmd::worker` |
| 3 | `heiwa work run --json` emits one JSON document while retaining both child streams in bounded pane evidence | done | `cargo test -p heiwa-shell --test work_run` |
| 4 | Reader-thread panics reach the caller instead of becoming clean worker results | done | `cargo test -p heiwa-shell --bin heiwa cmd::worker` |
| 5 | Relative provider commands execute the canonical binary recorded in the receipt | done | `cargo test -p heiwa-shell --test work_run relative_provider_runs_the_executable_whose_identity_was_recorded` |
| 6 | A failed initial heartbeat kills and reaps the provider child | done | `cargo test -p heiwa-shell --bin heiwa a_child_whose_heartbeat_cannot_be_persisted_is_not_left_running` |
| 7 | Sensitivity-screen rejection of a pane tail still permits worker exit evidence | done | `cargo test -p heiwa-shell --test work_run a_refused_pane_tail_still_records_that_the_worker_exited` |

### A1-c3 surface agreement and restart truth — 2026-09-13

Plane: Execution / Evidence. `heiwa_work::surface` derives Home, Work, and
Agent from one `WorkSessionSnapshotV1`, so they carry one Work, revision,
epoch, cursor, and bound, and each surface's `ClientProjection` refuses
cross-Work, cross-fold, stale, and gapped deltas. `heiwa work show --surface`
and `GET /api/v1/operator/work/{work_id}/surfaces` serve the same function.

Restart recovery appends one run-scoped `worker_stale` marker per unfinished
run, inside `OperatorSessionService::recover_interrupted_with`'s exclusive
section — at app runtime start and through `heiwa work recover`. The marker
records loss of supervision and the observed process: `alive` needs a pid
and matching platform start identity (now recorded with the heartbeat);
`gone` needs the pid absent or reused; everything else is `unknown`.
Recovery never stops, reattaches, or relaunches a process; a stale run has no
exit and `heiwa work show` says whether its process is still running. Ended
runs and earlier runs of the same worker keep their outcomes, and a live
owner's activity lease keeps its run from being marked.

Verification: `bash scripts/check_work_fabric_a1_acceptance.sh`. The binary
cases kill a real `heiwa work run` owner (surviving child recorded alive once,
no relaunch or file change), kill both (recorded gone), and restart
`heiwa app start` over an orphan (recorded before the port serves).

### A1-c3 review repair — 2026-09-13

Astra reproduced three boundary defects at `614960f1`, and a fourth (malformed
current-schema payload) at `14459ef2`, with disposable fixtures; each repair
below first failed against the revision it was found at.

| # | Repaired invariant | Status | Verification |
|---|---|---|---|
| 1 | An acceptance stamp names only the clean source (tracked and untracked) its checks observed; a revision committed or source added during checks fails the gate, and a dirty start never stamps | done | `bash scripts/tests/test_acceptance_stamp.sh` |
| 2 | Recovery interprets only rows the service's replay admits; unsupported-schema, rejected, unplaced, scope-mismatched, or unreadable worker evidence withholds the run and is reported, never marked | done | `cargo test -p heiwa-shell --bin heiwa cmd::recover` |
| 3 | Linux start identity carries the kernel boot identity; the app API surface epoch is minted per runtime instance, not per pid | done | `cargo test -p heiwa-shell --bin heiwa -- linux_start_identity work_surface_epoch` |
| 4 | A current-schema worker row whose payload does not parse as its typed worker payload withholds its run and is reported; no marker or process observation comes from it | done | `cargo test -p heiwa-shell --bin heiwa malformed` |

All four acceptance gates (L0, L1, L2, Work Fabric A1) now stamp through
`scripts/lib/acceptance_stamp.sh`; L0-L2 had the same end-only stamp check.
Recovery replays through `sync_materialized`'s paging and `apply_event`
admission, so damage and admission are counted exactly as materialization
counts them.

## Desktop Work surfaces — 2026-09-14

Plane: Intake / Evidence. The macOS desktop reads Work instead of only the
operator projection. `GET /api/v1/operator/work` (authenticated operator API)
returns a bounded catalog — 100 rows, newest first, with `total`, `truncated`,
and `skipped_events` — and an installation without Work answers an empty list.
One desktop service (`state/work.ts`) owns catalog and per-Work snapshot
fetches: each surfaces response is accepted only as exactly Home, Work, and
Agent with one identity for the requested Work; each Work has its own request
generation, so a late reply lands only on its own Work and never replaces a
newer one. Catalog rows show a snapshot's values when its durable Work revision
is at least the row's, so discovery and detail cannot disagree and an older
list cannot downgrade newer detail. Failures keep the last snapshot visibly
stale with Retry; an unknown Work is dropped; incompatible payloads never
render; an installation is called empty only with positive evidence.

Home lists unfinished Work first and reads up to three Home projections for
their attention facts. The Work view shows objective, linked conversation
status, run summary, blockers, approvals, actions, artifacts, tests, receipts,
and workspace, with bounds labelled and identifiers in Diagnostics. Workers
shows the selected Work's runs with each run's recorded state kept separate
from any supervision loss, which reads as a dated observation (alive, gone, or
unknown) — never stopped, recovered, or running now. The rail has a Work entry.

Verification: `npm test` in `apps/heiwa_app/desktop` (service races, epochs,
identity mismatch, retry, truncation, skipped and unreadable rows, repeated
runs, supervision states, reconciliation, bounded prefetch, focus retention,
and payloads captured from a real `heiwa app start` over disposable Work);
`cargo test -p heiwa-shell --bin heiwa work_catalog_route`. Keyboard and
narrow-window use were checked in an isolated browser fixture over those
captured payloads, not in the installed app.

## Execution and Evidence checkpoint — 2026-09-12

DREX distinguishes known rates from missing price evidence both within an
account and when comparing accounts. Unknown prices cannot satisfy a cost
ceiling. Saved models without price evidence load as unknown until discovery
or operator configuration establishes their rates; explicitly known free
models retain their zero-budget eligibility. Without a ceiling, an unknown
price is a last resort and is rendered as unknown in the rationale.

Verification: `cargo test -p heiwa_drex -p heiwa-provider --locked` passes.
Three added regressions first failed against the proposed price-truth fix:
cross-account price ordering, smallest-sufficient unknown fallback, and a
legacy cloud placeholder passing a zero-dollar ceiling. A fourth verifies
persistence and routing of explicitly known free models. Worker repairs above
also pass the workspace suite. These repairs do not complete A1-c3 or the
pending Work Fabric A1 acceptance gate.

## Provider stream repair — 2026-09-06

Plane: Execution / Evidence. The Codex subscription adapter now consumes the
current CLI JSONL protocol. Completed assistant items and usage reach the
consumer; success requires both a completion event and a successful process
exit. Failure, malformed output, and cancellation no longer masquerade as
successful empty output or leave an idle adapter child running.

Verification: `cargo test -p heiwa-provider --locked --test codex_cli` passes
15 behavior cases plus the isolated child-process driver. Coverage includes
current and legacy output, usage, provider failure, invalid JSONL/UTF-8,
nonzero/missing completion, stderr pressure, literal prompt arguments, spawn
failure, shutdown ordering, and consumer cancellation. The target is included
in `scripts/ci_rust_test_group.sh`'s runtime integration lane.

This repairs the provider transport contract; A1-c3's surface agreement and
restart-recovery rows remain pending. It does not establish native Codex
continuation, tool-effect receipts, API migration, or live model entitlement.

### Dependency audit follow-up

The full local gate at `744ac65b` passed Rust, web, Python/product, baseline,
and L0-L2 acceptance checks, but failed `verify_security` on three pre-existing
Python dependency findings. The lockfiles now select MkDocs Material `9.7.7`
and Banks `2.4.5`, the patched versions for
[CVE-2026-73295](https://github.com/squidfunk/mkdocs-material/releases/tag/9.7.7)
and [CVE-2026-71492](https://github.com/masci/banks/security/advisories/GHSA-x8wg-4xgc-vr54).
`uv run --locked --extra docs python -m mkdocs build --strict` passes;
`uv run --locked --all-extras --python 3.14 python -m pytest -q` from
`runtime/python` passes all 13 sidecar tests.

At `8323492d`, the repeated security gate passed every check except the runtime Python audit:
`heiwa-sidecar[llama] -> llama-index-core -> nltk 3.10.3` retains
`PYSEC-2026-3740` / `CVE-2026-81726`.
[Upstream lists no patched version](https://github.com/nltk/nltk/security/advisories/GHSA-8mgp-746c-j5xp)
as of 2026-09-06; PyPI's latest release is still `3.10.3`. No advisory ignore or
audit exclusion was added. That revision remained blocked pending a verified
replacement/removal of that optional dependency path or a patched upstream
release.

### Optional dependency removal — 2026-09-06

Plane: Execution / Evidence. Repository inspection found no sidecar operation
using LlamaIndex APIs; `check_deps` only reports whether its module is importable.
The unused `llama` installation extra is now retired. Regenerating the lockfile
removes 42 packages, including LlamaIndex, NLTK, and Banks, with no additions or
version changes among retained packages. Existing environments can reconcile
with `uv sync --extra dev` from `runtime/python`.

The `llama_index` diagnostic result key is preserved. Characterization tests
cover both an externally installed module and its absence; all 14 sidecar tests
pass after syncing the reduced all-extras environment. A real JSONL subprocess
probe also passes health, dependency checks, echo, and shutdown with LlamaIndex
absent. Sidecar descriptions now distinguish import diagnostics from execution
capability.

`bash scripts/verify_security.sh` passes with zero failures and zero warnings;
both Python dependency audits report no known vulnerabilities. No advisory
ignore or audit exclusion was added. This resolves the dependency blocker;
live provider entitlement, remote CI, and installed-runtime promotion remain
outside these local proofs. A1-c3 remains pending.

## Development and publishing refactor — 2026-09-07

Plane: Execution / Evidence. The delivery harness now records per-check logs
and atomic local verification receipts with source identity and worktree state.
Required L0-L2 gates fail when absent. A1 remains explicitly deferred. Native
desktop checks are explicit in the full local profile and remain required in
PR CI. Sidecar tests/lint share a locked local/CI entry point; the existing
required aggregate rejects failed, cancelled, or skipped Python checks.

Active workflows use GitHub-hosted runners, checked across all workflow files
and static matrices. Releases validate the resolved tag's declarations with
trusted main validators and build from the resolved commit. Containers reuse
the verified release-byte packaging path. Agent instructions now share one
authorization and evidence contract.

Focused verification covers source changes, missing gates, process cleanup,
interruption, concurrent receipts, aggregate failure propagation, retired
runners, and tag/main version divergence. Exact committed local and remote
results belong in their generated receipts; these changes do not complete A1.

## System 1 shadow judgment — 2026-09-23

Plane: Execution / Evidence. Design:
`docs/superpowers/specs/2026-09-23-system1-shadow-judgment-design.md`.
Work-scoped operator model turns now carry a shadow System 1 judgment: after
the terminal event is durable, the runner hands the exact DREX inputs to
`heiwa_shell::system1_shadow`. That module asks `turn-route-v1` (intent over
the DREX keys; capability class 1-5), replays `plan_model_call` with a
raise-only floor, and appends `heiwa.system1_shadow.v1` to the
`system1_shadow` stream. Execution never changes. The feature is off unless
`[system1] shadow = true`.

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | TypeScript experiment speaks TypeSafe's documented contract (Score maps keyed by level, Noul `true`/`false`, at most 10 levels); typecheck clean | done | `npm --prefix packages/heiwa_system1 test` (253) and `run typecheck` |
| 2 | Rust System 1 engine: question builders, documented-contract decoder (distributions must sum to 1), TypeSafe backend pinned to `jev-1.13.0`, capped selection-only fallback | done | `cargo test -p heiwa_judgment --locked` (34) |
| 3 | Runner offers finished Work-scoped model turns to a shadow observer; a panicking observer cannot strand a turn | done | `cargo test -p heiwa-shell --lib shadow` |
| 4 | Shadow records with raise-only counterfactual, privacy/sensitivity guards, `[system1]` config, `heiwa work shadow` report | done | `cargo test -p heiwa-shell --test system1_shadow`; `cargo test -p heiwa_config` |

Live check: a checkout runtime on 7475 ran with a disposable journal,
`local_only` turns, and a `gemma4` judge. Five Work-scoped turns went
through the authenticated operator API, and all five were judged. Judge
latency: p50 18.0s, p95 30.8s. Intent agreed with the keyword rules on
2 of 5. The judge would have raised the floor on 3 of 5 and changed the
model on none, because DREX already picks the most capable free local
model. Mutation checks: removing the panic containment, the raise-only
rule, or the privacy guard makes its test fail.

The first live run found a defect the stubs could not: the fallback
returned incoherent distributions (1.9 of probability mass). The decoder
now rejects them. The fallback asks only for a grammar-enforced selection,
and records note `answer_shape`.

Not established: any live Jev call, any quality label, or any evidence that
the judgment improves routing. The five-turn sample is a smoke test of the
loop. Promotion to an executing floor remains a separate decision.

Review round 1 — Astra's independent review found these gaps in the shadow
path, and each was reproduced with synthetic data before its repair.

| # | Repaired invariant | Status | Verification |
|---|---|---|---|
| 5 | Provider text never reaches the shadow journal: response bodies, echoed values, invalid keys, and the returned `model` (now provenance), with a final sensitive screen | done | `cargo test -p heiwa_judgment --test system1_backend --test system1_wire`; `cargo test -p heiwa-shell --test system1_shadow -- provider_text a_returned_model withheld` |
| 6 | A judgment request reaches only the classified endpoint: no redirects (typed `redirected`), no proxy for a local backend; TS adapters refuse redirects | done | `cargo test -p heiwa_judgment --test system1_backend --test system1_proxy`; `cargo test -p heiwa-shell --test system1_shadow redirected`; `npm --prefix packages/heiwa_system1 test` |
| 7 | Report cost keeps its truth per attempt across every call of a turn; totals and cost per completed turn exist only when nothing is unknown; completion is not called acceptance | done | `cargo test -p heiwa-shell --test system1_shadow -- cost_truth every_stage` |

Before the repair, the old report described a synthetic mixed Work as
$0.006 total and $0.0015 per result. That Work's accurate description is an
exact $0.004 plus an estimated $0.002, plus one unknown charge. A two-stage
tool turn charged $0.003 and then $0.004 reported $0.004 as its exact cost.
Mutation checks: following redirects, allowing proxies for a local backend,
removing the record screen, persisting the raw model, or reading only the
receipt each makes its regression fail.

Review round 2 — the first real local-model routing episode exposed the next
measurement boundary. The replayable Rust harness now runs a fixed,
read-only repository-analysis task through the production Work runner and
binds its machine label to the exact output digest. Its version-2 rubric
rejects extra keys and duplicate entries, and records workflow acceptance
separately from content correctness. Four synthetic tests pass, including
cases where the answer is correct but the turn is interrupted, cancelled,
timed out, or denied its read tool.

The live `gemma4:latest` smoke episode completed the runner but emitted
malformed follow-up tool-call JSON after one successful read. The harness
labelled that output `fail`/`wrong_shape` and kept workflow acceptance false;
this is evidence that the measurement catches a bad episode, not evidence
that the model should route Heiwa. The local run was zero-cost and took about
29 seconds. No Jev call, production quality label, routing promotion, or
installed-runtime change is established by this slice.

## Deferred with reason

- `work_node_bound` and `prior_history_digest` (WF-R15) need an enrolled mesh
  node. The attested-prefix design exists so binding adds a later event without
  changing an earlier one, so building the type now would produce something
  nothing can emit.
- Multi-repository coordination (`WorkTaskGraphV1`, scope reservation,
  barriers, publication sagas) is Release A2. A1-b's lease is per repository.
- Interactive pane operations — `send`, `split`, `focus`, `pause`, `resume`
  from the Terminal Runtime contract need a PTY adapter. A1-c2 delivers
  `create`, `read`, and `stop` over pipes; the rest wait for that adapter
  rather than being faked through a pipe that cannot carry them.
- Restart reattach, and containing a surviving orphan, stay deferred. A1-c3
  records lost supervision and the observed process; stopping an unsupervised
  provider is a separate action under its own authorization.
- `heiwa work recover` needs the exclusive activity lease, so it refuses while
  the app runtime or any worker writer is live. A worker whose owner dies while
  the app keeps running is recorded at the next app start. Per-run supervision
  proof would close that window.
- Worker launch stays ungated: `heiwa work run` spawns the raw command it is
  given. The Action Gate for raw terminal commands closes it.
- Automatic creation/routing of execution Work remains deferred. Explicit
  desktop continuation of an existing Work is implemented in C1-a1 below;
  ordinary conversation selection remains unscoped. A Work association is
  evidence continuity, not a tool grant or engine admission certificate.
- A worker's parent, and the separate tool/filesystem/network/budget/action
  leases the spec's "Legitimate Workers" lists, are not on `WorkerIdentity`.
  A1-c2 has exactly one writer lease and no child workers, so those fields
  would have nothing that could populate them.
- `SandboxMode::Worktree` remains unwired. A1-c2 runs the worker in the
  prepared worktree with a cleared environment, but `heiwa_protocol`'s sandbox
  modes still have no backend behind them; naming one would claim an enforced
  control that is only declared.
- Commit and push from a worktree need the Action Gate, which is A1-c.
- Upstream divergence needs a remote and a fetch, which is a network effect
  belonging with the GitHub Collaboration Service in Release B.

## Next experimental slice

- Release C1-a whole-provider containment and authority proof (see the C1 rows
  below); explicit desktop Work continuation is the C1-a1 checkpoint.
- Release A2 — multi-repository coordination remains tracked after the current
  macOS/Apple workflow slice.
- System 1 shadow evidence: run the TypeSafe backend with a key and
  shadow `standard`-privacy Work turns, where a floor above the best local
  class changes the route. Collect accept/reject labels and the floor that
  was actually needed, then calibrate before any promotion decision.

## Release C1 — Engines and Apple reminders

Design: `docs/superpowers/specs/2026-09-27-heiwa-engines-design.md`.
This records current local development evidence, not release acceptance.

| Slice | Scope | Status | Evidence / remaining work |
| --- | --- | --- | --- |
| C1-a | Authority, containment, grants, Work-scoped submission | doing | Offline fixture probe and 8 regression tests pass on macOS; whole-provider enforcement and runtime authority integration remain. See `docs/superpowers/plans/2026-09-27-engines-c1a-enforcement.md`. |
| C1-a1 | Explicit desktop continuation of existing Work | done | 212 desktop, 9 packaging, 45 shell library, 22 operator HTTP, and 58 session-service tests pass. L0 gate passes with updated seam hashes. Rust exposes and revalidates admitted membership; old runtimes fail before submission, lost acknowledgements retain request identity, and ordinary chat remains unscoped. No automatic Work creation, engine admission, installation, or release is claimed. |
| C1-a2 | File-bound operator decisions and verified decision consumers | doing | Core private-reader, canonical-integrity, CLI/DREX forgery, Mail staging and real HTTP authority regressions pass in development. Frozen gate, independent review and matching device proof remain. Contract: `docs/superpowers/plans/2026-09-28-engines-c1a2-decision-authority.md`. No same-user worker containment or legacy auto-migration is claimed. |
| C1-b | Shared tools and recoverable Apple effects | pending | Reminders R1 is CLI read/propose only: separate enrollment/permission, selected lists, imported Calendar-event proposal and honest incomplete/ambiguous scans. Source/hermetic progress is not device or write acceptance. Runtime authority/containment and persisted write-result proof remain required; no Reminders write acceptance claimed. |
| C1-c | Claude and Ollama execution families | pending | Offline Seatbelt canaries do not admit a provider engine. |
| C1-d | Lightweight answers, allowance pools, and surfaces | pending | Bounded read-only tools remain eligible for Answer; routing not implemented here. |
| C1-d1 | CLI contract: `heiwa.cli/v1` envelope and exit codes, `help --json` catalog, resumable `work watch` stream; `work`, `calendar plan`, `approvals` migrated | done | Combined desktop/CLI tree: `cargo test --locked -p heiwa-shell` passes 500 tests, including 19 CLI contract tests and real-shell argument preservation. Human and JSON watch resumption, cross-Work filtering, missing flag values, and rewritten journals are covered. Approvals decision internals remain unchanged. Other commands keep legacy output until migrated; C1 engine acceptance remains pending. |
| C1-e | Fresh-profile end-to-end acceptance | pending | `scripts/check_engines_c1_acceptance.sh` does not yet exist. |

Verification for the offline C1-a spike:
`python3 -m unittest discover -s scripts/tests -p test_engine_enforcement_probe.py -v`
and `python3 scripts/probe_engine_enforcement.py`.

## Cohesive desktop/runtime integration — 2026-10-10

This completes related development work rather than restarting a speculative
whole-codebase refactor. Acceptance is source/development acceptance; installed
and public release evidence remain distinct.

| Slice | Plane | Status | Evidence / boundary |
| --- | --- | --- | --- |
| Calendar choice persistence | Intake | doing | 36 targeted UI tests, 13 service tests and 12 connector tests pass. Independent save, stable row identity, empty deselection, stale catalog/client rejection and no-helper failure regressions are covered. Frozen gate and matching bundle proof remain; no new Apple permission/content acceptance is claimed. |
| Provider callers-first admission | Execution | doing | Private exact-account/model witness closes fallback and cross-account inventory borrowing. Production admission is rebuilt per execution before ranking. Structural gate and targeted regressions are being verified. This is LegacyCurrentTruth, not fresh inference eligibility. |
| Project MCP configuration | Evidence | doing | Duplicate process registration is rejected by instruction-sync checks; 33 sync tests pass. Unsupported Git npm registration and duplicated Docker gateway are removed from the operator edit only through the reviewed protected configuration. Runtime GUI reload remains distinct from CLI configuration loading. |
