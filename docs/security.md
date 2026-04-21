# Security

Heiwa is a local-first AI runtime with evolving hosted and state-backed surfaces. The security boundary should describe the installed runtime, the backend/state authority where it exists, and the release surfaces that are actually live today.

## Active surfaces

- installed `heiwa` runtime
- repo source and CI on GitHub
- published docs from MkDocs/GitHub Pages
- state/evidence services where the current runtime still depends on SpacetimeDB

## Trust boundaries

### Operator surface -> runtime

- The installed runtime should not expose platform secrets through local logs, transcripts, or environment leakage.
- Provider credentials and tokens must remain scoped to the owning user or machine context.
- Presentation layers must not be treated as the authority for privileged execution decisions.

### Runtime -> state plane

- SpacetimeDB is the authoritative ledger for users, OAuth identities, provider credentials, runs, routes, missions, and billing events.
- Multi-tenant data separation is enforced by `user_id` scoping throughout the route, state, database, and STDB layers.
- User auth and operator auth remain separate so operator tooling does not inherit user-facing trust.

### Runtime -> provider APIs

- BYOK credentials are tenant-scoped assets. They must never be shared across users, leaked through logs, or reused outside the owning tenant's execution path.
- The target posture is encrypted-at-rest credential storage with just-in-time decryption at execution. The current hardening backlog still includes encrypting stored Discord OAuth tokens and removing the remaining string-built queries.
- Routing decisions must respect each user's own budget, rate limits, and provider inventory rather than a shared global pool.

### Runtime -> execution surfaces

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
- keep docs and release surfaces separate from privileged runtime state
- do not overstate hosted or preview surfaces as the primary product contract
- keep internal preview/runtime experiments off the supported public story until verified

## Near-term hardening backlog

- verify JWT `aud` in addition to signature, issuer, and expiry
- encrypt stored Discord OAuth tokens and any other persisted tenant credentials
- replace remaining string-built database queries with parameterized queries
- bring WebSocket/event auth onto the same tenant-scoped model as HTTP routes
- move execution isolation toward worker or sandbox boundaries before exposing broader public automation

## Non-goals for public docs

This doc set does not claim that legacy hosted paths are the current product center, and it does not claim that experimental surfaces are ready public interfaces.
