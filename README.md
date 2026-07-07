# Heiwa

[![CI](https://github.com/Strategizing/heiwa-universe/actions/workflows/ci.yml/badge.svg)](https://github.com/Strategizing/heiwa-universe/actions/workflows/ci.yml)
[![Docs](https://github.com/Strategizing/heiwa-universe/actions/workflows/pages.yml/badge.svg)](https://github.com/Strategizing/heiwa-universe/actions/workflows/pages.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Heiwa is a local-first AI operating layer. The installed `heiwa` runtime is the product center, Rust owns the execution path, and this repository is being hardened for GitHub-native distribution rather than hosted-platform theater.

## What Heiwa Does

> Heiwa watches what matters, summarizes what changed, stages what needs action, executes what is safe, and proves what happened.

Three planes compose one flow:

- **Intake** — operator command bar and passive feeds (mail, calendar, messages, files, runtime alerts).
- **Execution** — DREX routes work to local models, provider CLIs, tools, and connectors under leases and approval gates.
- **Evidence** — every read or action emits a source-linked receipt, mirrored to SpacetimeDB when online.

Maturity is uneven across planes today; see [`HEIWA.md`](HEIWA.md#the-three-planes) for current vs target.

## One-Sentence Truth

`heiwa` is the installed product surface, DREX is the internal execution kernel, SpacetimeDB is the adjudication and evidence sync backend, Rust proposes and executes, and `heiwa` presents.

## Current Repo Focus

- Installed runtime: `apps/heiwa_shell/`
- Core execution and routing: `apps/heiwa_core/`, `crates/heiwa_loop/`, `crates/heiwa_session/`
- Provider normalization: `crates/heiwa_provider/`
- Terminal UX: `crates/heiwa_tui/`, `crates/heiwa_repl/`
- STDB-facing Rust surfaces: `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, `crates/heiwa_stdb/`
- Legacy STDB module reference: `legacy/apps/heiwa_hub/spacetimedb/`
- GitHub distribution surfaces: Actions, Pages, and release metadata

## Architecture

| Layer            | Canonical meaning                                                                          | Location                                                                               |
| ---------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| **Heiwa**        | Company and product identity                                                               | Repo root                                                                              |
| **`heiwa`**      | Primary installed runtime and operator surface                                             | `apps/heiwa_shell/`                                                                    |
| **DREX**         | Internal execution kernel and routing substrate                                            | `apps/heiwa_core/`                                                                     |
| **SpacetimeDB**  | Adjudication, canonical state, and evidence sync backend (materializes the Evidence plane) | `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, `crates/heiwa_stdb/` |
| **Rust runtime** | Volatile execution: provider supervision and candidate generation                          | `crates/`                                                                              |

> Rust proposes, SpacetimeDB adjudicates, `heiwa` presents.

## Quick Start

```bash
# Verify the local toolchain baseline
bash scripts/check_runtime_baseline.sh

# Build the installed runtime
cargo build -p heiwa-shell

# Run install and doctor
cargo run -p heiwa-shell --bin heiwa -- install
cargo run -p heiwa-shell --bin heiwa -- doctor

# Inspect providers and auth state
cargo run -p heiwa-shell --bin heiwa -- providers
cargo run -p heiwa-shell --bin heiwa -- auth status
```

## Platform Lane

- CI runs Rust build/test/clippy across macOS, Linux, and Windows.
- Docs publish through GitHub Pages on release tags.
- Cargo manifests now carry shared package metadata for release readiness.
- Release archives include the Apache-2.0 license and contributor materials.
- Contributor, security, pull request, and issue templates live under `.github/` and `SECURITY.md`.

## Read First

- [`HEIWA.md`](HEIWA.md)
- [`docs/product-contract.md`](docs/product-contract.md)
- [`docs/capability-fabric.md`](docs/capability-fabric.md)
- [`docs/local-self-operation.md`](docs/local-self-operation.md)
- [`AGENTS.md`](AGENTS.md)
- [`BUILD_MATRIX.md`](BUILD_MATRIX.md)
- [`SECURITY.md`](SECURITY.md)
- [`docs/`](docs/)
