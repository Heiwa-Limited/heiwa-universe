# packages/heiwa_sdk — Core SDK

The shared library powering state, routing, security, and execution gateway.

## Key Modules

| File | Purpose |
| --- | --- |
| `db.py` | Stateless compatibility facade; authoritative state remains in Rust |
| `heiwaclaw.py` | Execution gateway — resolves BrokerRouteResult → HeiwaClawDispatch (tool, adapter, provider, transport) |
| `tool_mesh.py` | Executes selected adapter with environment (heiwa_ops, heiwa_reflex) |
| `routing.py` | Compute routing logic |
| `security.py` | Auth validation, token handling, redaction |
| `config.py` | Environment loading (`load_swarm_env()`) |
| `transport.py` | LocalBusTransport (in-process pub/sub) + WebSocket transport for remote workers |
| `tick.py` | Maintenance tick (alerts, RFC publishing, persistence) |
| `claw_adapter.py` | DEPRECATED — OpenClaw wrapper, do not extend |

## State Layer

- Rust plus local JSONL own authoritative state.
- Lance is a derived, rebuildable recall index.
- Python services may receive narrow injected compatibility backends, but do
  not discover, launch, or claim authority over state services.

## Rules

- Prefer typed contracts from `heiwa_protocol`
- Route execution through HeiwaClaw/MCP, not ad-hoc provider calls
- Keep public API surfaces honest — back them with tests
- Route state mutations through Rust runtime service layers.
