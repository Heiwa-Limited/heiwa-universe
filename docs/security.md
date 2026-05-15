# Security

Heiwa is a local-first operator runtime with a public-safe client and hosted state/evidence backbone. The platform boundary is intentionally narrow: public users see `heiwa.ltd`, `app.heiwa.ltd`, `status.heiwa.ltd`, `docs.heiwa.ltd`, and public API/status ingress at `api.heiwa.ltd` where deployed. Internal preview runtimes such as trading can remain on separate transitional services, but they are not part of the supported public surface until their isolation and operational posture are ready.

## Public surfaces

- `heiwa.ltd`: marketing only
- `app.heiwa.ltd`: safe companion client for provider/account state, runs, approvals, and mission views
- `api.heiwa.ltd`: public HTTP/MCP/status ingress where deployed
- `status.heiwa.ltd`: read-only operational status
- `docs.heiwa.ltd`: public documentation

## Trust boundaries

### Browser or messaging client -> API/runtime ingress

- Public authentication and orchestration requests terminate at `api.heiwa.ltd` where that ingress is deployed.
- Discord and iMessage are optional clients, not the only identity or execution model.
- Cloudflare provides TLS termination, proxying, and baseline WAF coverage before traffic reaches the deployed API/runtime.

### App shell -> API/runtime boundary

- `app.heiwa.ltd` is a UI surface, not a second control plane.
- The app shell should never hold platform secrets or make privileged decisions locally.
- User reads and mutations must be enforced by the runtime/API boundary and canonical state reducers, not by browser-local logic.

### API/runtime boundary -> SpacetimeDB

- SpacetimeDB is the authoritative ledger for users, OAuth identity metadata, credential references/status, runs, routes, missions, and billing events.
- Multi-tenant data separation is enforced by `user_id` scoping throughout the route, state, database, and STDB layers.
- User auth and operator auth remain separate so operator tooling does not inherit user-facing trust.

### API/runtime boundary -> provider APIs

- BYOK credentials are tenant-scoped assets. They must never be shared across users, leaked through logs, or reused outside the owning tenant's execution path.
- The target posture is encrypted-at-rest credential storage with just-in-time decryption at execution. The current hardening backlog still includes encrypting stored Discord OAuth tokens and removing the remaining string-built queries.
- Routing decisions must respect each user's own budget, rate limits, and provider inventory rather than a shared global pool.

### API/runtime boundary -> execution runtimes

- Hosted ingress should act as a control/evidence plane, not the long-lived execution plane for arbitrary user workloads.
- Public launch paths should stay limited to curated missions and curated tools until stronger sandboxing exists.
- Any future arbitrary execution path should move through isolated workers or sandboxes rather than shared in-process execution.

## Identity and authorization

- Discord can identify users for sign-in and DM correlation where that ingress is enabled.
- User auth uses short-lived runtime/API-issued JWTs for dashboard reads and mission views where public auth is enabled.
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
