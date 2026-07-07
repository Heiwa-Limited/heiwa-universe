# Heiwa v1 Founder Control Plane 30-Day Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship founder-first Heiwa v1 spine with `SvelteKit` human auth/session UX, `WorkOS` identity, `Rust` control services, `SpacetimeDB` canonical audit/policy state, `Infisical` system secrets, and one sellable workflow: `Operator Research + Proposal Engine`.

**Architecture:** Browser talks only to `SvelteKit`. `SvelteKit` server routes act as narrow BFF and hold cookie/session/CSRF boundary. `Rust` owns machine auth, vault, routing, policy, and STDB writes. `WorkOS` remains human identity source-of-truth. `SpacetimeDB` stays canonical Heiwa state. `Python` is compatibility-only and must not grow new core logic.

**Tech Stack:** SvelteKit, TypeScript, WorkOS AuthKit, Rust, SpacetimeDB, Infisical, macOS Keychain, pytest, cargo test

**Spec:** `docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md`

---

## Execution Locks

These are resolved now. Do not reopen during Phase 1 unless blocked by hard implementation fact.

- Browser talks only to `SvelteKit` BFF routes.
- `Rust` control API is not casual browser-facing surface.
- Human session cookie:
  - idle timeout: `60 minutes`
  - absolute lifetime: `12 hours`
  - reauth threshold for sensitive actions: `15 minutes`
  - cookie flags: `HttpOnly`, `Secure`, `SameSite=Lax`
- CSRF:
  - browser mutations go through SvelteKit form actions or server endpoints
  - enforce `Origin` / `Referer` checks
  - add double-submit or server-issued CSRF token for non-form JSON mutations
- Machine enrollment:
  - unique machine identity = generated device keypair + stable machine UUID + operator label
  - bootstrap enrollment expires in `15 minutes`
  - default active machine cap for founder workspace: `5`
  - revoke path must invalidate active worker tokens immediately
  - lost/stolen procedure = revoke machine, rotate related bootstrap materials, audit event
  - capabilities use template-based approval at enrollment, heartbeat can refresh within approved template
- Vault ciphertext:
  - v1 default = encrypted credential material stored in `STDB` with explicit envelope versioning
  - metadata, version, and access audit also in `STDB`
  - implementation must keep storage behind narrow Rust vault interface so external ciphertext store can replace backing later without schema rewrite
- RBAC v1 roles only:
  - `owner`
  - `admin`
  - `operator`
  - `worker`
- Permission domains only:
  - `credentials`
  - `routing_policy`
  - `machines`
  - `workflows`
  - `audit`
  - `billing`
- Every routed request writes policy decision record with:
  - request id
  - actor type
  - workspace id
  - considered providers
  - chosen provider/model
  - policy version
  - reason code
  - privacy flags
  - local-only flag

## Repo Gate

No implementation starts until git state is clean enough to isolate auth work.

- unresolved merge conflicts must be `0`
- new work happens on clean branch or fresh worktree
- no Phase 1 auth code on top of conflicted files

## File Map

### New files

| File                                                                         | Responsibility                                          |
| ---------------------------------------------------------------------------- | ------------------------------------------------------- |
| `apps/heiwa_web/app/src/lib/server/auth/workos.ts`                           | WorkOS client, callback handling, session helper wiring |
| `apps/heiwa_web/app/src/lib/server/auth/session.ts`                          | Cookie issuance, auth age, reauth checks, CSRF helpers  |
| `apps/heiwa_web/app/src/routes/auth/callback/+server.ts`                     | Human auth callback route                               |
| `apps/heiwa_web/app/src/routes/auth/logout/+server.ts`                       | Session destroy route                                   |
| `apps/heiwa_web/app/src/routes/app/+layout.server.ts`                        | SSR auth gate and workspace load                        |
| `apps/heiwa_web/app/src/routes/app/settings/+page.server.ts`                 | Sensitive action reauth gate                            |
| `apps/heiwa_web/app/src/routes/app/machines/+page.server.ts`                 | Machine enrollment admin page load/actions              |
| `apps/heiwa_web/app/src/routes/app/providers/+page.server.ts`                | Provider vault management UI load/actions               |
| `apps/heiwa_web/app/src/routes/api/internal/me/+server.ts`                   | SvelteKit BFF endpoint to Rust identity bridge          |
| `apps/heiwa_web/app/src/routes/api/internal/provider-credentials/+server.ts` | BFF credential create/rotate proxy                      |
| `apps/heiwa_web/app/src/routes/api/internal/machines/+server.ts`             | BFF machine enrollment proxy                            |
| `apps/heiwa_web/app/src/routes/api/internal/usage/+server.ts`                | BFF usage/audit proxy                                   |
| `apps/heiwa_web/app/src/hooks.server.ts`                                     | Cookie/session parse and request-local auth context     |
| `apps/heiwa_web/app/tests/auth/session.test.ts`                              | Session TTL / CSRF / reauth tests                       |
| `apps/heiwa_web/app/tests/auth/callback.test.ts`                             | Callback + cookie issuance tests                        |
| `apps/heiwa_web/app/tests/app/machines.test.ts`                              | Machine admin route tests                               |
| `apps/heiwa_web/app/tests/app/providers.test.ts`                             | Provider management route tests                         |
| `apps/heiwa_orchestrator/src/http/mod.rs`                                    | Rust HTTP router for control endpoints                  |
| `apps/heiwa_orchestrator/src/http/auth.rs`                                   | SvelteKit-to-Rust identity bridge handlers              |
| `apps/heiwa_orchestrator/src/http/machines.rs`                               | Machine enrollment/register/token handlers              |
| `apps/heiwa_orchestrator/src/http/providers.rs`                              | Provider credential CRUD + rotate handlers              |
| `apps/heiwa_orchestrator/src/http/audit.rs`                                  | Usage, audit, security event handlers                   |
| `apps/heiwa_orchestrator/src/identity/mod.rs`                                | User/workspace mirror and RBAC helpers                  |
| `apps/heiwa_orchestrator/src/machines/mod.rs`                                | Machine enrollment, verification, revocation            |
| `apps/heiwa_orchestrator/src/vault/mod.rs`                                   | Vault interface, envelope versioning, STDB backing      |
| `apps/heiwa_orchestrator/src/policy/mod.rs`                                  | Routing policy load/eval and decision record writer     |
| `apps/heiwa_orchestrator/src/secrets/infisical.rs`                           | Infisical client for system secrets                     |
| `apps/heiwa_orchestrator/tests/http_auth.rs`                                 | Rust control auth endpoint tests                        |
| `apps/heiwa_orchestrator/tests/machine_enrollment.rs`                        | Machine auth flow tests                                 |
| `apps/heiwa_orchestrator/tests/provider_vault.rs`                            | Vault + rotation + audit tests                          |
| `apps/heiwa_orchestrator/tests/policy_decisions.rs`                          | Policy decision record tests                            |
| `apps/heiwa_hub/spacetimedb/src/founder_control_plane.rs`                    | STDB table/reducer additions for v1 spine               |
| `apps/heiwa_hub/tests/test_legacy_fragment_auth_removed.py`                  | Legacy fragment path kill test                          |

### Modified files

| File                                                                         | Change                                                               |
| ---------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `apps/heiwa_web/package.json`                                                | Add SvelteKit app dependencies/scripts if missing                    |
| `apps/heiwa_web/wrangler.toml`                                               | Align web deploy to SvelteKit auth surface                           |
| `apps/heiwa_hub/auth.py`                                                     | Freeze legacy auth path, add redirect or deprecation guards only     |
| `apps/heiwa_hub/mcp_server.py`                                               | Remove browser-fragment auth assumptions, keep compatibility only    |
| `apps/heiwa_hub/spacetimedb/src/lib.rs`                                      | Add founder v1 tables/reducers if not split to module                |
| `apps/heiwa_orchestrator/src/main.rs`                                        | Mount new Rust control HTTP routes                                   |
| `apps/heiwa_orchestrator/src/lib.rs`                                         | Export new modules                                                   |
| `crates/heiwa_provider/src/keychain.rs`                                      | Reuse or extend local operator secret helpers if needed              |
| `.env.example`                                                               | Shrink env surface, point system secrets to Infisical-managed values |
| `ops/rooms/web.md`                                                           | Update auth flow to SvelteKit BFF + secure cookie                    |
| `docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md` | Lock clarified decisions above                                       |

### STDB entities to add or extend

- `users`
- `workspaces`
- `workspace_memberships`
- `roles`
- `permissions`
- `oidc_identities`
- `machine_identities`
- `machine_enrollments`
- `worker_registrations`
- `service_tokens`
- `auth_events`
- `provider_credentials`
- `credential_versions`
- `credential_access_events`
- `routing_policies`
- `policy_decision_records`
- `requests`
- `request_spans`
- `audit_events`
- `security_events`
- `workflow_runs`

---

### Task 1: Clean branch and lock scope

**Files:**

- Modify: `docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md`
- Reference: `.git status`, conflicted workspace files

- [ ] **Step 1: Capture current git blocker**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
git status --short --branch
```

Expected: show current conflicted files and branch state.

- [ ] **Step 2: Add resolved execution-lock decisions to spec**

Update `docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md` with:

- SvelteKit BFF boundary
- session TTL/reauth/CSRF strategy
- machine enrollment rules
- vault ciphertext decision
- minimal RBAC
- policy decision record requirements

- [ ] **Step 3: Run doc sanity pass**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
rg -n "Open Questions|BFF|idle timeout|reauth threshold|machine enrollment|policy decision" docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md
```

Expected: clarified sections present.

- [ ] **Step 4: Resolve or isolate merge conflicts before code**

Outcome required:

- merge conflicts cleared, or
- fresh worktree/branch created after conflict resolution

- [ ] **Step 5: Commit doc-only lock**

```bash
git add docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md
git commit -m "docs: lock founder control plane execution assumptions"
```

### Task 2: Scaffold SvelteKit auth shell and BFF boundary

**Files:**

- Create: `apps/heiwa_web/app/src/hooks.server.ts`
- Create: `apps/heiwa_web/app/src/lib/server/auth/workos.ts`
- Create: `apps/heiwa_web/app/src/lib/server/auth/session.ts`
- Create: `apps/heiwa_web/app/src/routes/auth/callback/+server.ts`
- Create: `apps/heiwa_web/app/src/routes/auth/logout/+server.ts`
- Create: `apps/heiwa_web/app/src/routes/app/+layout.server.ts`
- Modify: `apps/heiwa_web/package.json`
- Modify: `apps/heiwa_web/wrangler.toml`
- Test: `apps/heiwa_web/app/tests/auth/session.test.ts`
- Test: `apps/heiwa_web/app/tests/auth/callback.test.ts`

- [ ] **Step 1: Write failing session tests**

Cover:

- unauthenticated request redirected from `/app`
- session cookie issued at callback
- idle timeout invalidates expired session
- sensitive-route helper rejects stale auth age
- CSRF helper rejects bad origin on mutation

- [ ] **Step 2: Run auth shell tests**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe/apps/heiwa_web
pnpm test auth
```

Expected: FAIL because auth shell files do not exist yet.

- [ ] **Step 3: Add WorkOS server client wrapper**

Implement `src/lib/server/auth/workos.ts` for:

- WorkOS client init
- callback exchange
- normalized identity payload

- [ ] **Step 4: Add cookie/session helper**

Implement `src/lib/server/auth/session.ts` with:

- issue cookie
- parse cookie
- auth age
- idle timeout
- absolute expiry
- CSRF token helper

- [ ] **Step 5: Add SvelteKit auth routes and SSR gate**

Implement:

- callback route
- logout route
- hooks session parse
- `/app` layout guard

- [ ] **Step 6: Re-run web auth tests**

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_web
git commit -m "feat: add sveltekit auth shell and session boundary"
```

### Task 3: Replace legacy browser fragment auth path

**Files:**

- Modify: `apps/heiwa_hub/auth.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `ops/rooms/web.md`
- Test: `apps/heiwa_hub/tests/test_legacy_fragment_auth_removed.py`

- [ ] **Step 1: Write failing legacy-path test**

Assert:

- legacy `#token=` redirect no longer emitted for browser flow
- browser path points to SvelteKit callback/cookie model

- [ ] **Step 2: Run legacy auth test**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
pytest apps/heiwa_hub/tests/test_legacy_fragment_auth_removed.py -q
```

Expected: FAIL.

- [ ] **Step 3: Freeze legacy Python auth surface**

Change Python auth to:

- stop fragment token handoff for browser UX
- keep compatibility only where strictly needed
- emit explicit deprecation comments

- [ ] **Step 4: Update room doc**

Reflect new flow in `ops/rooms/web.md`.

- [ ] **Step 5: Re-run legacy auth test**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/auth.py apps/heiwa_hub/mcp_server.py ops/rooms/web.md apps/heiwa_hub/tests/test_legacy_fragment_auth_removed.py
git commit -m "refactor: remove legacy browser fragment auth flow"
```

### Task 4: Add Rust identity bridge and STDB identity mirror

**Files:**

- Create: `apps/heiwa_orchestrator/src/http/mod.rs`
- Create: `apps/heiwa_orchestrator/src/http/auth.rs`
- Create: `apps/heiwa_orchestrator/src/identity/mod.rs`
- Modify: `apps/heiwa_orchestrator/src/main.rs`
- Modify: `apps/heiwa_orchestrator/src/lib.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Test: `apps/heiwa_orchestrator/tests/http_auth.rs`

- [ ] **Step 1: Write failing Rust auth bridge tests**

Cover:

- accept normalized identity payload from SvelteKit server
- upsert `users` and `workspace_memberships`
- reject missing workspace scope
- record `auth_events`

- [ ] **Step 2: Run failing Rust tests**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
cargo test -p heiwa-orchestrator http_auth -- --nocapture
```

Expected: FAIL because endpoints/modules missing.

- [ ] **Step 3: Add STDB tables/reducers for identity mirror**

Implement minimal reducers/tables for:

- `users`
- `workspaces`
- `workspace_memberships`
- `auth_events`

- [ ] **Step 4: Add Rust identity bridge handlers**

Implement narrow server-only endpoints:

- `POST /internal/auth/sync-user`
- `GET /internal/auth/me`

- [ ] **Step 5: Re-run tests**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat: add rust identity bridge and stdb auth mirror"
```

### Task 5: Integrate Infisical system-secret flow

**Files:**

- Create: `apps/heiwa_orchestrator/src/secrets/infisical.rs`
- Modify: `.env.example`
- Modify: `apps/heiwa_orchestrator/src/main.rs`
- Test: `apps/heiwa_orchestrator/tests/http_auth.rs`

- [ ] **Step 1: Write failing secret-provider test**

Cover:

- orchestrator reads required system secrets through Infisical-backed provider
- startup fails closed when required secret missing

- [ ] **Step 2: Run Rust test**

Expected: FAIL.

- [ ] **Step 3: Implement Infisical client wrapper**

Support:

- fetch WorkOS secrets
- fetch signing material
- fetch internal API credentials

- [ ] **Step 4: Shrink `.env.example`**

Keep only local-dev bootstrap values and pointers. Remove architecture dependence on raw secret sprawl.

- [ ] **Step 5: Re-run tests**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator/src/secrets/infisical.rs .env.example apps/heiwa_orchestrator/src/main.rs
git commit -m "feat: add infisical-backed system secret provider"
```

### Task 6: Implement machine enrollment and worker token flow

**Files:**

- Create: `apps/heiwa_orchestrator/src/http/machines.rs`
- Create: `apps/heiwa_orchestrator/src/machines/mod.rs`
- Modify: `apps/heiwa_orchestrator/src/http/mod.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Test: `apps/heiwa_orchestrator/tests/machine_enrollment.rs`
- Test: `apps/heiwa_web/app/tests/app/machines.test.ts`

- [ ] **Step 1: Write failing Rust machine tests**

Cover:

- enrollment bootstrap expires after 15 minutes
- machine registration writes `machine_identities`
- short-lived worker token minted
- revoked machine token rejected
- active machine cap enforced

- [ ] **Step 2: Write failing SvelteKit machine admin tests**

Cover:

- owner/admin can create enrollment
- worker role cannot create enrollment
- revoke action requires recent reauth

- [ ] **Step 3: Run both test suites**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
cargo test -p heiwa-orchestrator machine_enrollment -- --nocapture
cd /Users/dmcgregsauce/heiwa-universe/apps/heiwa_web
pnpm test machines
```

Expected: FAIL.

- [ ] **Step 4: Add STDB machine entities**

Implement:

- `machine_identities`
- `machine_enrollments`
- `worker_registrations`
- `service_tokens`

- [ ] **Step 5: Implement Rust machine handlers**

Endpoints:

- `POST /internal/machines/enroll`
- `POST /machines/register`
- `POST /machines/token`
- `POST /machines/revoke`

- [ ] **Step 6: Implement SvelteKit machine admin page**

Use BFF-only calls to Rust.

- [ ] **Step 7: Re-run tests**

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_orchestrator apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_web/app
git commit -m "feat: add machine enrollment and worker token flow"
```

### Task 7: Build provider credential vault with audit

**Files:**

- Create: `apps/heiwa_orchestrator/src/http/providers.rs`
- Create: `apps/heiwa_orchestrator/src/vault/mod.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_web/app/src/routes/app/providers/+page.server.ts`
- Modify: `apps/heiwa_web/app/src/routes/api/internal/provider-credentials/+server.ts`
- Test: `apps/heiwa_orchestrator/tests/provider_vault.rs`
- Test: `apps/heiwa_web/app/tests/app/providers.test.ts`

- [ ] **Step 1: Write failing vault tests**

Cover:

- create credential record
- rotate credential version
- audit access event on read/use/rotate
- worker cannot read human provider secrets
- ciphertext carries envelope version

- [ ] **Step 2: Run tests**

Expected: FAIL.

- [ ] **Step 3: Add STDB vault entities**

Implement:

- `provider_credentials`
- `credential_versions`
- `credential_access_events`

- [ ] **Step 4: Implement Rust vault interface**

Trait shape:

- `store_credential`
- `resolve_credential`
- `rotate_credential`
- `revoke_credential`

Back with STDB ciphertext storage and explicit versioning.

- [ ] **Step 5: Add provider management UI**

Support:

- list providers
- add/update key
- rotate key
- view masked metadata only

- [ ] **Step 6: Re-run tests**

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_orchestrator apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_web/app
git commit -m "feat: add provider credential vault and audit events"
```

### Task 8: Add minimal RBAC and sensitive-action reauth

**Files:**

- Modify: `apps/heiwa_orchestrator/src/identity/mod.rs`
- Modify: `apps/heiwa_web/app/src/lib/server/auth/session.ts`
- Modify: `apps/heiwa_web/app/src/routes/app/settings/+page.server.ts`
- Modify: `apps/heiwa_web/app/src/routes/app/providers/+page.server.ts`
- Modify: `apps/heiwa_web/app/src/routes/app/machines/+page.server.ts`
- Test: `apps/heiwa_web/app/tests/auth/session.test.ts`

- [ ] **Step 1: Write failing RBAC tests**

Cover role matrix:

- `owner`: all founder-v1 actions
- `admin`: machines/providers/workflows/audit
- `operator`: workflows and usage, no credential rotation
- `worker`: no browser admin actions

- [ ] **Step 2: Run session/RBAC tests**

Expected: FAIL.

- [ ] **Step 3: Implement minimal domain permission map**

Domains only:

- credentials
- routing_policy
- machines
- workflows
- audit
- billing

- [ ] **Step 4: Implement reauth threshold**

Sensitive actions fail closed if auth age > 15 minutes.

- [ ] **Step 5: Re-run tests**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator/src/identity/mod.rs apps/heiwa_web/app
git commit -m "feat: add minimal founder rbac and reauth gates"
```

### Task 9: Add routing policy engine and policy decision records

**Files:**

- Create: `apps/heiwa_orchestrator/src/policy/mod.rs`
- Modify: `apps/heiwa_orchestrator/src/http/audit.rs`
- Modify: `apps/heiwa_orchestrator/src/http/providers.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Test: `apps/heiwa_orchestrator/tests/policy_decisions.rs`

- [ ] **Step 1: Write failing policy tests**

Cover:

- private request forces local/BYOK-only lane
- cheap draft request picks lowest-cost allowed provider
- policy version written
- considered providers logged
- local-only requirement logged

- [ ] **Step 2: Run policy tests**

Expected: FAIL.

- [ ] **Step 3: Add STDB policy entities**

Implement:

- `routing_policies`
- `policy_decision_records`
- `requests`
- `request_spans`

- [ ] **Step 4: Implement policy evaluation**

Input dimensions:

- task type
- privacy level
- budget ceiling
- allowed providers
- tool access need
- local-only flag

- [ ] **Step 5: Emit policy decision record for every routed request**

Persist exact evidence fields from execution locks.

- [ ] **Step 6: Re-run tests**

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_orchestrator apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat: add routing policy engine and decision records"
```

### Task 10: Add usage and audit surfaces in SvelteKit

**Files:**

- Create: `apps/heiwa_web/app/src/routes/app/usage/+page.server.ts`
- Create: `apps/heiwa_web/app/src/routes/app/audit/+page.server.ts`
- Create: `apps/heiwa_web/app/src/routes/api/internal/usage/+server.ts`
- Modify: `apps/heiwa_orchestrator/src/http/audit.rs`
- Test: `apps/heiwa_web/app/tests/app/providers.test.ts`

- [ ] **Step 1: Write failing UI data-route tests**

Cover:

- usage page returns request spans + cost totals
- audit page returns auth/admin/policy/credential events
- worker role cannot load audit page

- [ ] **Step 2: Run SvelteKit tests**

Expected: FAIL.

- [ ] **Step 3: Add Rust audit/usage handlers**

Support:

- list usage totals
- list audit events
- list security events

- [ ] **Step 4: Add SvelteKit usage/audit pages**

Read via BFF only.

- [ ] **Step 5: Re-run tests**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator/src/http/audit.rs apps/heiwa_web/app
git commit -m "feat: add usage and audit surfaces"
```

### Task 11: Build first paid workflow end-to-end

**Files:**

- Create: `apps/heiwa_web/app/src/routes/app/workflows/operator-research/+page.server.ts`
- Modify: `apps/heiwa_orchestrator/src/http/mod.rs`
- Modify: `apps/heiwa_orchestrator/src/policy/mod.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Test: `apps/heiwa_orchestrator/tests/policy_decisions.rs`

- [ ] **Step 1: Write failing workflow tests**

Cover:

- ingest founder notes/URLs
- classify sensitivity
- route query through policy engine
- persist outputs as workflow run
- record provider/cost/audit trail

- [ ] **Step 2: Run workflow tests**

Expected: FAIL.

- [ ] **Step 3: Add workflow run storage**

Implement:

- `workflow_templates`
- `workflow_runs`

- [ ] **Step 4: Add workflow handler and page**

Minimal v1:

- input notes/URLs
- submit job
- show artifact list
- show provider/cost trace

- [ ] **Step 5: Re-run tests**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_web/app
git commit -m "feat: add operator research proposal workflow"
```

### Task 12: Pilot hardening and founder-offer prep

**Files:**

- Modify: `docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md`
- Create: `docs/superpowers/status/2026-04-17-founder-v1-pilot-readiness.md`
- Create: `apps/heiwa_web/app/src/routes/pricing/+page.svelte`
- Create: `apps/heiwa_web/app/src/routes/security/+page.svelte`

- [ ] **Step 1: Write pilot readiness checklist**

Checklist must include:

- auth working
- machine enrollment working
- provider vault working
- audit visible
- workflow demo reproducible

- [ ] **Step 2: Add pricing/security pages**

Support offer:

- `Founder AI Stack Setup`
- security posture summary

- [ ] **Step 3: Run smoke tests**

Run:

```bash
cd /Users/dmcgregsauce/heiwa-universe
cargo test -p heiwa-orchestrator
cd /Users/dmcgregsauce/heiwa-universe/apps/heiwa_web
pnpm test
```

Expected: PASS or documented failures with exact blockers.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/status/2026-04-17-founder-v1-pilot-readiness.md apps/heiwa_web/app docs/superpowers/specs/2026-04-17-heiwa-v1-founder-control-plane-design.md
git commit -m "docs: prepare founder v1 pilot readiness and offer pages"
```

## 30-Day Calendar

### Days 1-3

- Task 1
- resolve branch/worktree blocker
- lock architecture decisions in spec

### Days 4-7

- Task 2
- Task 3

### Days 8-12

- Task 4
- Task 5

### Days 13-17

- Task 6
- Task 7

### Days 18-21

- Task 8
- Task 9

### Days 22-25

- Task 10
- Task 11

### Days 26-30

- Task 12
- pilot demo
- 3 founder outreach attempts

## Verification Gates

- Gate 1: branch/worktree clean before Task 2
- Gate 2: no fragment-token browser auth after Task 3
- Gate 3: no worker path reuses human session after Task 6
- Gate 4: every provider credential read/use/rotate audited after Task 7
- Gate 5: every routed request writes policy decision record after Task 9
- Gate 6: founder workflow demo usable end-to-end after Task 11

## Known Risks

- Current repo conflicts may delay all code work until resolved.
- SvelteKit app path may need light scaffold cleanup if prior migration work diverged.
- STDB schema growth may pressure current generated bindings; keep migrations narrow and test generated artifacts early.
- Python compatibility layer may still leak old assumptions; freeze and contain it instead of polishing it.

## Definition of Done

Founder v1 is done when:

- founder signs in through WorkOS on SvelteKit
- browser only holds secure server cookie
- MacBook admin can enroll/revoke PC worker
- system secrets resolve through Infisical
- provider BYOK stored through Rust vault path
- routing policy writes decision evidence records
- usage and audit visible in app
- `Operator Research + Proposal Engine` demo works end-to-end
- branch is clean and pilot-ready
