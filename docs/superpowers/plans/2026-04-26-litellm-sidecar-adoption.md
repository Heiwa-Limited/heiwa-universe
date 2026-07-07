# LiteLLM Sidecar Adoption Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Do not install or route production traffic through LiteLLM until Task 1 is complete and the security notes are resolved.

**Goal:** Add a cost-saving inference sidecar for API-key and free-provider routes without turning Heiwa into a thin model gateway.

**Architecture:** `heiwa` and DREX remain the routing authority. LiteLLM runs only as a local sidecar for cacheable OpenAI-compatible API traffic, with Heiwa recording route decisions, budget intent, cache policy, provider outcome, and fallback evidence. Provider-owned OAuth CLI subscriptions stay direct unless a provider explicitly supports safe proxying without breaking subscription economics.

**Tech Stack:** Rust routing/evidence, local LiteLLM proxy, Redis or local cache backend, OpenRouter free-router optional, existing `~/.heiwa/config.toml`.

**Current-source checks, 2026-04-26:**

- LiteLLM docs advertise an OpenAI-compatible proxy with auth hooks, logging hooks, cost tracking, rate limiting, and OpenAI SDK base-url compatibility: <https://docs.litellm.ai/>
- LiteLLM routing supports deployment fallback/cooldown after 429s, high failure rate, and selected non-retryable errors: <https://docs.litellm.ai/docs/routing>
- LiteLLM proxy caching supports `ttl`, `s-maxage`, `no-cache`, `no-store`, and `namespace`: <https://docs.litellm.ai/docs/proxy/caching>
- LiteLLM `v1.83.7-stable` is the current stable line to evaluate, with signed Docker image guidance after the March 2026 supply-chain incident: <https://newreleases.io/project/github/BerriAI/litellm/release/v1.83.7-stable>
- OpenRouter exposes `openrouter/free`, but the free tier is rate-limited and nondeterministic, so it belongs below local/OAuth subscription routes: <https://openrouter.ai/docs/guides/routing/routers/free-models>
- E2B remains usage-billed per running sandbox second, so it should stay for untrusted code and isolation, not routine local inference: <https://e2b.dev/docs/billing>

---

## Non-Negotiables

- Do not route Claude Code, Codex, Gemini CLI, or Antigravity OAuth subscription traffic through LiteLLM unless direct testing proves it preserves subscription auth, quota semantics, tool behavior, and cost.
- Do not move route authority from Rust/DREX into LiteLLM config.
- Do not store provider API keys in repo files. Use `~/.heiwa` secrets or OS keychain-backed provider config.
- Do not enable semantic cache for private sovereign prompts until namespace, redaction, and retention behavior are explicit.
- Pin LiteLLM by exact stable version and verify Docker/package provenance before any install.

## File Structure

| Path                                             | Action | Responsibility                                            |
| ------------------------------------------------ | ------ | --------------------------------------------------------- |
| `~/.heiwa/config.toml`                           | Modify | Add disabled-by-default sidecar settings                  |
| `crates/heiwa_provider/src/providers/litellm.rs` | Create | Local sidecar adapter                                     |
| `crates/heiwa_provider/src/registry.rs`          | Modify | Register `litellm_sidecar` account kind                   |
| `apps/heiwa_shell/src/main.rs`                   | Modify | Show sidecar health in `heiwa providers` / `heiwa doctor` |
| `apps/heiwa_core/src/drex/router.rs`             | Modify | Permit sidecar only for eligible API-key/free tiers       |
| `docs/standards/inference-sidecars.md`           | Create | Policy and operator notes                                 |

---

### Task 1: Verify sidecar economics and security

- [ ] **Step 1: Inventory current paid/free routes**

Run:

```bash
heiwa-route status
```

Expected: local Ollama first, Google free-tier second, subscription routes later.

- [ ] **Step 2: Prove OAuth CLI routes stay direct**

Run each provider through current adapter path and record auth kind, model, cache tokens, and cost bucket.

Expected: Claude/Codex/Gemini/Antigravity remain `oauth_cli` or provider-native shell routes, not LiteLLM.

- [ ] **Step 3: Threat-check LiteLLM version**

Verify exact LiteLLM package and Docker image signatures. Do not install `1.82.7` or `1.82.8`.

Expected: pinned `v1.83.7-stable` or newer stable with verified provenance.

- [ ] **Step 4: Decide cache boundary**

Document which prompts may use exact cache, semantic cache, and no cache.

Expected: sovereign/private workspace prompts default `no-store`; low-risk public docs/search/classification may cache.

### Task 2: Add disabled sidecar config

- [ ] **Step 1: Extend `~/.heiwa/config.toml`**

Add:

```toml
[sidecars.litellm]
enabled = false
base_url = "http://127.0.0.1:4000"
allowed_tiers = ["api_key", "free"]
deny_auth_kinds = ["oauth_cli", "local_runtime"]
cache_default = "no-store"
```

- [ ] **Step 2: Add config parser tests**

Expected: missing config means disabled; explicit `enabled=true` still rejects denied auth kinds.

### Task 3: Implement provider adapter

- [ ] **Step 1: Create `litellm.rs` adapter**

Use OpenAI-compatible `/v1/chat/completions` only. Return provider outcome, cache headers, latency, token usage, and selected upstream model.

- [ ] **Step 2: Register `litellm_sidecar`**

Expected: `heiwa providers` shows health but marks it inactive unless enabled.

- [ ] **Step 3: Add route guard**

Expected: routes with `auth_kind=oauth_cli` or `provider=ollama` cannot select LiteLLM.

### Task 4: Add cache policy controls

- [ ] **Step 1: Map Heiwa policy to LiteLLM cache controls**

Use `no-store` by default. Permit `ttl` and `namespace` only when route metadata says prompt class is cacheable.

- [ ] **Step 2: Add cache evidence**

Expected: route records include `cache_policy`, `cache_hit`, `cache_namespace`, `cache_ttl_s`, and fallback reason.

### Task 5: Optional OpenRouter free route

- [ ] **Step 1: Add `openrouter/free` as lowest remote tier**

Expected: only selected for low-risk, non-sovereign, low-priority work after local and subscription-safe routes are unavailable.

- [ ] **Step 2: Record nondeterminism**

Expected: evidence captures actual returned model because `openrouter/free` can choose among free models.

### Task 6: Ship behind operator switch

- [ ] **Step 1: Add `heiwa doctor --ai-ops` checks**

Checks: LiteLLM reachable, version pinned, cache backend reachable, deny-auth list present, no repo-stored API keys.

- [ ] **Step 2: Add local smoke**

Run a harmless public prompt through sidecar and compare direct provider cost/evidence.

Expected: no behavior change when sidecar disabled; measurable cost/cache evidence when enabled for eligible routes.

---

## OmniRoute Pattern Notes

OmniRoute is useful as a reference, not a runtime dependency.

**Adopt as patterns:**

- Subscription quota dashboards and reset-aware routing shape
- Multi-account round-robin per provider/rate group
- Explicit fallback chain vocabulary: subscription -> API key -> cheap -> free
- MCP/tool inventory ideas for operator visibility

**Skip as dependency:**

- TypeScript runtime control plane
- Unknown subscription proxy semantics
- Provider/star-count marketing as capability evidence
- Any path that makes external gateway config more authoritative than DREX

## Success Criteria

- `heiwa-route status` still shows local/OAuth routes as first-class Heiwa routes.
- LiteLLM is visible as a sidecar, not a route authority.
- Cached requests are limited to explicit low-risk prompt classes.
- Route evidence can answer: why this provider, why cached or not, what fallback occurred, what it cost.
- Disabling `[sidecars.litellm]` restores current behavior without code changes.
