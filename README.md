# Heiwa

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
# 1. Install the local heiwa runtime
cargo install --path apps/heiwa_shell --root ~/.heiwa --force

# 2. Bootstrap the runtime root
~/.heiwa/bin/heiwa install

# 3. Verify runtime ownership and projections
~/.heiwa/bin/heiwa doctor

# 4. Inspect providers
~/.heiwa/bin/heiwa providers
~/.heiwa/bin/heiwa auth status
```

Hosted installer payload for later publication lives at [`apps/heiwa_cli/scripts/install_heiwa.sh`](apps/heiwa_cli/scripts/install_heiwa.sh). Do not advertise a `curl ... | sh` URL until it is actually live.

## Key Manifests

- [`HEIWA.md`](HEIWA.md) -- **The Canonical Truth** (Read this first)
- [`docs/runtime-owned-heiwa.md`](docs/runtime-owned-heiwa.md)
- [`docs/standards/runtime-baseline.md`](docs/standards/runtime-baseline.md)
- [`config/swarm/BUILD_BLUEPRINT_2026-03-06.md`](config/swarm/BUILD_BLUEPRINT_2026-03-06.md)
- [`justfile`](justfile) -- Build and task contract
