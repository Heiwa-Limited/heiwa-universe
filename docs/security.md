# Security

Heiwa is a local-first operator runtime with a public-safe client. Durable user evidence stays on the owner machine; there is no hosted evidence authority. Public domains are support surfaces only when deployed. Internal preview runtimes such as trading remain isolated until their security posture is proven.

## Public surfaces

- `heiwa.ltd`: marketing only
- `app.heiwa.ltd`: authenticated product shell for keys, runs, approvals, and mission views
- `api.heiwa.ltd`: user/session APIs, runtime health, MCP, and task ingress where deployed
- `status.heiwa.ltd`: read-only operational status
- `docs.heiwa.ltd`: public documentation

## Trust boundaries

### Browser or client -> runtime API

- Public authentication and orchestration requests terminate at `api.heiwa.ltd` where that ingress is deployed.
- Discord and other clients are optional ingress surfaces, not the only identity or execution model.
- Public edge infrastructure provides TLS termination, proxying, and baseline WAF coverage before traffic reaches hosted runtime services.

### App shell -> runtime API

- `app.heiwa.ltd` is a UI surface, not a second control plane.
- The app shell should never hold platform secrets or make privileged decisions locally.
- All user reads and mutations must be enforced by the installed or hosted runtime with tenant-scoped authorization.

### Runtime API -> local evidence plane

- Versioned JSONL under `~/.heiwa/evidence/` is canonical execution evidence; SQLite holds bounded hot state and Lance is a rebuildable recall index.
- Raw journal streams never sync to GitHub until explicit redaction, privacy, conflict, and promotion rules exist.
- User boundaries remain explicit throughout routes, state, receipts, and any future sync projection.
- User auth and operator auth remain separate so operator tooling does not inherit user-facing trust.

### Runtime API -> provider APIs

- BYOK credentials are tenant-scoped assets. They must never be shared across users, leaked through logs, or reused outside the owning tenant's execution path.
- The target posture is encrypted-at-rest credential storage with just-in-time decryption at execution. The current hardening backlog still includes encrypting stored Discord OAuth tokens and removing the remaining string-built queries.
- Routing decisions must respect each user's own budget, rate limits, and provider inventory rather than a shared global pool.

### Runtime API -> execution runtimes

- Hosted runtime APIs should act as control planes, not long-lived execution planes for arbitrary user workloads.
- Public launch paths should stay limited to curated missions and curated tools until stronger sandboxing exists.
- Any future arbitrary execution path should move through isolated workers or sandboxes rather than shared in-process execution.

## Identity and authorization

- Discord can identify users for sign-in and DM correlation where that ingress is enabled.
- User auth uses short-lived runtime-issued JWTs for dashboard reads and mission views.
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
