# apps/heiwa_hub — Hub Runtime (Railway Control Plane)

The always-on control plane hosted on Railway. Boots all agents, serves MCP/HTTP, manages the execution pipeline.

## Key Files

| File | Purpose |
| --- | --- |
| `main.py` | Hub entrypoint — boots agents + uvicorn server |
| `start.sh` | Railway entrypoint — Tailscale, Ollama, PYTHONPATH, then `main.py` |
| `mcp_server.py` | FastAPI app — MCP/HTTP/WebSocket API surface |
| `Dockerfile` | Multi-stage Railway build (Python + Node.js + CLI tools) |
| `envelope.py` | Token/payload extraction for auth |

## Subdirectories

| Directory | Purpose | Context |
| --- | --- | --- |
| `agents/` | Runtime agents (Spine, Executor, Captain, Telemetry, Messenger) | See `agents/CONTEXT.md` |
| `cognition/` | Intent/risk/compute pipeline | See `cognition/CONTEXT.md` |
| `tests/` | pytest test suite | |
| `actions/` | One-shot action scripts (smoke tests) | |
| `heiwaproductiondb/` | SpacetimeDB Rust module source | |

## Rules

- All agents extend `BaseAgent` from `agents/base.py`
- Transport is `LocalBusTransport` (in-process pub/sub) — no NATS
- State backend is SpacetimeDB (set via `HEIWA_STATE_BACKEND` env var)
- Messenger is optional — only boots when `DISCORD_TOKEN` is present
- Railway deploys on push to main via `.github/workflows/deploy.yml`
