# CLAUDE.md — heiwa-universe

This repository builds Heiwa, a local-first AI runtime and enterprise platform. Claude Code is one wrapped provider surface inside Heiwa, not the product itself.

Naming:

- **Heiwa** = product/app/runtime/CLI/packages/docs.
- **Heiwa Limited** = company/publisher/legal identity.
- **Heiwa Universe** = this repo, `Heiwa-Limited/heiwa-universe`, private until the public security/readiness gate passes.

## Claude's Role Here

- Claude Code is a peer executor alongside Codex, Gemini CLI, Grok, Antigravity, and local model runtimes.
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
- Evidence-plane work lives in `crates/heiwa_evidence/` (JSONL journal truth: envelopes, locking, replay, recovery, compaction); core and orchestrator consume it through their `evidence/` shims. Lance is wired behind the `lance` feature in `crates/heiwa_embed/`, selected via `embedding.backend`. STDB was extracted 2026-07-15.
- Legacy surfaces (old Hub, CLI, limbs) were removed from the tree on 2026-07-06; they live in git history and `~/heiwa_archive/`. Do not treat them as work targets.
- Web and `/code` surfaces are later work. Do not overstate them.

## Provider Truth

Heiwa wraps provider-owned runtimes:

- Claude Code
- Codex
- Gemini CLI
- Grok
- Antigravity
- Ollama and later local runtimes

Integration maturity is not identical across them. Be explicit about what is truly wired today.

## Shared Peer Truth

Use corrected peer framing before architecture or parity work:

- Hermes is Python, server/VPS-friendly, terminal-first. It proves learning loop,
  skills, FTS5 recall, Honcho user modeling, messaging gateway, cron delivery,
  MCP, provider switching, and terminal backends. Do not call it a worker mesh.
- OpenHuman is Rust + Tauri/CEF with local memory plus managed default services.
  It proves consumer desktop onboarding, Memory Tree, Obsidian vault,
  Composio/OAuth integrations, TokenJuice, and voice/meeting surface. Do not
  call it pure local-first.
- Heiwa's defensible difference: provider-peer MacBook owner seat, local runtime
  authority, approvals, receipts, local-first evidence (GitHub sync planned,
  redaction-gated), and provider-owned runtime truth.
- Biggest current gap: connector/tool breadth and compression/learning loop.
  Do not imply parity until code proves it.

## Commands

```bash
cargo build --workspace
cargo test -p heiwa-shell --test smoke -- --nocapture
cargo test -p heiwa-loop -- --nocapture
bash scripts/check_agent_baseline.sh
```

Use targeted crate tests before claiming runtime progress. Run the baseline gate
before closing repo-health, promotion, or peer-agent handoff work.

## Hard Rules

- local-first truth over web-first framing
- provider-owned semantics stay provider-owned
- Backend is Lance + GitHub: text truth in git, Lance derived recall index, SQLite hot state. No hosted authority plane.
- GitHub is the distribution surface; a cloud/VPS plane is deferred until traction warrants it
- honesty over completeness theater
