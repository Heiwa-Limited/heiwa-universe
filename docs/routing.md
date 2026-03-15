# Routing Policy

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
