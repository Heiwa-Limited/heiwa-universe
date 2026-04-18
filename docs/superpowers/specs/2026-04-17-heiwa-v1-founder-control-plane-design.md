# Design: Heiwa v1 Founder Control Plane

## Status: Draft (2026-04-17)

## 1. One-Sentence Truth

Heiwa v1 is secure personal AI gateway for founders and operators: `SvelteKit` owns human session UX, `WorkOS` owns human identity, `Rust` owns control/gateway/policy execution, `SpacetimeDB` remains canonical state and audit plane, `Infisical` owns system-secret distribution, and machine workers authenticate separately from human browser sessions.

## 2. Product Scope

### In

- founder/solo-operator product surface
- passkeys-first human auth
- machine-enrolled execution nodes
- provider credential vault
- policy-based model routing across local + cloud
- request metering and append-only audit trail
- one paid workflow: `Operator Research + Proposal Engine`

### Out

- team SAML / SCIM
- broad agent marketplace
- generalized chat app
- mobile app
- new core auth or policy logic in Python
- framework fork to `Next.js`
- `Postgres` as second authority plane

## 3. Stack Call

| Layer | Decision | Reason |
| --- | --- | --- |
| Web app | `SvelteKit` | Matches current web migration direction and avoids React/Next split-brain. |
| Human auth | `WorkOS AuthKit` | Official SvelteKit SDK, passkeys, hosted auth, enterprise slope. |
| Runtime authority | `SpacetimeDB` | Already canonical Heiwa authority/evidence plane. |
| Control/gateway/policy | `Rust` | Best fit for token verification, provider adapters, routing, worker registration, and STDB integration. |
| System secrets | `Infisical` | Better machine identity and audit model than config-only secret distribution. |
| Operator-local secrets | macOS Keychain | Existing Heiwa path on MacBook already present. |
| Compatibility layer | Python | Freeze core expansion; maintain only where migration not yet landed. |

## 4. Trust Boundary Model

### 4.1 MacBook

MacBook is trust anchor and operator console.

Use it for:

- privileged admin session
- provider credential approval
- policy and routing edits
- billing/admin review
- audit review
- local dev and high-trust operator work

Store only operator-local secrets in macOS Keychain. Do not treat browser session as machine credential.

### 4.2 PC

PC is machine-enrolled execution node.

Use it for:

- local models
- heavy apps
- browser automation
- test workers
- future scheduled execution

PC authenticates as machine identity. It does not borrow human browser cookie, JWT, or OAuth session.

### 4.3 22" Monitor

Telemetry plane only:

- queue
- logs
- usage
- Discord
- SOPs/docs

## 5. Authority Map

| System | Owns |
| --- | --- |
| `WorkOS` | Human identity, passkeys, OIDC, auth source-of-truth |
| `SvelteKit` | Auth callback handling, secure human session cookie, dashboard/admin UX |
| `Rust` | Machine registration, token mint/verify, provider adapters, routing, policy enforcement |
| `SpacetimeDB` | Canonical Heiwa state, audit records, routing records, worker records, usage records |
| `Infisical` | System secrets, machine identity auth into secret plane, secret access audit |
| macOS Keychain | Operator-local secrets on MacBook |

Rule: one concern, one authority. No dual-write auth truth. No browser-visible privileged tokens.

## 6. App Boundary

Browser talks only to `SvelteKit`.

`SvelteKit` server routes act as narrow BFF:

- enforce cookie/session checks
- enforce CSRF/origin checks
- SSR-gate authenticated routes
- proxy only needed server actions to Rust control services

`Rust` is not casual browser-facing surface. Public browser traffic should not hit Rust control endpoints directly unless future design explicitly calls for it.

## 7. Human Auth Model

### 7.1 Flow

1. User hits SvelteKit app.
2. SvelteKit redirects to `WorkOS AuthKit`.
3. User signs in with passkey or allowed OIDC path.
4. WorkOS returns authenticated identity to SvelteKit server callback.
5. SvelteKit creates server-issued session cookie.
6. SvelteKit mirrors needed identity/workspace metadata into STDB via Rust control API.
7. Browser receives only secure session cookie, never privileged internal token.

### 7.2 Session Shape

- idle timeout: `60 minutes`
- absolute lifetime: `12 hours`
- reauth threshold for sensitive actions: `15 minutes`
- cookie flags: `HttpOnly`, `Secure`, `SameSite=Lax`
- browser mutations use SvelteKit form actions or server endpoints only
- state-changing routes enforce `Origin` / `Referer`
- non-form JSON mutations use server-issued CSRF token or equivalent double-submit control

### 7.3 Rules

- passkeys first
- custom auth domain configured before production passkey rollout
- short-lived session, server validated
- step-up auth for destructive admin actions
- no fragment token handoff
- no localStorage session authority

### 7.4 Current Repo Correction

Current Discord OAuth + fragment JWT path in [apps/heiwa_hub/auth.py](../../../apps/heiwa_hub/auth.py) and [ops/rooms/web.md](../../../ops/rooms/web.md) is legacy. v1 replaces it with SvelteKit server callback plus cookie session.

## 8. Machine Auth Model

### 8.1 Goal

Separate machine trust from human trust.

### 8.2 Flow

1. Operator initiates machine enrollment from trusted admin session on MacBook.
2. Rust control plane creates bootstrap enrollment record in STDB and one-time enrollment secret reference.
3. Machine authenticates to Infisical machine identity path and retrieves minimum bootstrap material.
4. Machine calls `POST /machines/register`.
5. Rust verifies bootstrap material, device metadata, and requested capabilities.
6. STDB writes machine identity + worker registration + capability set.
7. Rust mints short-lived service token for worker runtime.
8. Worker uses rotating short-lived token for heartbeat and execution claims.

### 8.3 Enrollment Rules

- unique machine identity = generated device keypair + stable machine UUID + operator label
- bootstrap enrollment expires in `15 minutes`
- founder v1 default active machine cap: `5`
- revocation immediately invalidates active worker tokens
- lost/stolen flow: revoke machine, rotate related bootstrap material, emit security event
- capability approval is template-based at enrollment; later heartbeat refreshes must stay inside approved template or require fresh approval

### 8.4 Rules

- no reuse of human session cookie
- no long-lived bearer token pinned in shell profile
- worker token short TTL
- machine token scoped to worker endpoints only
- capability set recorded and auditable

## 9. Secrets Model

### 9.1 Secret Classes

#### A. Operator-local

Stored in macOS Keychain:

- local recovery material
- local dev-only secrets
- operator bootstrap material where device-bound storage is acceptable

#### B. System secrets

Stored in Infisical:

- WorkOS secrets
- STDB service secrets
- webhook secrets
- internal service credentials
- signing keys not tied to local-only operator use

#### C. Tenant/provider credentials

Stored in Heiwa vault path:

- OpenAI keys
- Anthropic keys
- Google keys
- OpenRouter keys
- provider-specific BYOK material

Credential metadata and access events live in STDB. Encrypted secret material is controlled by Rust vault service, not browser and not ad hoc `.env` files.

### 9.2 Ciphertext Decision

v1 stores encrypted credential material in `STDB` with explicit envelope versioning. Metadata, access audit, and version linkage also stay in `STDB`.

Implementation rule:

- keep ciphertext behind narrow Rust vault interface
- do not let callers depend on raw storage shape
- preserve future migration path to external ciphertext store without API rewrite

### 9.3 Rules

- no provider keys in frontend
- no new `.env` sprawl as architecture
- no child process gets vault master material by default
- every credential read/use/rotate emits audit event

## 10. STDB Canonical Schema

### 10.1 Identity Mirror

- `users`
- `workspaces`
- `workspace_memberships`
- `roles`
- `permissions`
- `oidc_identities`

WorkOS stays source-of-truth for human auth. STDB stores Heiwa-local mirror needed for routing, audit, and policy.

### 10.2 Machine/Auth

- `machine_identities`
- `machine_enrollments`
- `worker_registrations`
- `service_tokens`
- `auth_events`
- `session_records`

### 10.3 Vault

- `providers`
- `provider_credentials`
- `credential_versions`
- `credential_access_events`

### 10.4 Routing/Policy

- `model_endpoints`
- `routing_policies`
- `cost_policies`
- `tool_policies`
- `request_policies`
- `policy_decision_records`

### 10.5 Usage/Audit

- `requests`
- `request_spans`
- `billing_events`
- `audit_events`
- `security_events`

### 10.6 Workflow Assets

- `documents`
- `document_chunks`
- `prompt_assets`
- `workflow_templates`
- `workflow_runs`

## 11. RBAC v1

Keep roles small:

- `owner`
- `admin`
- `operator`
- `worker`

Keep permission domains small:

- `credentials`
- `routing_policy`
- `machines`
- `workflows`
- `audit`
- `billing`

No giant custom permission lattice in founder v1.

## 12. API Surface

### 12.1 Public App API

- `GET /me`
- `POST /workspaces`
- `GET /workspaces/:id`
- `POST /provider-credentials`
- `POST /chat/completions`
- `GET /usage`
- `GET /audit-events`

### 12.2 Machine/Internal Control API

- `POST /machines/register`
- `POST /machines/token`
- `POST /workers/heartbeat`
- `POST /workers/execute`
- `POST /routing/evaluate`

### 12.3 Admin API

- `POST /credentials/:id/rotate`
- `POST /roles`
- `POST /policies`
- `GET /security/events`

All sensitive endpoints terminate in Rust control layer, not thin Python wrappers.

## 13. SvelteKit Route Map

### Public

- `/`
- `/pricing`
- `/security`
- `/signin`

### Authenticated

- `/app`
- `/app/settings`
- `/app/providers`
- `/app/routing`
- `/app/usage`
- `/app/audit`
- `/app/workflows`
- `/app/workflows/operator-research`
- `/app/machines`

### Auth/System

- `/auth/callback`
- `/auth/logout`
- `/auth/reauth`

SvelteKit owns cookie lifecycle and SSR auth gating. Browser never assembles privileged auth state from fragments.

## 14. Rust Service Boundaries

### 14.1 Identity Bridge

Consumes WorkOS identity payloads from SvelteKit server layer, verifies trust boundary, mirrors user/workspace state into STDB.

### 14.2 Machine Auth Service

Owns:

- enrollment
- token mint/verify
- worker scope validation
- heartbeat acceptance

### 14.3 Vault Service

Owns:

- provider credential encryption/decryption
- key rotation workflow
- access event writing
- env injection of single resolved provider credential only

### 14.4 Routing Service

Owns:

- provider selection
- local/cloud policy evaluation
- budget checks
- privacy gates
- model allowlists

### 14.5 Audit Service

Owns append-only writes for:

- auth events
- credential access events
- admin changes
- routing decisions
- workflow execution

## 15. Policy Decision Record

Every routed request writes evidence record with:

- request id
- actor type: human or machine
- workspace id
- considered providers
- chosen provider/model
- policy version
- reason code
- privacy flags
- local-only requirement flag

Routing must be inspectable, not magic.

## 16. Python Reduction Plan

Freeze new Python core logic now.

Python remains temporarily for:

- legacy hub paths
- compatibility surfaces
- migration shims

Move off Python first in these areas:

1. human auth/session boundary
2. machine auth
3. credential write/read path
4. routing/policy decisions

## 17. First Paid Workflow

### `Operator Research + Proposal Engine`

Flow:

1. ingest founder notes, docs, and URLs
2. classify sensitivity and allowed providers
3. route research tasks through policy engine
4. store outputs in founder vault
5. generate proposal/plan/brief artifact
6. emit cost, provider, and audit trail for every request span

Why first:

- strong founder value
- easy sales demo
- maps directly to operator reality
- exercises auth, vault, routing, usage, and audit spine without generic-chat sprawl

## 18. Security Control Checklist

### 18.1 Must Kill

- fragment token handoff
- privileged browser-visible tokens
- new Python auth growth
- `.env` as long-term secret architecture
- human session reuse for machine workers

### 18.2 Must Land

- secure cookie session
- short-lived machine tokens
- append-only audit events
- credential access events
- server-side RBAC checks
- provider allowlist policy
- env allowlist for subprocess execution
- admin re-auth for credential reveal/rotate/export

### 18.3 ASVS L2 Baseline Mapping

| Area | v1 control |
| --- | --- |
| Authentication | WorkOS passkeys/OIDC, secure callback, step-up auth |
| Session management | server-issued secure cookies, short TTL, no fragment/localStorage authority |
| Access control | workspace membership + role checks in Rust and SvelteKit server layer |
| Cryptography | encrypted credential storage, Keychain for local operator secrets |
| Secrets management | Infisical for system secrets, vault path for tenant creds |
| Logging | append-only auth/admin/credential/routing events in STDB |
| Data protection | provider keys never exposed to frontend |
| Architecture | separate human auth and machine auth trust paths |

## 19. Rollout Plan

### Phase 1

- SvelteKit app shell
- WorkOS auth
- secure cookie session
- remove fragment token callback path

### Phase 2

- Rust machine auth path
- Infisical system-secret integration
- worker registration in STDB

### Phase 3

- vault service
- provider credential management UI
- credential access audit

### Phase 4

- routing/policy service
- usage metering
- audit UI
- first paid workflow

## 20. Explicit Non-Goals

- no `Next.js`
- no Clerk-first path
- no Postgres-first authority split
- no new core logic in shell
- no broad team-workspace product before solo-founder v1 works

## 21. Open Questions

1. Whether current `apps/heiwa_hub/` becomes migration shim only or partial long-term control ingress.
2. Exact token format and signing strategy for machine service tokens in Rust.
3. Whether pricing/billing events stay founder-local first or route through shared plan ledger immediately.

## 22. Decision

Proceed with:

- `SvelteKit`
- `WorkOS`
- `SpacetimeDB`
- `Rust`
- `Infisical`
- macOS Keychain for MacBook-local operator secrets

Reject for v1:

- `Next.js`
- `Clerk`
- `Postgres-first`
- new Python core auth/policy work
