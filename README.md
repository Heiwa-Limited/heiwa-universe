# Heiwa

[![SpacetimeDB](https://img.shields.io/badge/state-SpacetimeDB-0c73d8?style=flat-square)](https://spacetimedb.com)
[![Web](https://img.shields.io/badge/dashboard-app.heiwa.ltd-000000?style=flat-square)](https://app.heiwa.ltd)

Heiwa is a local-first AI runtime and enterprise platform. It normalizes access to provider subscriptions, API keys, local models, device capabilities, and evidence through a unified execution kernel.

## One-Sentence Truth

`heiwa` is the installed product surface, DREX is the internal execution kernel, SpacetimeDB is the adjudication and evidence plane, Rust proposes and executes, and `heiwa` presents.

## Core Pillars

- **Local-First Runtime** -- `heiwa` on your machine is the primary operator surface.
- **DREX Kernel** -- Advanced intent classification, risk scoring, and device-aware routing.
- **Evidence-First** -- Every task, model selection, and run is adjudicated and persisted in SpacetimeDB.
- **Provider Neutral** -- Unified access to Claude Code, Codex, Gemini CLI, Antigravity, and local runtimes like Ollama.
- **Sovereign Execution** -- Prefer local models and local devices for privacy and cost control.

## Architecture

| Layer | Canonical meaning | Location |
| --- | --- | --- |
| **Heiwa** | Company and product identity | Repo root |
| **`heiwa`** | Primary installed runtime and operator surface | `apps/heiwa_shell/` |
| **DREX** | Internal execution kernel and routing substrate | `apps/heiwa_core/` |
| **SpacetimeDB** | Adjudication, canonical state, and evidence plane | `apps/heiwa_hub/spacetimedb/` |
| **Rust runtime** | Volatile execution: provider supervision and candidate generation | `crates/` |

> Rust proposes, SpacetimeDB adjudicates, `heiwa` presents.

## Quick Start

```bash
# 1. Install and verify baseline
bash scripts/check_runtime_baseline.sh

# 2. Build the local runtime
cargo build -p heiwa-shell

# 3. Run install and doctor
./target/debug/heiwa-shell install
./target/debug/heiwa-shell doctor

# 4. List providers and auth
./target/debug/heiwa-shell providers
./target/debug/heiwa-shell auth status
```

## Key Manifests

- [`HEIWA.md`](HEIWA.md) -- **The Canonical Truth** (Read this first)
- [`docs/standards/runtime-baseline.md`](docs/standards/runtime-baseline.md)
- [`config/swarm/BUILD_BLUEPRINT_2026-03-06.md`](config/swarm/BUILD_BLUEPRINT_2026-03-06.md)
- [`justfile`](justfile) -- Build and task contract
