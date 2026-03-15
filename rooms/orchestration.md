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

- CLI
- MCP
- HTTP API
- docs

Discord is useful for notification and human-facing query/approval UX, but not as a trust boundary or state authority.

## Approval Posture

- External/public actions require explicit approval.
- Destructive or privileged work should fail closed if approval or lease state is missing.
- Human-facing status should be queryable without pretending that notification surfaces own the truth.

## Current Repo Mapping

- Cell source of truth: `config/identities/profiles.json`
- Materialized cell catalog: `packages/heiwa_sdk/heiwa_sdk/cells.py`
- Release gate: `packages/heiwa_sdk/heiwa_sdk/bench.py`

The goal is to keep orchestration narrow and typed, not to build one monolithic super-agent.
