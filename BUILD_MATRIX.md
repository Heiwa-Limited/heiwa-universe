# Heiwa Build Matrix

Updated 2026-04-21. Client-only architecture. Parallel lanes for Claude + Codex dispatch.

## Architecture (client-only)

| Layer | Component | Crate/Path |
| --- | --- | --- |
| L1 Runtime | `heiwa` CLI (Ratatui TUI) | `apps/heiwa_cli`, `crates/heiwa_tui`, `crates/heiwa_repl` |
| L1 Runtime | Heiwa.app (Tauri desktop) | `apps/heiwa_web` (rename → `apps/heiwa_app`) |
| L2 Secrets | OS keychain vault | `crates/heiwa_vault` (NEW) |
| L2 Providers | OAuth bridges — claude/gemini/codex/ollama | `crates/heiwa_provider` |
| L3 Routing | DREX kernel | `crates/heiwa_loop`, `crates/heiwa_session` |
| L4 Tools | MCP server + client | `crates/heiwa_mcp` (NEW) |
| L5 Sidecars | Python: LangGraph, LlamaIndex, Ragas | `runtime/python/` (NEW) |
| L6 State | SQLite ledger, quota, history | `crates/heiwa_session` (extend) |
| L7 Distribution | GitHub Releases + homebrew tap | `infra/platform/github/` |
| L7 Distribution | Landing + docs | GitHub Pages via `mkdocs` |

Frozen: `apps/heiwa_hub/spacetimedb/` — reference schema only, no new work.

## Lanes

Two lanes run in parallel. Claude owns **LOCAL**, Codex owns **PLATFORM**, plus a shared **CLEANUP** backlog either picks up.

### Lane A — LOCAL (Claude)

Build the client runtime and routing.

| ID | Task | Crate/Path | Depends |
| --- | --- | --- | --- |
| L1 | Scaffold `heiwa_vault` crate (macOS Keychain / Secret Service / Win Credential Manager) | `crates/heiwa_vault` | — |
| L2 | OAuth credential bridge: `get_oauth_session_for_user(provider)` | `crates/heiwa_provider` | L1 |
| L3 | Local SQLite quota ledger (replaces STDB cross-device) | `crates/heiwa_session` | — |
| L4 | DREX → vault wiring: route selects session, checks local quota | `crates/heiwa_loop` | L2, L3 |
| L5 | `heiwa_mcp` server + client scaffold | `crates/heiwa_mcp` (NEW) | — |
| L6 | Port `~/bin/ai` routing table into DREX as reference implementation | `crates/heiwa_loop` | L4 |
| L7 | `heiwa init` flow (first-run: detect providers, prompt OAuth, write config) | `apps/heiwa_cli` | L1, L2 |
| L8 | Python sidecar scaffold (`uv` + pyproject, LangGraph+Ragas health checks) | `runtime/python/` | — |

### Lane B — PLATFORM (Codex)

Make the repo publishable and distributable via GitHub.

| ID | Task | Path | Depends |
| --- | --- | --- | --- |
| P1 | GitHub Actions: cargo build matrix (macOS+Linux+Windows), test, clippy | `.github/workflows/ci.yml` | — |
| P2 | GitHub Actions: release workflow (cargo-dist or cross-build → Releases) | `.github/workflows/release.yml` | P1 |
| P3 | Homebrew tap repo `Strategizing/homebrew-heiwa` with auto-update formula | external + `infra/platform/github/` | P2 |
| P4 | GitHub Pages: mkdocs site (landing + docs) under `docs/`, publish on tag | `.github/workflows/pages.yml`, `mkdocs.yml` | — |
| P5 | Repo hygiene: strip Railway refs from docs, agents, policies | `docs/`, `.claude/`, `.gemini/`, `ops/` | — |
| P6 | `heiwa` crate metadata for cargo publish (license, README, keywords) | `Cargo.toml` manifests | — |
| P7 | Plugin install protocol spec: `heiwa install gh:owner/repo` | `docs/plugins.md`, `crates/heiwa_install` | — |
| P8 | CONTRIBUTING.md + issue templates + CODE_OF_CONDUCT.md | `.github/` | — |

### Lane C — CLEANUP (shared backlog)

Either agent claims. Keeps main repo portable.

| ID | Task |
| --- | --- |
| C1 | Delete `apps/heiwa_orchestrator/`, `apps/heiwa_limbs/` if Railway-only; confirm before delete |
| C2 | Audit `apps/heiwa_shell/` vs `apps/heiwa_cli/` — merge or split cleanly |
| C3 | Rename `apps/heiwa_web/` → `apps/heiwa_app/` (Tauri desktop, not web) |
| C4 | Purge `railway` strings from `crates/heiwa_protocol/`, agent policies, swarm docs |
| C5 | Delete or archive `apps/heiwa_trading/` from main tree if not MVP scope |
| C6 | Unify `HEIWA.md`, `IDENTITY.md`, `SOUL.md` into one canonical `HEIWA.md` |
| C7 | Prune `docs/superpowers/plans/` of obsolete Railway plans |

## Build order

1. Lane A starts L1, L3, L5 in parallel (no deps).
2. Lane B starts P1, P4, P5, P6, P8 in parallel (no deps).
3. Cleanup C1–C7 ride along where they unblock specific lane tasks.
4. Merge point: L7 `heiwa init` + P3 homebrew tap → first shippable alpha.

## Worktree convention

- Claude works in `.worktrees/claude/<task-id>/`
- Codex works in `.worktrees/codex/<task-id>/`
- Branch name mirrors path: `claude/<task-id>` or `codex/<task-id>`
- All worktrees ignored by git (`.worktrees/` in `.gitignore`)
- Short-lived: delete worktree + branch after PR merge

## Git state (2026-04-21 consolidation)

- `main` is trunk; all other local branches deleted
- Pre-consolidation branches preserved as `backup/*-20260421` tags (pushed to origin)
- Lost origin branches: `feat/heiwa-terminal-v1-cockpit`, `phase2-auth-plane` — left intact on remote as extra backup
- Known CVE: GitHub flagged 6 dependabot alerts on `main` — audit during P5

## Non-goals (deferred)

- VPS or hosted backend
- Cross-device sync
- Plugin marketplace (use `gh:` URLs instead)
- STDB cloud authority plane
- Heiwa identity service (GitHub device-flow is enough)
