# HEIWA ENVIRONMENT MANIFEST (v3.0 - MacBook Node A)

> Last updated: 2026-03-14. Reflects MacBook M4 Pro as the primary orchestrator node.
> WSL/Ubuntu (Node B: Ryzen 7700X + RTX 3060) is the GPU worker — update paths there separately.

## Node A — MacBook M4 Pro (Primary Orchestrator)

### Core Paths
- `HEIWA_ROOT`: `/Users/dmcgregsauce/heiwa` (canonical monorepo)
- `HEIWA_HOME`: `~/.heiwa` (operator state, cache, summaries)
- Canonical CLI: `/Users/dmcgregsauce/heiwa/apps/heiwa_cli/heiwa`

### Key Environment Variables
- `HEIWA_ROOT` — monorepo root; auto-discovered by CLI if not set
- `HEIWA_WORKSPACE_ROOT` — used by subprocess wrappers (ollama_exec.py) to find `config/agents.yaml`
- `HEIWA_NODE_ID` — defaults to `macbook@heiwa-agile`
- `HEIWA_AUTH_TOKEN` — hub auth token (set in `.env`)
- `HEIWA_STATE_BACKEND` — `spacetimedb` | `postgres` | `sqlite` (defaults to sqlite locally)
- `HEIWA_ENABLE_BROKER` — `true` | `false` (default: `true`)
- `HEIWA_OLLAMA_URL` — defaults to `http://127.0.0.1:11434`
- `HEIWA_OLLAMA_MODEL` — overrides resolved model for Ollama adapter
- ~~`NATS_URL`~~ — removed; transport now uses LocalBusTransport (co-located) + WebSocket (remote workers)

### Local Model State (as of 2026-03-14)
Installed via Ollama (`ollama list`):
- `qwen3.5:4b` — primary for node A orchestration/reasoning
- `qwen3-embedding:0.6b` — embeddings
- `qwen2.5-coder:1.5b`, `qwen2.5-coder:0.5b` — code-first tasks
- `llama3.2:3b` — fallback
- Target (not yet pulled): `llama-4-scout:q4_k_m`, `glm-4.7-flash:q4_k_m`

### Ollama
- Binary: `/opt/homebrew/bin/ollama`
- Start: `ollama serve` (background)
- Models stored in Ollama's default location
- `config/agents.yaml` provides fallback chain for ollama_exec.py wrapper

### Active Config Files
- `config/swarm/ai_router.json` — model registry, provider routing
- `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
- `config/agents.yaml` — ollama wrapper config (created 2026-03-14)
- `config/identities/profiles.json` — HeiwaCells agent catalog

### Infrastructure Services (local, manual start)
- Ollama: `ollama serve`
- Hub: `python -m apps.heiwa_hub.main`
- Note: NATS is no longer required. Agent transport is handled by LocalBusTransport for co-located agents and WebSocket for remote workers (see `packages/heiwa_sdk/heiwa_sdk/transport.py`).

### DB Backend (local)
- Default: `HEIWA_STATE_BACKEND=sqlite` → `hub.db` (SQLite)
- STDB: Railway-hosted when `HEIWA_STATE_BACKEND=spacetimedb`
- SQLite schema includes: proposals, nodes, runs, missions, mission_steps, cell_runs, artifacts, and more

## Node B — Ryzen 7700X + RTX 3060 (GPU Worker)
- Headless Linux worker; connect via SSH
- Primary models: `Qwen2.5-Coder-7B` Q4, `all-MiniLM-L6-v2`, `SDXL-Turbo`
- Managed via WSL/systemd; see node B's local manifest for exact paths

## Cloud (Railway)
- Control plane host
- SpacetimeDB authoritative state layer
- Fixed cost target: < $40/month

## Security
- Vault: `~/.heiwa/vault.env` (chmod 600)
- Redaction: `packages/heiwa_sdk/heiwa_sdk/security.py`
- Untrusted code: E2B sandboxes only — never run LLM-generated code on host
