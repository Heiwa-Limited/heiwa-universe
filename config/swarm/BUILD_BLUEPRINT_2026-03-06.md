# Heiwa Build Blueprint Sync

Date: 2026-05-22 local-first reset. Original March report is historical only.

## Non-Negotiables

- Current source-of-truth/server: Devon's MacBook checkout plus `~/.heiwa/`.
- State: local files/SQLite are current owner truth; SpacetimeDB is sync/adjudication when enabled.
- Transport: localhost HTTP/WebSocket for the cockpit, plus future worker WebSockets.
- Canonical monorepo: `/Users/dmcgregsauce/heiwa-universe`.
- Local-first execution for private or sovereign tasks.
- No dedicated cloud GPU rental.
- No architecture that requires an external message broker for core ingress or durable state.
- MCP Tool Shed: agent-tool connectivity routes through MCP where applicable.
- Mandatory finalizers: every autonomous service execution must include resource cleanup.
- E2B sandboxes for untrusted code execution. LLM-generated code never runs on host infrastructure.
- Hardware constraints documented in `config/swarm/HARDWARE_CONSTRAINTS.md`.

## Hardware Topology

- Node A: MacBook M4 Pro, 24 GB unified memory.
  Role: owner runtime, CLI terminal, local server, primary reasoning surface, local model host.
- Node B: Ryzen 7 7700X, RTX 3060 12 GB VRAM, 32 GB RAM.
  Role: future headless GPU worker for embeddings, reranking, browser automation, media generation, local GPU inference.
- Public edge: Cloudflare later, only after local user functionality is reliable.

## Four-Class Execution Model

- Class 1 CPU-first: shell execution, file operations, Git operations, parsing, assembly, linting, audit checks.
- Class 2 GPU-justified: local LLM inference, embeddings, vector reranking, image generation, audio processing.
- Class 3 Premium Remote: complex reasoning, hard debugging, strategy planning, adversarial review, long-context work.
- Class 4 Remote Support: Cloudflare Workers, status APIs, notifications, GitHub distribution, optional public edge.

## Routing Rules

- Privacy-first gate: if `privacy_level = sovereign`, force Class 1 or 2 only.
- Cost gate: if the budget is near exhaustion, downgrade or queue Class 3 work.
- Provider order: cheapest acceptable route first. Prefer local tooling, then local models, then subscription CLIs, then metered APIs only when justified.
- Cloud inference overflow: use provider APIs/subscriptions, not rented cloud GPUs.
