# AGENTS.md — heiwa-universe

This repository builds the Heiwa full stack. The current product center of gravity is the installed `heiwa` runtime on the user's machine.

## Canonical Product Truth

- **Heiwa** is the company and product identity.
- **`heiwa`** is the primary installed operator surface.
- **DREX** is the internal execution kernel.
- **SpacetimeDB** is the backend adjudication, subscription, and evidence plane.
- **Railway, GitHub, and Cloudflare** are support infrastructure for hosted, deployment, edge, and enterprise needs.

Compression:

> Rust proposes and executes, SpacetimeDB adjudicates and records, `heiwa` presents.

## Current Repo Spine

| Path | Role |
| --- | --- |
| `apps/heiwa_shell/` | Installed `heiwa` runtime and shell surface |
| `apps/heiwa_app/` | Companion visual shell for the same runtime; web client today, native wrapper later |
| `apps/heiwa_core/` | Rust execution kernel and hosted runtime path |
| `apps/heiwa_orchestrator/` | DREX orchestration, scoring, and STDB-facing runtime work |
| `crates/heiwa_stdb/` | STDB evidence and offline fallback crate |
| `crates/heiwa_provider/` | Provider normalization and adapter surfaces |
| `crates/heiwa_install/` | Install and doctor flows |
| `crates/heiwa_session/` | Local session daemon primitives |
| `crates/heiwa_repl/` | REPL parsing and footer telemetry |
| `crates/heiwa_loop/` | Bounded loop workflow |
| `packages/heiwa_sdk/` | Python compatibility and migration surface |
| `packages/heiwa_bindings/` | Generated bindings for STDB types |
| `legacy/apps/heiwa_hub/` | Quarantined legacy Hub reference, not current product spine |

## Architecture Direction (April 2026)

- Production target stack is **Rust + TypeScript + Shell**.
- **Rust** owns the authoritative state layer, orchestration, routing, and future DREX execution logic.
- **TypeScript** owns companion visual surfaces and typed client contracts.
- **Shell** remains the bootstrap and operator glue layer for the local runtime plus hosted Linux/WSL execution.
- The Python Hub and cognition packages are still live in the repo, but they are prototype and compatibility surfaces, not the long-term control plane.

## Provider Truth

Heiwa wraps provider-owned runtimes. It does not own their internals.

- Claude Code, Codex, Gemini CLI, and Antigravity remain provider-owned CLI surfaces.
- Providers own their own system prompts, auth semantics, session behavior, cloud model inventory, and native quotas.
- Ollama and other local runtimes remain local-model providers, not Heiwa-native models.
- Heiwa adds local install/auth UX, routing, evidence, bounded loops, and operator coherence across those surfaces.

Be honest about maturity:

- discovery and wrapping may exist before parity does
- a provider may be known before it is fully loop-capable
- hosted surfaces exist in the repo, but they are not the current product center
- `apps/heiwa_app` is the companion visual shell path today, not a fully native desktop runtime yet

## Operator and Infra Truth

- `~/.heiwa/` is the owner-local runtime root on Devon's machine.
- Users/operators should not have to think about SpacetimeDB directly.
- Railway hosts supported Heiwa services; it does not define the product.
- GitHub is source, CI, and release distribution.
- Cloudflare is public edge and later remote-surface infrastructure.

## Working Priorities

Prioritize this order:

1. local runtime truth
2. provider/account normalization
3. evidence and bounded execution
4. internal backend sync and hosted support paths
5. remote surfaces such as `/code`

Do not optimize for maturity theater first:

- do not overstate `/code`
- do not over-rotate into web-console-first language
- do not treat Railway as the product center
- do not pretend every wrapped provider is equally integrated

## Required Reading

Before making architecture or runtime changes, read:

1. `HEIWA.md`
2. this file
3. `CLAUDE.md` or `GEMINI.md` when working through that provider surface

If repo docs drift, `HEIWA.md` is the canonical architecture file.
