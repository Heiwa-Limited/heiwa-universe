# Railway Infrastructure

Heiwa's primary execution plane on Railway.

## Components
1. **heiwa-core** (Main Rust runtime authority)
2. **heiwa-trading** (Optional trading surface)
3. **SpacetimeDB** (Authoritative state — routes, tasks, runs, nodes, leases, approvals)

## Bootstrapping a New Environment

```bash
railway init --name heiwa-universe

# Set Variables for Core
railway variables --set 'HEIWA_STATE_BACKEND=spacetimedb' --service heiwa-core
railway variables --set 'STDB_SERVER=maincloud' --service heiwa-core
railway variables --set 'PORT=8080' --service heiwa-core

# Link custom domain
railway domain link api.heiwa.ltd --service heiwa-core
```

## Volumes & Persistence

- `runtime/spool/` is bound to a Railway volume for durable dead-letter spooling
- SpacetimeDB handles all persistent state — no separate Postgres required

## Transport

- WebSocket worker ingress at `/ws/worker`
- Legacy compatibility bridge at `/ws/worker/legacy`
- No external message brokers

## Runtime Baseline

- Docker builder pin: `rust:1.93-slim`
- Healthcheck: `/ready`
- Start command: `bash apps/heiwa_core/start.sh`
- Production default: remote STDB (`maincloud`)
