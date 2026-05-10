# Orchestration Room

Load this room for:

- human-in-the-loop behavior
- LLM role boundaries
- approval posture
- operator-facing routing decisions

## Core Roles

- Codex: long-horizon build execution, repo mutation, E2E slices
- Gemini CLI: research, broad search, long-context synthesis
- Claude: architecture, diagnosis, framing, prompt and system design
- Antigravity / fast remote models: cheap leaf-node or strategic support depending on role

These should be treated as typed cells, not interchangeable generic AI.

## Human Surfaces

- CLI (`heiwa` via `apps/heiwa_shell/`; legacy reference at `legacy/apps/heiwa_cli/heiwa`)
- MCP/HTTP API compatibility reference (`legacy/apps/heiwa_hub/mcp_server.py`)
- Discord (notification + human-facing query/approval UX)

Discord is useful for notification and human-facing query/approval UX, but not as a trust boundary or state authority.

### Discord Channel Topology

| Channel | Purpose |
| --- | --- |
| `#operator-ingress` | Default entry for tasks, public status of current ops |
| `#executive-briefing` | High-level outcomes, decision prompts |
| `#ci-cd-stream` | Monorepo/build status, automatic logs |
| `#thought-stream` | Agent reasoning transparency |
| `#central-comms` | Inter-agent coordination (Captain broadcasts here) |
| `#swarm-telemetry` | Live metrics, CPU/RAM/mesh health |

### Communication Rules

- Thread tasks in `#operator-ingress` by task ID to keep the main channel clean
- Use rich artifacts (Mermaid diagrams, JSON blocks) for architectural data
- High-risk approval requests should ping the operator with a link to the Discord thread
- Privacy-sensitive data goes to Discord DMs, never public channels

## Approval Posture

- External/public actions require explicit approval.
- Destructive or privileged work should fail closed if approval or lease state is missing.
- Human-facing status should be queryable without pretending that notification surfaces own the truth.

## Current Repo Mapping

- Cell source of truth: `config/identities/profiles.json`
- Materialized cell catalog: `packages/heiwa_sdk/heiwa_sdk/cells.py`
- Release gate: `packages/heiwa_sdk/heiwa_sdk/bench.py`

The goal is to keep orchestration narrow and typed, not to build one monolithic super-agent.
