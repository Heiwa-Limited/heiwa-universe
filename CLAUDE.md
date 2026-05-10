# CLAUDE.md — heiwa-universe

This repository builds Heiwa, a local-first AI runtime and enterprise platform. Claude Code is one wrapped provider surface inside Heiwa, not the product itself.

## Claude's Role Here

- Claude Code is a peer executor alongside Codex, Gemini CLI, Antigravity, and local model runtimes.
- Claude owns its own native tools, system prompts, auth semantics, model availability, and quota behavior.
- Heiwa adds repo-local context, routing, evidence, shell ergonomics, and cross-provider normalization.
- Do not write docs or code that implies Heiwa owns Claude's inference internals.

## Required Reading

Before touching runtime or architecture work, read in this order:

1. `HEIWA.md`
2. `AGENTS.md`
3. `.claude/settings.json`
4. `.claude/settings.local.json`

## Current Product Truth

- The installed `heiwa` runtime is the current product center.
- `apps/heiwa_shell/` is the primary operator surface in this repo.
- `apps/heiwa_core/` contains the Rust execution kernel and hosted runtime path.
- STDB-facing active work lives in `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, and `crates/heiwa_stdb/`.
- `legacy/apps/heiwa_hub/` is quarantined migration/reference material, not a current mutation target.
- Web and `/code` surfaces are later work. Do not overstate them.

## Provider Truth

Heiwa wraps provider-owned runtimes:

- Claude Code
- Codex
- Gemini CLI
- Antigravity
- Ollama and later local runtimes

Integration maturity is not identical across them. Be explicit about what is truly wired today.

## Commands

```bash
cargo build --workspace
cargo test -p heiwa-shell --test smoke -- --nocapture
cargo test -p heiwa-loop -- --nocapture
```

Use targeted crate tests before claiming runtime progress.

## Hard Rules

- local-first truth over web-first framing
- provider-owned semantics stay provider-owned
- SpacetimeDB is backend authority, not a normal operator surface
- GitHub is the distribution surface; a cloud/VPS plane is deferred until traction warrants it
- honesty over completeness theater
