# infra/ — Infrastructure Configs

Deployment and node configuration for the Heiwa mesh.

## Subdirectories

| Directory | Purpose |
| --- | --- |
| `cloud/` | Cloud provider configs (Railway) |
| `local/` | Local development configs |
| `nodes/` | Per-node setup guides and scripts |

## Node Topology

| Node | Hardware | Role | Status |
| --- | --- | --- | --- |
| Railway (heiwa-cloud-hq) | Cloud container | Always-on control plane, CLI tools, STDB | Primary |
| MacBook M4 Pro (node-a) | 24GB RAM, M4 GPU | Orchestrator, Ollama, human-in-loop | Boost (optional) |
| WSL/RTX 3060 (node-b) | 32GB RAM, 12GB VRAM | GPU worker, media gen, embeddings | Boost (optional) |

## Key Environment Variables

| Var | Purpose |
| --- | --- |
| `HEIWA_STATE_BACKEND` | `spacetimedb` (Railway) or `sqlite` (local dev) |
| `HEIWA_AUTH_TOKEN` | Hub auth token |
| `HEIWA_NODE_ID` | Node identifier for fleet registration |
| `PORT` | HTTP server port (default 8080) |
| `GEMINI_API_KEY` | Google AI Studio free tier (for Captain reasoning) |

## Notes

- NATS has been removed. Transport is LocalBusTransport (co-located) + WebSocket (remote)
- Railway is the primary execution plane — boost nodes are optional
- All CLI tools (Claude Code, Gemini CLI, Codex) are installed in the Docker image
