# Railway Infrastructure

Heiwa's primary execution plane on Railway.

## Components
1. **heiwa-hub** (Main Python worker & server)
2. **SpacetimeDB** (Authoritative state — proposals, nodes, runs, leases, approvals)

## Bootstrapping a New Environment

```bash
railway init --name heiwa_hub

# Set Variables for Hub
railway variables --set 'HEIWA_STATE_BACKEND=spacetimedb' --service heiwa-hub
railway variables --set 'PORT=8080' --service heiwa-hub

# Link custom domain
railway domain link api.heiwa.ltd --service heiwa-hub
```

## Volumes & Persistence

- `runtime/spool/` is bound to a Railway volume for durable dead-letter spooling
- SpacetimeDB handles all persistent state — no separate Postgres required

## Transport

- In-process `LocalBusTransport` for co-located agents
- WebSocket for remote boost node connections
- No external message brokers
