# GEMINI.md — heiwa-universe

This repository builds Heiwa, a local-first AI runtime and enterprise platform. Gemini CLI is one wrapped provider surface inside Heiwa, not the product itself.

## Gemini's Role Here

- Gemini CLI is a peer executor alongside Claude Code, Codex, Antigravity, and local model runtimes.
- Gemini owns its own native auth flows, system prompts, cloud model inventory, quota behavior, and tool semantics.
- Heiwa adds repo-local context, routing, evidence, shell ergonomics, and cross-provider normalization.
- Do not imply that Heiwa owns Gemini's inference internals.

## Required Reading

Before touching runtime or architecture work, read in this order:

1. `HEIWA.md`
2. `AGENTS.md`
3. `.gemini/settings.json`
4. `.gemini/policies/heiwa-executive.toml`

## Current Product Truth

- The installed `heiwa` runtime is the current product center.
- `apps/heiwa_shell/` is the main operator surface in this repo.
- `apps/heiwa_core/` contains the Rust execution kernel and hosted runtime path.
- `apps/heiwa_hub/spacetimedb/` is the backend authority plane.
- Web and `/code` surfaces are later work. Do not overstate them.

## Provider Truth

Heiwa wraps provider-owned runtimes:

- Claude Code
- Codex
- Gemini CLI
- Antigravity
- Ollama and later local runtimes

Integration maturity differs across those providers. Discovery is not the same as full execution parity.

## Commands

```bash
cargo build --workspace
cargo test -p heiwa-shell --test smoke -- --nocapture
cargo test -p heiwa-loop -- --nocapture
```

## Hard Rules

- local-first truth over hosted-first framing
- provider-owned semantics stay provider-owned
- SpacetimeDB is backend authority, not a normal operator surface
- Railway is support infra, not the product center
- honesty over maturity theater
