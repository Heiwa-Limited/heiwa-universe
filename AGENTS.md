# AGENTS.md — The Heiwa Swarm Map

Heiwa is an omnidirectional fluid mesh of peer agents. The authoritative control plane is **Heiwa Core** (Rust).

## 1. Core Authority (`apps/heiwa_core/`)

- **Heiwa Core** (`heiwa-core`): The authoritative Rust orchestrator and gateway. Manages:
    - Unified WebSocket Pipe (`/ws`) for all clients.
    - DREX routing and model-tier selection.
    - Mission and Task lifecycle (STDB-backed).
    - Machine and session authentication.
    - Worker ingress for boost nodes.

## 2. Mesh Connectivity

- **Sovereign Boost Nodes**: Optional local nodes (Mac/WSL) that dial into `heiwa-core` to provide delegated execution and inference.
- **Heiwa CLI**: Operator surface connecting to the core auth/runtime plane.
- **app.heiwa.ltd**: Unified product shell (TypeScript) over the core API.

## 2.5. Provider Authority

- **Class 3 Peers**: Codex, Claude Code, Gemini CLI, and Antigravity are peer executors over the same Heiwa stack.
- **Provider-Owned Subagents**: Each provider owns its own subagents, reviewers, and delegated execution loops. Routine spawn/message/wait/close flow stays provider-managed.
- **Escalation Boundary**: Interrupt the human operator only for destructive host actions, irreversible external side effects, credential or policy break-glass, or platform/harness prompts that the provider cannot suppress from configuration.
- **Project Auto-Activation**: Repo-local provider config lives in `.codex/`, `.claude/`, and `.gemini/`. Canonical specialist wrappers live in `ops/agents/`, sync into `.gemini/agents/` and `.claude/agents/`, and install into `~/.codex/skills` via `uv run scripts/sync_agents.py --install-codex`.

## 3. Legacy / Reference (`apps/heiwa_hub/`)

- **Python Hub**: Legacy prototype logic. High-value patterns (Spine, Telemetry, Messenger) are being ported to the Rust Core.
- **SpacetimeDB**: Still rooted at `apps/heiwa_hub/spacetimedb/` as the state authority.

## 4. Ground Truth & Progress

- `docs/superpowers/status/feature_list.json`: System capability checklist.
- `docs/superpowers/status/progress.md`: Active work logs.
- `docs/superpowers/specs/2026-04-02-heiwa-rationalization-design.md`: Current architecture specification.

## 5. Security Posture

- **Machine Auth**: Managed via `HEIWA_MACHINE_AUTH_TOKEN`.
- **User Sessions**: Managed via `HEIWA_JWT_SIGNING_SECRET` and `.heiwa.ltd` wildcard cookies.
- **Redaction**: All logs are automatically redacted via centralized Rust primitives.
