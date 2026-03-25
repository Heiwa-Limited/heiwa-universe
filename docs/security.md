# Security

Heiwa is a multi-tenant BYOK orchestration platform. The platform boundary is intentionally narrow: public users see `heiwa.ltd`, `app.heiwa.ltd`, `status.heiwa.ltd`, `docs.heiwa.ltd`, and the hub control plane at `api.heiwa.ltd`. Internal preview runtimes such as trading can remain on separate Railway services, but they are not part of the supported public surface until their isolation and operational posture are ready.

## Public surfaces

- `heiwa.ltd`: marketing only
- `app.heiwa.ltd`: authenticated product shell for keys, runs, and mission views
- `api.heiwa.ltd`: Discord OAuth entry/callback, user/session APIs, hub health, MCP, and task ingress
- `status.heiwa.ltd`: read-only operational status
- `docs.heiwa.ltd`: public documentation

## Trust boundaries

### Browser or Discord client -> Hub API

- All public authentication and orchestration requests terminate at `api.heiwa.ltd`.
- Discord OAuth identifies the user; hub-issued JWTs identify subsequent web API calls.
- Cloudflare provides TLS termination, proxying, and baseline WAF coverage before traffic reaches Railway.

### App shell -> Hub API

- `app.heiwa.ltd` is a UI surface, not a second control plane.
- The app shell should never hold platform secrets or make privileged decisions locally.
- All user reads and mutations must be enforced at the Hub with tenant-scoped authorization.

### Hub API -> SpacetimeDB

- SpacetimeDB is the authoritative ledger for users, OAuth identities, provider credentials, runs, routes, missions, and billing events.
- Multi-tenant data separation is enforced by `user_id` scoping throughout the route, state, database, and STDB layers.
- User auth and operator auth remain separate so operator tooling does not inherit user-facing trust.

### Hub API -> provider APIs

- BYOK credentials are tenant-scoped assets. They must never be shared across users, leaked through logs, or reused outside the owning tenant's execution path.
- The target posture is encrypted-at-rest credential storage with just-in-time decryption at execution. The current hardening backlog still includes encrypting stored Discord OAuth tokens and removing the remaining string-built queries.
- Routing decisions must respect each user's own budget, rate limits, and provider inventory rather than a shared global pool.

### Hub API -> execution runtimes

- The Hub should act as the control plane, not the long-lived execution plane for arbitrary user workloads.
- Public launch paths should stay limited to curated missions and curated tools until stronger sandboxing exists.
- Any future arbitrary execution path should move through isolated workers or sandboxes rather than shared in-process execution.

## Identity and authorization

- Discord is the user identity provider for sign-in and DM correlation.
- User auth uses short-lived hub-issued JWTs for dashboard reads and mission views.
- operator auth stays separate for admin routes, MCP internals, and privileged controls.
- Read-only public surfaces must not expose write-capable operator actions.

## Assets that matter

- BYOK credentials and OAuth tokens
- cross-tenant mission, run, and artifact data
- billing events and usage attribution
- routing logic and approval state
- operator tokens and internal provider credentials

## Current guardrails

- fail closed on missing secrets and identities
- redact transport and provider credentials in logs
- keep marketing, docs, and status separate from privileged runtime state
- prefer canonical domains over direct provider URLs in the public shell
- keep internal preview runtimes off the supported public host map

## Near-term hardening backlog

- verify JWT `aud` in addition to signature, issuer, and expiry
- encrypt stored Discord OAuth tokens and any other persisted tenant credentials
- replace remaining string-built database queries with parameterized queries
- bring WebSocket/event auth onto the same tenant-scoped model as HTTP routes
- move execution isolation toward worker or sandbox boundaries before exposing broader public automation

## Non-goals for public docs

This doc set does not claim that Discord or experimental canvases are the only product interfaces, and it does not claim that internal preview services are ready public surfaces.
