# Heiwa Fluid Mesh: The AI-Dentity Architecture

## 1. Overview

The goal of this overhaul is to transform Heiwa into a 24/7 "AI-Dentity" running natively on Railway and boost nodes. It removes structural "classism" between models (Class 3 vs Free/Local) and establishes an Omnidirectional Fluid Mesh. In this mesh, any model (Claude, Gemini, Codex, Ollama, Antigravity) is a peer `MeshAgent` that can dynamically spawn sub-tasks and delegate to any other model in the provider matrix based on optimal strengths and token efficiency.

## 2. Architecture

### 2.1 The Single Endpoint (Identity Router)

- **Component**: `HeiwaAgent` (Captain).
- **Location**: Always-on Railway process.
- **Role**: The primary persona. It provides a single ingress point via Discord DMs and the local CLI REPL.
- **Routing**: Upon receiving user intent, it normalizes the request and routes it to the _most optimal starting model_ defined in `config/swarm/ai_router.json`, effectively handing off the "steering wheel" to that model.

### 2.2 The Omnidirectional Swarm (Peer Agents)

- **Concept**: We discard the strict "Class 3 Planners vs. Execution Swarm" divide.
- **Packaging**: Claude, Gemini, Codex, Ollama, and Antigravity are each packaged as standalone `MeshAgent` instances connected to the Hub via WebSockets.
- **Native Capabilities**: Models retain their native tooling (e.g., Claude's MCP tools, Gemini's Superpowers).
- **Fluid Handoff**: Any `MeshAgent` can spawn a sub-task for another `MeshAgent`. If Claude needs deep visual analysis, it queries the mesh matrix and dispatches a sub-task specifically to Gemini. If Gemini needs to parse 10k lines of logs cheaply, it dispatches to Ollama.
- **The Orchestration Service**: A unified `OrchestrationService` in the `heiwa_sdk` handles these dynamic sub-agent dispatch requests, replacing the fragmented logic in `SpineAgent` and `mcp_server.py`.

### 2.3 SpacetimeDB State Layer (The Shared Brain)

- **Role**: SpacetimeDB is the single source of truth for the entire mesh.
- **Missions & Tasks**: When an agent spawns a sub-agent, it creates a `Mission` record in STDB. The target sub-agent claims it, reads context, and writes the result back.
- **Telemetry Migration**: `TelemetryAgent`'s in-memory usage cache is migrated entirely to STDB to survive container restarts and provide mesh-wide rate limit awareness.

## 3. Consolidation & Cleanup

- **Repo Unification**: `heiwa-core`, `heiwa-spacetime`, and `heiwa-limited-repo` are archived or merged into `~/heiwa` to enforce the "One Logical Identity" rule.
- **SQLite Purge**: All legacy SQLite fallback code is removed from `packages/heiwa_sdk/heiwa_sdk/db.py` to silence production migration warnings and enforce STDB sovereignty.
- **Security Hardening**: `HEIWA_AUTH_TOKEN` manual checks are replaced with a centralized `SecurityService` in the SDK to prevent token leakage.

## 4. Execution Flow

1. User DMs Heiwa on Discord.
2. `HeiwaAgent` (Captain) receives it, scores risk, and consults `ai_router.json`.
3. Captain assigns the primary `Mission` to **Claude**.
4. Claude begins execution, writes a plan to STDB, and realizes it needs to scrape a massive website.
5. Claude uses its sub-agent dispatch tool to create a sub-task targeting **Antigravity**.
6. The Hub's `DeliveryManager` routes the sub-task to the Antigravity `MeshAgent`.
7. Antigravity completes the scrape, writes the summary to STDB.
8. Claude reads the summary, finishes the logic, and reports back to Captain.
9. Captain summarizes the final result back to the user on Discord.
