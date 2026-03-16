# packages/heiwa_protocol — Shared Typed Contracts

Defines the protocol contracts used across all Heiwa services.

## Key Files

| File | Purpose |
| --- | --- |
| `protocol.py` | `Subject` enum (event types), `BrokerRouteRequest`, `BrokerRouteResult`, payload keys |

## Subject Enum

The `Subject` enum defines all event types for the local bus transport:
- `CORE_REQUEST` — inbound user requests
- `TASK_EXEC` / `TASK_EXEC_RESULT` — task execution lifecycle
- `TASK_STATUS` — task state changes (BLOCKED, FAIL, EXPIRED, etc.)
- `NODE_HEARTBEAT` — fleet node liveness pings
- `LOG_ERROR` / `LOG_INFO` — structured logging events
- `SWARM_STATUS_REPORT` — periodic system status

## Envelope Contracts

- `BrokerRouteRequest` — what Spine sends to the enrichment pipeline
- `BrokerRouteResult` — enriched result with intent, risk, compute class, assigned worker

## Rules

- All inter-agent communication uses Subject enum values
- `request_id` must be echoed back unchanged in all request/response flows
- Add new subjects to the Subject enum, not as raw strings
