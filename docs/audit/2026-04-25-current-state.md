# Current State Audit Report — 2026-04-25

> **Type:** Report (descriptive). No prescriptions here — see `docs/superpowers/plans/2026-04-25-codex-scope-roadmap.md`.

**Repo:** `heiwa-universe` on `main`
**Author:** Codex audit + Claude empirical recount
**Snapshot taken:** 2026-04-25

---

## Top-line numbers

- **Tracked files:** 2,016
- **Tracked LOC (sum across top-level zones):** ~259,883
- **Estimated product LOC:** ~40,000
- **Slop ratio:** ~85% non-product

The Codex estimate of "245k tracked / 40k product" is within 6% of empirical recount.

## LOC by top-level zone

| Zone | LOC | Notes |
| --- | --- | --- |
| `packages/` | 155,191 | Bulk concentrated in `heiwa_skills` (86k) + `heiwa_bindings` (51k) |
| `apps/` | 53,584 | `heiwa_hub` alone is 24,793 |
| `docs/` | 28,973 | `superpowers` + `design` dominate |
| `crates/` | 8,771 | **All product, all Rust** |
| `config/` | 5,285 | |
| `runtime/` | 3,861 | |
| `scripts/` | 2,255 | |
| `ops/` | 1,632 | |
| `infra/` | 331 | |

## `packages/` breakdown

| Subdir | LOC | Class (proposed) |
| --- | --- | --- |
| `heiwa_skills` | 86,477 | **legacy** — Python skills surface, superseded by `crates/heiwa_loop` + provider-owned skill systems |
| `heiwa_bindings` | 51,046 | **generated** — language bindings emitted from STDB schema |
| `heiwa_sdk` | 8,622 | **product** (Python SDK, current) |
| `heiwa_cli` | 3,199 | **product** (Python compat shim) |
| `heiwa_cognition` | 3,031 | **legacy** — superseded by Rust kernel |
| `heiwa_protocol` | 2,282 | **product** (shared schema) |
| `heiwa_ui` | 306 | **legacy** — old UI primitives |
| `heiwa_identity` | 228 | **product** (identity helpers) |

## `apps/` breakdown

| Subdir | LOC | Class (proposed) |
| --- | --- | --- |
| `heiwa_hub` | 24,793 | **legacy** — old Python ops surface; not the operator path per `HEIWA.md` |
| `heiwa_app` | 8,243 | **product** — companion visual shell (web client form) |
| `heiwa_core` | 5,670 | **product** — Rust execution kernel |
| `heiwa_trading` | 5,354 | **product (subproduct)** — Polymarket paper-trading tournament; kept per `CLAUDE.md` |
| `heiwa_cli` | 3,512 | **legacy** — Python CLI shim, superseded by `heiwa_shell` |
| `heiwa_shell` | 2,445 | **product** — primary operator surface |
| `heiwa_limbs` | 2,446 | **legacy / experimental** — Rust limb prototypes |
| `heiwa_orchestrator` | 1,104 | **product** — orchestration kernel |
| `heiwa_dj` | 17 | **archive** — pointer to `~/ai-dj/`, only stub remains |

## `docs/` breakdown

| Subdir | Files | Class (proposed) |
| --- | --- | --- |
| `superpowers/` | ~62 plans/specs/status/handoffs | **reference** (planning artifacts, not product docs) |
| `design/` | ~65 architecture exploration | **reference** |
| `audit/` | 1 prior report | **reference (operational)** |
| `enterprise/` | 3 | **product (planning)** |
| `standards/` | 2 | **product (governance)** |
| Top-level docs/*.md | ~20 | **product** (architecture, deployment, security, etc.) |

## `crates/` breakdown — all product

| Crate | Role |
| --- | --- |
| `heiwa_config` | Local profile / config |
| `heiwa_embed` | Embedding surface |
| `heiwa_install` | Install / update flows |
| `heiwa_loop` | Bounded loop execution |
| `heiwa_mcp` | MCP integration |
| `heiwa_protocol` | Wire protocol |
| `heiwa_provider` | Provider adapter normalization |
| `heiwa_quota` | Quota ledger |
| `heiwa_repl` | REPL surface |
| `heiwa_session` | Session management |
| `heiwa_stdb` | STDB client |
| `heiwa_tui` | TUI primitives |
| `heiwa_vault` | Secret storage |

13 crates, ~8,771 LOC total. Average ~675 LOC per crate. Tight, focused.

## File-class distribution (proposed taxonomy)

These are the seven classes from Codex's scope:

| Class | Meaning | Approx LOC |
| --- | --- | --- |
| `product` | Active surfaces shipping in `heiwa` binary or its companion app | ~40,000 |
| `generated` | Bindings, schema-derived code; should be reproducible from source schema | ~51,000 |
| `legacy` | Old surfaces kept for migration/reference, not in product contract | ~120,000 |
| `reference` | Plans, design docs, audits, ADRs | ~28,000 |
| `archive` | Frozen snapshots of removed work | ~5,000 |
| `vendored` | Third-party code copied in (rare, per workspace policy) | ~0 (clean today) |
| `runtime-artifact` | Logs, caches, tmp data — should never be tracked | ~minimal but present |

## Known runtime-artifact leakage on `main`

Files that should not be tracked but are:

- `__pycache__/` directories appearing under `apps/`, `packages/`, repo root
- `.pytest_cache/` under `apps/heiwa_trading`
- `node_modules/` (top-level — to verify)
- `target/`, `.venv/`, `.wrangler/` — verified untracked, but `.gitignore` should explicitly cover
- `runtime/` — partially logs vs partially product config; needs split

## What is NOT in this report

- Recommendations (see plans)
- Deletion candidates (see slop budget report)
- Migration sequencing (see roadmap)
- Pi-mono adoption (see pi-mono adoption report)

## Reproducing this audit

```bash
cd ~/heiwa-universe
git ls-files | wc -l              # tracked file count
git ls-files | awk -F/ '{print $1}' | sort | uniq -c | sort -rn  # files per top dir
for d in apps crates packages docs ops scripts infra config runtime; do
  count=$(git ls-files "$d" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
  echo "$d: $count LOC"
done
```

This script becomes the basis for `scripts/audit_product_surface.sh` in Plan 1.
