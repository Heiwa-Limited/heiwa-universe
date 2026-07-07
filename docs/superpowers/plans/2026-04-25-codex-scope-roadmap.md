# Codex Scope Roadmap — Repo Reshape

> **Master document.** Ties together 4 executable plans + 3 reports derived from Codex's 2026-04-25 audit of `heiwa-universe`.

**Status:** Drafted 2026-04-25
**Owner:** Devon
**Operator authority:** Class 3 peer execution; no per-step approval required.

---

## Why this exists

Codex audited the tracked repo on 2026-04-25 and found ~245k LOC tracked vs an estimated ~40k LOC of real product. Empirical recount on `main` confirms:

| Zone                                                                   | Tracked LOC | Class                 |
| ---------------------------------------------------------------------- | ----------- | --------------------- |
| `packages/heiwa_skills`                                                | 86,477      | legacy / reference    |
| `packages/heiwa_bindings`                                              | 51,046      | generated             |
| `apps/heiwa_hub`                                                       | 24,793      | legacy Python surface |
| `docs/superpowers` + `docs/design`                                     | ~28k        | reference / planning  |
| `crates/*`                                                             | 8,771       | **product**           |
| `apps/heiwa_shell` + `heiwa_core` + `heiwa_app` + `heiwa_orchestrator` | 17,462      | **product**           |
| `packages/heiwa_sdk` + `heiwa_protocol` + `heiwa_cli`                  | 14,103      | **product (mostly)**  |

Real product surface: ~40k LOC. Slop ratio: ~80%. Without a defined boundary and CI enforcement, contributor attention dilutes, builds slow, and "is X production?" gets answered by guess.

## What we are doing

Operationalize Codex's 5-step scope as concrete, sequenced work. Each plan produces a merged change with measurable repo-shape effect. Each report documents a baseline against which future drift is measured.

## Documents in this set

### Reports (descriptive — no code changes)

1. **[2026-04-25-current-state.md](../../audit/2026-04-25-current-state.md)**
   Snapshot of tracked LOC by zone, file class distribution, and the slop ratio. Baseline for every later measurement.

2. **[2026-04-25-slop-budget.md](../../audit/2026-04-25-slop-budget.md)**
   Proposed slop budget (LOC cap per non-product class), growth-rate cap, and CI threshold values.

3. **[2026-04-25-pi-mono-adoption.md](../../audit/2026-04-25-pi-mono-adoption.md)**
   Concrete extracted recommendations from pi-mono and claw-code, classed `adopt`, `skip`, `defer`. Supersedes the March 23 comparison's general framing with actionable line-items.

### Plans (executable — produce merged changes)

1. **[2026-04-25-product-surface-definition.md](2026-04-25-product-surface-definition.md)**
   Add `PRODUCT_SURFACE.md` + `scripts/audit_product_surface.sh` + first unit test. Locks the boundary. Foundation for all later gates.

2. **[2026-04-25-slop-quarantine.md](2026-04-25-slop-quarantine.md)**
   Classify every tracked path with one of: `product`, `generated`, `legacy`, `reference`, `archive`, `vendored`, `runtime-artifact`. Move legacy/archive surfaces under `legacy/` and `archive/` subtrees. No deletion in this plan.

3. **[2026-04-25-repo-hygiene-ci-gates.md](2026-04-25-repo-hygiene-ci-gates.md)**
   New CI job `audit-product-surface` plus expanded `gitignore` and pre-commit hooks. Fails on tracked logs, `__pycache__`, vendor leakage, stale package metadata, and slop budget breaches.

4. **[2026-04-25-oss-demo-path.md](2026-04-25-oss-demo-path.md)**
   The "fresh clone → `heiwa doctor` → provider discovery → routed call → evidence receipt" path becomes a runnable, CI-gated end-to-end test plus a documented quickstart.

## Dependency graph

```
           Plan 1 (Surface)
           /       |        \
          v        v         v
  Plan 2          Plan 3    Plan 4
(Quarantine)    (CI Gates) (OSS Demo)
                   ^
                   |
           needs Plan 2 done
```

- **Plan 1** must land first. Its taxonomy and audit script are inputs to everything else.
- **Plan 2** depends on Plan 1's classes; it is the bulk physical reshape.
- **Plan 3** depends on Plan 2 having completed at least the first quarantine pass — otherwise the gate fails immediately on legitimate prior state.
- **Plan 4** depends only on Plan 1 (needs to know what is in the product surface) and can run in parallel with Plans 2 and 3.
- **Pi-mono adoption (report 3)** is independent — a backlog of small extracted PRs, not a sequenced plan.

## Sequencing

Suggested order if executing serially:

1. Land Plan 1 (1–2 sessions). Boundary locked.
2. Land Plan 2 (2–4 sessions, mostly mechanical). Slop quarantined.
3. In parallel: Plan 3 and Plan 4 (each 1–2 sessions).
4. Treat pi-mono adoption as opportunistic — pick one recommendation per week.

If executing with parallel subagents (per `superpowers:dispatching-parallel-agents`): Plans 1 and 4 can begin together; Plan 2 must wait on Plan 1; Plan 3 waits on Plan 2.

## Success criteria

This roadmap is done when all of the following are true on `main`:

- `PRODUCT_SURFACE.md` exists and is referenced by `HEIWA.md`.
- `scripts/audit_product_surface.sh` runs in CI and the build fails when slop budget is exceeded.
- A new contributor running `heiwa doctor` from a fresh clone gets a usable, evidence-producing local runtime in under 5 minutes.
- The slop budget report's quoted numbers update on each merge via CI artifact.
- `apps/heiwa_hub` and `packages/heiwa_skills` are either quarantined under `legacy/` or have an explicit "kept for X reason" annotation in `PRODUCT_SURFACE.md`.

## Hard rules carried from `HEIWA.md` and `CLAUDE.md`

- Do not delete legacy surfaces in this set of plans. Quarantine only. Deletion happens after one full release cycle of "no traffic" evidence.
- Do not invent new product surfaces in service of cleanup. The boundary captures what exists, not what we wish existed.
- Honesty over completeness theater. If `apps/heiwa_app` is a partial native shell, the surface doc says so.
- `heiwa` (the binary) is the product. Repo cleanup serves that, not the reverse.

## What this roadmap does NOT cover

- Deletion of quarantined surfaces (deferred to a later cycle)
- New feature work in `crates/*` (orthogonal — does not block on cleanup)
- SpacetimeDB schema changes (governed elsewhere)
- Provider adapter parity work (governed by `apps/heiwa_shell` plans)
- Hosted/cloud rollout (deferred per `HEIWA.md` Stage 3+)
