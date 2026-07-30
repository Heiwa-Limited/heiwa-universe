# Heiwa Build Matrix

Updated 2026-07-27. Canonical repo shape: MacBook-first runtime, local `~/.heiwa` state, GitHub-native distribution, paused Cloudflare public edge, local-first JSONL evidence with a Lance recall index. SpacetimeDB was extracted 2026-07-15.

## Architecture

| Layer                       | Owner                                 | Current path                                                           | Status                                                               |
| --------------------------- | ------------------------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Installed runtime           | Rust + Shell                          | `apps/heiwa_shell/`                                                    | Active product surface                                               |
| Execution kernel            | Rust                                  | `apps/heiwa_core/`, `apps/heiwa_orchestrator/`, `crates/heiwa_loop/`   | Active substrate                                                     |
| Provider/auth normalization | Rust                                  | `crates/heiwa_provider/`, `crates/heiwa_vault/`, `crates/heiwa_quota/` | Active, uneven by provider                                           |
| Session and local memory    | Rust + SQLite mirror                  | `crates/heiwa_session/`, `crates/heiwa_memory/`                        | Active local substrate                                               |
| Companion app               | TypeScript public shell + cockpit SPA | `apps/heiwa_app/clients/web/`, `apps/heiwa_app/clients/cockpit/`       | Web client today; native wrapper later                               |
| Evidence / recall           | JSONL journal + Lance index           | `crates/heiwa_evidence/`, `crates/heiwa_embed/`                        | Local truth plane; Lance is derived and rebuildable                  |
| Python sidecar/reference    | Python                                | `runtime/python/`, `packages/heiwa_sdk/`, `apps/heiwa_trading/`        | Compatibility/R&D sidecars, not product center                       |
| Distribution                | GitHub                                | `.github/workflows/{ci,release,pages}.yml`                             | CI, release archives, GHCR image, docs                               |
| Public edge                 | Cloudflare                            | `apps/heiwa_app/wrangler.toml`, `infra/platform/cloudflare/`           | Paused until user functionality is solid; no local runtime authority |

## Product Contract

Rust proposes and executes. Local `~/.heiwa` state records current owner truth; JSONL evidence under `~/.heiwa/evidence/` is the authority plane and Lance is the derived recall index. `heiwa` presents. TypeScript renders public and cockpit surfaces. Python remains sidecar/reference until promoted behind explicit Rust-owned contracts.

## Current Work Lanes

| Lane         | Goal                                                                                   | Gate                                                                                                      |
| ------------ | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Runtime      | Keep `heiwa` local runtime installable, updateable, and honest about provider maturity | `cargo test -p heiwa-shell --test smoke`, `heiwa doctor --ai-ops`, `heiwa app runtime status --json`      |
| State        | Local SQLite/files are owner truth; JSONL evidence is the authority plane              | `cargo test -p heiwa-session --test transcript_migration`, `cargo test -p heiwa_evidence`                 |
| Web          | Keep public shell static/safe and cockpit local-runtime oriented                       | `npm run typecheck`, `python apps/heiwa_app/scripts/check_static_surface.py`                              |
| Python       | Keep sidecars dependency-clean and non-authoritative                                   | `uv run --extra dev python -m pytest` where relevant; lockfiles must have no open Dependabot alerts       |
| Distribution | Publish through GitHub Releases/GHCR and docs through GitHub Pages                     | `.github/workflows/release.yml`, `.github/workflows/pages.yml`, `scripts/check_release_metadata.sh`       |
| Edge         | Keep Cloudflare DNS/Pages/WAF declarations aligned with public shell reality           | `apps/heiwa_app/wrangler.toml`, `infra/platform/cloudflare/main.tf`, live Cloudflare auth before mutation |

## Active Baseline

- `main` is trunk and branch-protected.
- GitHub CI requires security scan, Rust matrix, TypeScript lint, docs build, agent sync, and repo hygiene.
- GitHub Releases build `heiwa` archives for Linux, macOS arm64, and Windows plus checksums and GHCR image.
- GitHub Pages publishes docs on release tags and manual dispatch.
- Cloudflare Pages is not active public access yet; when re-enabled it should host the public web shell from `apps/heiwa_app/clients/web`.
- The cockpit SPA under `apps/heiwa_app/clients/cockpit` is served by `heiwa app start` on localhost, not assumed to be a privileged hosted runtime.
- There is no hosted authority plane. Evidence sync to GitHub is future, redaction-gated work; the local runtime is complete without it.

## Retired Assumptions

- Do not use `apps/heiwa_cli` as the installed runtime path; current runtime is `apps/heiwa_shell`.
- Do not describe a hosted control plane as the default product center.
- Do not describe Python Hub/cognition as the long-term control plane.
- Do not treat `apps/heiwa_app/clients/web` and `clients/cockpit` as the same thing: public shell is static and safe; cockpit is the local operator UI.
- Do not list `auth.heiwa.ltd` or `trade.heiwa.ltd` as active public surfaces until they have a verified hosted target.

## Branch / Worktree Policy

- Work directly on root `main` for owner-local consolidation unless Devon asks for branch isolation.
- Branches/worktrees are temporary only; commit promoted work to `main` or scrap it.
- Preserve dirty worktrees until their local changes are promoted, archived, or explicitly discarded.
- Delete merged/superseded remote branches only after checking live PR state and attached worktrees.
