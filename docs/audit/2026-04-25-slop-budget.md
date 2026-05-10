# Slop Budget Proposal — 2026-04-25

> **Type:** Report (proposal). The CI threshold values here are inputs to Plan 3 (`2026-04-25-repo-hygiene-ci-gates.md`).

**Purpose:** Without a measurable cap, slop returns. This document defines the budget — what is acceptable, what is failing, and what triggers CI failure.

**Baseline source:** `docs/audit/2026-04-25-current-state.md`

---

## Why a budget at all

Every codebase accumulates non-product LOC: generated bindings, planning docs, archived experiments. That is not inherently bad — it becomes a problem when:

1. Contributors cannot tell what is product (signal/noise breaks)
2. CI runs over irrelevant code (cost / time tax)
3. Search results dilute (`grep` noise)
4. Stale docs misrepresent product maturity to outsiders
5. New code drifts because there is no enforced shape

A budget answers "how much non-product is OK, by class." A breach is a discussion trigger, not necessarily a block.

## Budget by class

| Class | Hard cap (LOC) | Soft cap (LOC) | Rationale |
| --- | --- | --- | --- |
| `product` | none | none | Grow as needed |
| `generated` | 75,000 | 50,000 | Bindings naturally large but should not exceed product. Currently 51k → at soft cap |
| `legacy` | 60,000 | 30,000 | Should shrink each quarter. Currently ~120k → **2x over hard cap** |
| `reference` | 40,000 | 25,000 | Plans + design + audits. Currently ~28k → near soft cap, OK |
| `archive` | 20,000 | 10,000 | Frozen, should be small; if it grows, things are not actually being deleted |
| `vendored` | 5,000 | 1,000 | Vendored deps should be the rare exception |
| `runtime-artifact` | 0 | 0 | **Any tracked runtime artifact is a CI failure** |

## Current state vs budget

| Class | Current | Hard cap | Status |
| --- | --- | --- | --- |
| `product` | ~40k | n/a | OK |
| `generated` | ~51k | 75k | OK (above soft cap) |
| `legacy` | ~120k | 60k | **FAIL — 2x over** |
| `reference` | ~28k | 40k | OK |
| `archive` | ~5k | 20k | OK |
| `vendored` | 0 | 5k | OK |
| `runtime-artifact` | leakage detected | 0 | **FAIL — any presence** |

## Growth-rate caps

Per-merge deltas to `main`:

| Class | Max LOC delta per PR | Override path |
| --- | --- | --- |
| `legacy` | +0 (no new legacy) | Explicit `legacy-add: <reason>` in PR body |
| `generated` | +5,000 | Bindings regen — no override needed if delta is from a registered generator |
| `reference` | +2,000 | `reference-add: <reason>` for large doc adds |
| `archive` | +500 | `archive-add: <reason>` |
| `vendored` | +0 | `vendor-add: <reason>` + workspace owner ack |
| `runtime-artifact` | 0 | **No override** |

## CI threshold values (input to Plan 3)

The audit script will exit non-zero when any of these are true:

```
LEGACY_HARD_CAP=60000
GENERATED_HARD_CAP=75000
REFERENCE_HARD_CAP=40000
ARCHIVE_HARD_CAP=20000
VENDORED_HARD_CAP=5000
RUNTIME_ARTIFACT_TOLERANCE=0
PRODUCT_TO_NONPRODUCT_RATIO_FLOOR=0.20  # product must be >=20% of total
```

Soft-cap breaches surface as warnings in the CI job summary but do not fail the build.

## Reduction targets and timelines

To bring `legacy` under hard cap (60k from current ~120k), 60k LOC needs to leave the tracked set within the next 6 months.

Suggested cadence:

| Window | Action | Estimated LOC removed |
| --- | --- | --- |
| Cycle 1 (May 2026) | Quarantine `apps/heiwa_hub` under `legacy/` (move only) | 0 from tracked, but reclassed |
| Cycle 1 | Quarantine `packages/heiwa_skills` | 0 from tracked, reclassed |
| Cycle 2 (Jun 2026) | Quarantine `packages/heiwa_cognition`, `packages/heiwa_ui` | 0 from tracked, reclassed |
| Cycle 3 (Jul 2026) | First deletion pass: anything in `legacy/` with no traffic for one full release | -30k |
| Cycle 4 (Aug 2026) | Second deletion pass | -30k |

Note: quarantine alone does not reduce LOC. It reclasses files so the budget reflects intent. Deletion happens only after a quarantine soak period, so the budget is the lever, not the destruction.

## Exception: `apps/heiwa_trading`

`apps/heiwa_trading` is 5,354 LOC and is classed `product (subproduct)` per `~/heiwa-universe/CLAUDE.md` — the Polymarket paper-trading tournament was absorbed into the canonical monorepo. It is not slop and not legacy; it is a separate active product that ships out of the same workspace.

The budget should treat its LOC as `product`, not as drag.

## How the budget is enforced

Three layers:

1. **CI job** (`audit-product-surface`) runs `scripts/audit_product_surface.sh` on every PR, fails on hard-cap breach
2. **Pre-commit hook** rejects tracked runtime artifacts locally
3. **PR description bot** (future) parses `legacy-add:`/`vendor-add:` overrides and surfaces them in review

## What this report does NOT do

- Mandate deletion (deletion plans come from cycle 3+)
- Override `HEIWA.md` honesty rules ("legacy" is not a value judgment, it is a class)
- Touch `crates/*` or active product surfaces
- Set per-file ownership (a separate CODEOWNERS conversation)
