# AGENTS.md — heiwa-universe

This repository builds the Heiwa full stack. The current product center of gravity is the installed `heiwa` runtime on the user's machine.

## Canonical Product Truth

- **Heiwa** is the company and product identity.
- **`heiwa`** is the primary installed operator surface.
- **DREX** is the internal execution kernel.
- **Local MacBook state** under `~/.heiwa/` plus this checkout is the current owner runtime truth.
- **SpacetimeDB** is the adjudication, subscription, and evidence sync plane when enabled.
- **GitHub and Cloudflare** are support infrastructure for distribution, docs, edge, and later remote surfaces.

Compression:

> Rust proposes and executes, local Heiwa state records current user truth, SpacetimeDB syncs evidence, `heiwa` presents.

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

- User-functionality stack is **Rust + TypeScript + Shell** on Devon's MacBook first.
- **Rust** owns the authoritative state layer, orchestration, routing, and future DREX execution logic.
- **TypeScript** owns companion visual surfaces and typed client contracts.
- **Shell** remains the bootstrap and operator glue layer for the local runtime plus future Linux/WSL execution.
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
- This checkout plus `~/.heiwa/` are the current source-of-truth/server for user functionality.
- Users/operators should not have to think about SpacetimeDB directly.
- GitHub is source, CI, and release distribution.
- Cloudflare is paused public edge and later remote-surface infrastructure.

## Agentic Runtime Workflow

Use [`docs/local-self-operation.md`](docs/local-self-operation.md#agentic-runtime-workflow) before starting, stopping, probing, or changing the local app runtime.

- Treat `7474` as the installed product runtime; after code edits, verify the current checkout on a temporary alternate port such as `7475`.
- Never assume a reachable localhost app is the binary you just changed; check `cli_path`, port, and endpoint behavior.
- If a new API endpoint returns `index.html`, assume stale or wrong runtime until proven otherwise.
- Prompt for update/restart when runtime, cockpit assets, or schema boundaries require it; auto-restart only when explicitly enabled and no active work or only safely paused work exists.
- Stop every runtime process you started before final reporting unless Devon asked to keep it running.
- Remove temporary probe files and fixtures as you go; never delete durable `~/.heiwa/state` evidence without explicit approval.

GitHub plus Cloudflare are the public install source: GitHub owns source, releases, checksums, and CI evidence; Cloudflare may front docs, install pages, update manifests, and status, but must not become a second binary authority.

Heiwa must initialize and adapt per machine through `~/.heiwa/machine.json`; do not hardcode one-user or one-device assumptions into runtime behavior.

Promotion rule (Local-first bypass posture): Local sandbox verification is the canonical gate. Verify the checkout with the sandbox release checks, merge to `main` locally, and then promote to the installed `heiwa` runtime via local checkout updates (`heiwa app update --source checkout`). Bypassing GitHub Actions and PR checks is standard to prevent remote blockers from stopping operator progress.

## Working Priorities

Every work item must classify as Intake, Execution, Evidence, or out-of-scope (see [Three Planes in `HEIWA.md`](HEIWA.md#the-three-planes)). If it does not advance a plane, defer or reject.

Prioritize this order:

1. local runtime truth
2. provider/account normalization
3. evidence and bounded execution
4. internal backend sync and optional remote support paths
5. remote surfaces such as `/code`

Do not optimize for maturity theater first:

- do not overstate `/code`
- do not over-rotate into web-console-first language
- do not introduce a hosted control plane as the product center
- do not pretend every wrapped provider is equally integrated

## Required Reading

Before making architecture or runtime changes, read:

1. `HEIWA.md`
2. this file
3. `docs/local-self-operation.md`
4. `CLAUDE.md` or `GEMINI.md` when working through that provider surface

If repo docs drift, `HEIWA.md` is the canonical architecture file.
