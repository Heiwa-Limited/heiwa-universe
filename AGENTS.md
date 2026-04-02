# AGENTS.md — The Heiwa Swarm Map

Heiwa is an omnidirectional fluid mesh of peer agents. Any agent can spawn any other agent or tool via the Hub.

## 1. Core Agent Roster (`apps/heiwa_hub/agents/`)

- **Captain** (`heiwa_agent.py`): The 24/7 identity. Handles Discord DMs and entry routing.
- **Spine** (`spine.py`): Fleet coordination, heartbeats, and node registry.
- **Messenger** (`messenger.py`): Discord server integration and human-in-the-loop approvals.
- **Telemetry** (`telemetry.py`): Swarm-wide usage tracking and rate-limit awareness (STDB-native).
- **Executor** (`executor.py`): Local tool and shell execution surface.

## 2. Mesh Connectivity (`packages/heiwa_sdk/`)

- **SecurityService**: Centralized auth token validation and secret redaction.
- **OrchestrationService**: (In Progress) Centralized enrichment and handoff logic.
- **DeliveryManager**: (In Progress) Unified routing for LocalBus and WebSocket task delivery.

## 3. Heiwa Limbs (`apps/heiwa_limbs/`)

Stand-alone processes in non-Python languages (e.g., Rust) that connect to the mesh via SpacetimeDB or WebSockets.

## 3.5 Architecture Direction (April 2026)

- Production target stack is **Rust + TypeScript + Shell**.
- **Rust** owns the authoritative state layer, orchestration, routing, and future DREX execution logic.
- **TypeScript** owns operator-facing web surfaces and typed client contracts.
- **Shell** remains the bootstrap and operator glue layer for Railway, Linux, and WSL execution.
- The Python Hub and cognition packages are still live in the repo, but they are prototype and compatibility surfaces, not the long-term control plane.

## 4. Ground Truth & Progress

- `docs/superpowers/status/feature_list.json`: System capability checklist.
- `docs/superpowers/status/progress.md`: Active work logs.
- Canonical operator context now lives under `ops/context/` and `ops/rooms/`.

## 5. Security Posture

Agents must never access `HEIWA_AUTH_TOKEN` directly. Use `SecurityService().validate_token()`. All logs are automatically redacted via `redact_text`.
