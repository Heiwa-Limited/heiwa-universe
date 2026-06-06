# Routing Policy

## Current Execution Matrix

- Operator-private todo routing matrix:
  [`docs/strategy/2026-06-06-heiwa-todo-routing-matrix.md`](strategy/2026-06-06-heiwa-todo-routing-matrix.md)
- Machine-readable matrix for future `~/bin/ai route` style dispatch:
  [`config/swarm/heiwa_todo_routing_matrix_v1.json`](../config/swarm/heiwa_todo_routing_matrix_v1.json)

The routing matrix is not a substitute for runtime proof. It assigns first-pass
agent responsibility while repo/runtime truth remains the authority.

## Fast-path principles

- cheapest acceptable route first
- privacy-first for sovereign data
- local-first before remote
- no cloud GPU rental as a default architecture
- no REST-only multi-turn agentic sessions

## Runtime transport

Heiwa is moving toward:

- **SpacetimeDB** for state synchronization
- **WebSockets** for low-latency status/event streaming
- **MCP** for tool interoperability

Polling and compatibility-only surfaces may remain temporarily for fallback or diagnostics, but they are not the target architecture.
