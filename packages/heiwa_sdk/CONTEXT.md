# packages/heiwa_sdk — Core SDK

The shared library powering state, routing, security, and execution gateway.

## Key Modules

| File | Purpose |
| --- | --- |
| `db.py` | Multi-backend DB abstraction (SpacetimeDB primary, legacy SQLite/Postgres being removed) |
| `spacetimedb.py` | Native SpacetimeDB CLI bridge |
| `heiwaclaw.py` | Execution gateway — resolves BrokerRouteResult → HeiwaClawDispatch (tool, adapter, provider, transport) |
| `tool_mesh.py` | Executes selected adapter with environment (heiwa_ops, heiwa_reflex) |
| `routing.py` | Compute routing logic |
| `security.py` | Auth validation, token handling, redaction |
| `config.py` | Environment loading (`load_swarm_env()`) |
| `transport.py` | LocalBusTransport (in-process pub/sub) + WebSocket transport for remote workers |
| `tick.py` | Maintenance tick (alerts, RFC publishing, persistence) |
| `claw_adapter.py` | DEPRECATED — OpenClaw wrapper, do not extend |

## State Layer

- SpacetimeDB is authoritative (`HEIWA_STATE_BACKEND=spacetimedb`)
- Tables: proposals, nodes, runs, capability_leases, approval_requests, approval_decisions
- Legacy SQLite/Postgres code in db.py is scheduled for removal
- All state writes should go through SpacetimeDB first

## Rules

- Prefer typed contracts from `heiwa_protocol`
- Route execution through HeiwaClaw/MCP, not ad-hoc provider calls
- Keep public API surfaces honest — back them with tests
- STDB-backed service layers over direct DB access
