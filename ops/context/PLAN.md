# Heiwa Product Roadmap

## Summary

Ship a multi-tenant BYOK orchestration platform. Users sign in via Discord, connect their AI provider keys, and run agent workflows through web and Discord interfaces. Trading and Autoresearch are the launch verticals.

SpacetimeDB remains the authoritative state layer. Railway remains the primary runtime. The cognition pipeline (intent, risk, routing, compilation) is retained and extended with per-user key routing.

## Phase 1: Multi-Tenant Foundation

- User table in STDB with unique identity, display name, and creation metadata
- OAuth identities table linking Discord (and future providers) to user records
- Discord sign-in flow: OAuth2 code grant, token exchange, user creation or linking
- `user_id` scoping on all core tables: proposals, missions, runs, routes, artifacts
- User-scoped STDB views and queries -- no user can read another user's state
- Agent standards contract: one canonical versioned standard document, loaded as execution context, validated by HeiwaBench
- Execution policy: feature-flagged `observe` / `enforce` modes for all execution path changes

**Done when**: A user can sign in via Discord OAuth, and their identity exists in STDB with correct scoping. All core tables enforce user_id isolation.

## Phase 2: BYOK Key Vault + Per-User Routing

- Web UI on `app.heiwa.ltd` for connecting API keys (OpenAI, Anthropic, Google, etc.)
- Encrypted credential vault in STDB -- keys encrypted at rest, decrypted only at execution time
- Per-user rate ledger: track each user's rate limits and usage across their connected providers
- Cognition pipeline routes using the user's available providers, not just platform defaults
- Credential validation and health checks: test key on connect, flag expired or invalid keys
- Tool lifecycle hooks: `before_tool_call` and `after_tool_call` at route resolution and dispatch
- Lease-gated execution: tool execution requires an active lease matching tool/runtime/scope

**Done when**: A user connects an API key on the web, and Hub routes a task through that key. Lease enforcement is active in `enforce` mode.

## Phase 3: First Vertical -- Trading

- Trading loop runs as a Hub mission (absorb `apps/heiwa_trading/` from standalone service into orchestrated workflow)
- User signs in, connects keys, Heiwa runs market scans using their inference budget
- Results surfaced via Discord DM and web dashboard
- Portfolio view on `app.heiwa.ltd` -- positions, P&L, scan history
- Polymarket scan triggerable from Discord DM command

**Done when**: A user can trigger a Polymarket scan from Discord, see scored results in DM, and view portfolio on web.

## Phase 4: Product Surfaces

- `app.heiwa.ltd` dashboard: mission history, active runs, provider status, rate limit visibility, trading cockpit
- Discord bot: DM-based task submission, results delivery, alerts, approval requests
- Tiered access:
  - **Free**: 50 orchestrations/month, BYOK only
  - **Pro**: unlimited orchestrations, platform inference credits included
  - **Team**: shared workspaces, multi-user billing
- Billing events recorded in STDB for metering and invoicing

**Done when**: Both web and Discord interfaces are functional for a new user end-to-end. A free-tier user can sign up, connect keys, and run tasks without intervention.

## Phase 5: Platform Growth

- **Autoresearch** as vertical #2: Karpathy-style autonomous research loops with multi-model orchestration
- Custom missions: user-defined multi-step agent workflows with configurable steps, constraints, and acceptance criteria
- Billing and usage metering from STDB `billing_events`
- HeiwaCells marketplace: community-contributed agent personas available for orchestration
- HeiwaPods: multi-node scaling -- formalized pod records with provider identity, trust tier, GPU inventory, liveness
- Pod trust-tier routing: sovereign tasks route only to trusted local pods

**Done when**: Multiple verticals run simultaneously for multiple users. Community HeiwaCells are discoverable and usable.

## Retained from Current Plan

These items from the previous roadmap are folded into the phases above:

| Item | Destination |
|------|-------------|
| Agent standards contract (versioned, validated by HeiwaBench) | Phase 1 |
| Execution policy (`observe` / `enforce` modes) | Phase 1 |
| Tool lifecycle hooks (`before_tool_call` / `after_tool_call`) | Phase 2 |
| Lease enforcement (scope matching, fail-closed semantics) | Phase 2 |
| HeiwaPods (pod records, trust-tier routing, GPU inventory) | Phase 5 |

## Deprioritized

- **Textual terminal UI**: The web dashboard (`app.heiwa.ltd`) is the product UI. Terminal rendering is an operator convenience, not a user-facing surface.
- **Rust limb extraction**: Premature optimization. Python-first until profiling proves otherwise.
- **MCP as a primary surface**: MCP remains available but is not a user-facing product interface.

## Public Interfaces / Contracts

- **Execution hook contract**: `before_tool_call(context) -> allow | deny | approval_required | append_audit_metadata`; `after_tool_call(context) -> audit/result metadata updates`
- **Lease validation contract**: Every tool execution requires an active lease matching tool/runtime/scope
- **Credential vault contract**: Store, retrieve, revoke, encrypt/decrypt provider API keys per user
- **Pod contract**: Persistent pod metadata with capability, trust tier, liveness, allocation state, GPU inventory

## Test Plan

### Route and gateway tests
- Execution proceeds when a valid lease exists and scopes match
- Execution is denied when no active lease exists or tool scope mismatch occurs
- Custom hook can deny execution and the denial propagates back
- `before_tool_call` failures fail-closed (deny execution)
- `after_tool_call` can append metadata without breaking result flow

### STDB / control-plane tests
- Lease issuance, renewal, and revocation remain compatible with proposal flow
- Pod registration produces a valid pod record with all required fields

### Auth flow tests
- Discord OAuth code grant produces a valid user record in STDB
- Duplicate OAuth sign-in links to existing user (no duplicates)
- User_id scoping prevents cross-user data access on all core tables

### Credential vault tests
- API key stored encrypted at rest, decrypted correctly at execution time
- Key revocation immediately prevents routing through that provider
- Invalid key detection on connect (test call fails, key flagged)
- Encrypted keys not readable via raw STDB queries

### Per-user routing tests
- User A's connected providers route differently from User B's
- Missing provider key for a user falls back to platform default (if allowed by tier)
- Rate limit exhaustion on one user's key does not affect other users

### Billing event tests
- Each orchestration records a billing event with correct user_id, provider, model, and token count
- Free-tier user blocked after 50 orchestrations in a billing period
- Pro-tier user not blocked regardless of orchestration count

### Acceptance scenarios
- New user: Discord sign-in, connect API key, trigger task, see result -- end to end
- Trading user: sign in, connect keys, trigger Polymarket scan from Discord, view portfolio on web
- High-risk operation requests approval before execution
- Operator can revoke a lease and see active execution fail-closed immediately

## Assumptions

- SpacetimeDB remains the authoritative state layer
- Railway remains the primary runtime with auto-deploy on main
- Python-first for all user-facing and operator-facing code
- STDB lookup latency is acceptable for lease checks in v1; cache with subscription-based invalidation planned when needed
- Discord is the primary auth provider at launch; additional OAuth providers (Google, GitHub) are future work
