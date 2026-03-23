# Design Spec: Routing Unification (2026-03-23)

## Status: DRAFT (post-review revision)
**Date:** Sunday, March 23, 2026
**Author:** Claude Code (Heiwa Class 3 Executor)
**Reviewed by:** Gemini CLI (peer review during brainstorming)

## Goal

Unify three overlapping routing systems (`ComputeRouter`, `LocalLLMEngine`, `ai_router.json`) into a single decision path where `ComputeRouter` is the only router and `LocalLLMEngine` is a pure executor.

## Current Reality

Three systems make independent routing decisions:

1. **ComputeRouter** (`packages/heiwa_cognition/heiwa_cognition/router.py`) — Control-plane router. Converts intent/risk/privacy into a `ComputeRoute`. Reads STDB model tiers first, falls back to `ai_router.json`. Used by Spine, MCP server, enrichment for Class 3 agentic task routing.

2. **LocalLLMEngine** (`packages/heiwa_cognition/heiwa_cognition/llm.py`) — API inference layer with its own hardcoded tier chains (`_tier_chain()`). Three complexity-dependent cascades: low = Gemini Flash -> Gemini CLI -> Ollama; medium = Flash -> Pro -> Gemini CLI -> Claude CLI -> Ollama; high = Pro -> Flash -> Gemini CLI -> Claude CLI -> Ollama. Uses both direct Gemini HTTP API (`_call_gemini` via `GEMINI_API_KEY`, rate group `google_gemini_api`) and CLI subprocess calls (`_call_cli_tool`). Used by intent classification, planner (via IntentNormalizer), audit, messenger (via `planner.engine`), HeiwaClaw reflex adapter, chat. Ignores ComputeRouter entirely.

3. **ai_router.json** (`config/swarm/ai_router.json`) + **ProviderRegistry** (`packages/heiwa_sdk/heiwa_sdk/provider_registry.py`) — Static provider metadata (transport, auth, rate groups, rotation). Read by ComputeRouter for fallback routing and by ProviderRegistry for provider resolution.

The problem: `LocalLLMEngine` duplicates routing logic that should live in `ComputeRouter`. When a caller needs LLM inference for enrichment or classification, the request bypasses the compute routing pipeline entirely and follows a hardcoded cascade that doesn't respect STDB tier configuration, capability classes, or rate ledger state.

## Target Design

### Architecture

```
Caller -> llm_generate(prompt, intent, risk)
            |
            v
        ComputeRouter.route_inference(intent, risk, privacy, runtime)
            |  (reads STDB tiers + ProviderRegistry)
            v
        RoutedPlan(primary + fallbacks + retry_policy)
            |
            v
        LocalLLMEngine.execute(target, prompt, system)
            |  (resolves provider mechanics via ProviderRegistry.resolve())
            v
        Provider call (api_http | cli_stdio | local_http)
            |
            v  (on failure: walk fallbacks, mark rate ledger)
            v  (on chain exhaustion: re-query ComputeRouter)
            v
        InferenceResult
```

### Responsibilities After Unification

| Component | Decides | Executes |
|---|---|---|
| ComputeRouter | Which model, provider, fallback chain | Nothing |
| LocalLLMEngine | Nothing | Provider API calls, retries, response normalization |
| ProviderRegistry | Nothing | Nothing (metadata lookup only) |
| Facade (`llm_generate`) | Nothing | Wires router -> executor, manages fallback/re-query loop |
| ai_router.json | Nothing | Nothing (bootstrap seed for STDB, not a live override) |

### What Changes

- `ComputeRouter` gains `route_inference()` method that returns a `RoutedPlan`
- `LocalLLMEngine` gains `execute(target, prompt, system)` method that calls the right provider based on `InferenceTarget.transport`, resolving `cli_command` and adapter details via `ProviderRegistry.resolve(target.provider)`
- New facade functions `llm_generate()`, `llm_generate_json()`, `llm_generate_async()`, and `llm_generate_with_plan()` in `heiwa_cognition`, exported from `packages/heiwa_cognition/heiwa_cognition/__init__.py` and the hub compatibility shims
- All direct and indirect callers migrate from `LocalLLMEngine` complexity-based APIs to the `llm_generate*` facade family

### What Doesn't Change

- `ComputeRouter.route()` for Class 3 agentic task routing (Spine, MCP)
- HeiwaClaw/ToolMesh execution path
- Provider-specific call mechanics inside `LocalLLMEngine` (`_call_ollama`, `_call_gemini`, `_call_cli_tool`)
- `ProviderRegistry` structure and interface
- Rate ledger integration
- `ai_router.json` file structure (stays as seed data)

## Data Contracts

### RoutedPlan

The decision object `ComputeRouter.route_inference()` produces:

```python
@dataclass(slots=True)
class RoutedPlan:
    primary: InferenceTarget
    fallbacks: list[InferenceTarget]  # 1-2 capability-preserving alternatives
    intent: str
    risk: str
    privacy: str
    reason: str                       # why this route was chosen
    retry_policy: str                 # "exhaust_then_reroute" | "reroute_immediately"
```

`retry_policy` controls behavior on chain exhaustion:
- `exhaust_then_reroute`: walk primary + fallbacks locally, then re-query ComputeRouter with updated availability. This is the default.
- `reroute_immediately`: skip fallbacks and re-query immediately. Reserved for cases where the primary failure indicates systemic provider issues.

### InferenceTarget

A single provider+model to attempt:

```python
@dataclass(slots=True)
class InferenceTarget:
    model_id: str             # STDB canonical ID, e.g. "gemini-cli/gemini-3-flash"
    provider_model_id: str    # provider-specific string, e.g. "gemini-3-flash-preview"
    provider: str             # e.g. "google-gemini-cli"
    rate_group: str           # e.g. "google_gemini_cli"
    transport: str            # "api_http" | "cli_stdio" | "local_http"
    effort_knob: str          # provider-specific effort setting
    capability_class: int     # preserved from STDB tier
```

- `model_id` is the STDB canonical ID from the `model_tiers` table.
- `provider_model_id` is the string passed to the provider API (e.g. the actual Gemini model name).
- Both fields come from the STDB `model_tiers` row.
- `transport` values are exactly the set from `ProviderRegistry`: `api_http`, `cli_stdio`, `local_http`. `gateway_websocket` is out of scope for LLM inference — that transport is for Class 3 agentic tasks routed through HeiwaClaw/ToolMesh. If `ProviderRegistry.resolve()` returns `gateway_websocket` for a provider, `execute()` must treat it as an unsupported transport and skip to the next fallback.
- `LocalLLMEngine.execute()` resolves additional provider mechanics (e.g. `cli_command`, adapter tool) via `ProviderRegistry.resolve(target.provider)` internally. `InferenceTarget` stays minimal.

### InferenceResult

What the executor returns:

```python
@dataclass(slots=True)
class InferenceResult:
    text: str
    provider: str
    model: str
    attempts: int           # how many targets were tried
    rerouted: bool          # whether ComputeRouter was re-queried
```

### Facade Contract

```python
# Default path — callers use this
def llm_generate(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    *,
    privacy: str | None = None,
    runtime: str | None = None,
    system: str | None = None,
) -> str:
    """Route and execute an LLM inference call. Returns text."""

# JSON variant — for callers that need parsed dict output (e.g. intent classification)
def llm_generate_json(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    **kwargs,
) -> dict[str, Any]:
    """Route and execute, then parse response as JSON. Returns {} on parse failure."""

# Async variant — for callers in async contexts (e.g. heiwaclaw, audit)
async def llm_generate_async(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    **kwargs,
) -> str:
    """Async version of llm_generate. Runs execute() in thread pool."""

# Escape hatch — for inspection, tests, and debugging only
def llm_generate_with_plan(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    **kwargs,
) -> tuple[RoutedPlan, InferenceResult]:
    """Same as llm_generate but returns the routing decision alongside the result."""
```

The facade is the only new default path. `llm_generate_json()` replaces `LocalLLMEngine.generate_json()`. `llm_generate_async()` replaces `LocalLLMEngine.generate_async()`. `llm_generate_with_plan()` exists for inspection, tests, and debugging — not as a regular call path.

### Fallback Assembly

`ComputeRouter.route_inference()` assembles the `RoutedPlan` from:

1. STDB `model_tiers` table — filtered by intent strengths, capability class, privacy, runtime compatibility, enabled status. Note: STDB stores strengths as `strengths_json` (JSON string); the seed file uses `strengths` (list). The router already handles both forms via `json.loads()`.
2. `ProviderRegistry` — joined to get transport, rate_group, and provider metadata
3. Rate ledger — exclude providers currently in cooldown or exhausted

Fallbacks are selected from the same filtered tier set, ordered by cost (cheapest first), and should preserve the capability class of the primary. If no providers at the required capability class are available after exhaustion+reroute, the facade returns empty rather than silently downgrading. Callers that want graceful degradation can catch the empty result and retry with a lower risk level explicitly.

### Runtime Parameter Contract

`route_inference()` accepts an optional `runtime` parameter with the same semantics as `_select_model_from_tiers()`:
- `"sovereign"` or `"boost"` / `"macbook"`: only local providers (ollama, local, vllm, litellm)
- `"railway"`: exclude local providers
- `"both"`: allow both local and remote providers
- `None` / `"auto"`: detect from environment (`RAILWAY_ENVIRONMENT`, `HEIWA_EXECUTOR_RUNTIME`, etc.)

### Prerequisite: Gemini API Provider Registration

The current Gemini HTTP API path (`LocalLLMEngine._call_gemini()`) uses `GEMINI_API_KEY` directly with a hardcoded rate group `google_gemini_api`. This provider has no representation in `ai_router.json`, `model_tiers.json`, or `ProviderRegistry`. Before Phase 1, the following seed data must be added:

1. Add `google-gemini-api` provider entry to `ai_router.json` with `transport: "api_http"`, `auth_kind: "api_key"`, `rate_group: "google_gemini_api"`
2. Add Gemini API model tiers to `config/seeds/model_tiers.json` (e.g. `gemini-api/gemini-2.5-flash`, `gemini-api/gemini-2.5-pro`) with `provider: "google-gemini-api"`
3. Run seed loader to populate STDB

Without this, `route_inference()` cannot produce `InferenceTarget` entries for the Gemini HTTP API path, which is the current primary inference provider on Railway.

## Execution Flow

1. Caller invokes `llm_generate(prompt, intent="classification", risk="low")`
2. Facade calls `ComputeRouter.route_inference("classification", "low")` -> `RoutedPlan`
3. Facade calls `LocalLLMEngine.execute(plan.primary, prompt, system)`:
   - Engine calls `ProviderRegistry.resolve(target.provider)` to get `cli_command`, adapter details
   - Engine dispatches to `_call_gemini()`, `_call_ollama()`, or `_call_cli_tool()` based on `target.transport`
   - Engine passes `target.provider_model_id` to the provider API
4. On primary failure, facade checks `retry_policy`:
   - If `exhaust_then_reroute` (default): walk `plan.fallbacks` in order, recording failures in rate ledger
   - If `reroute_immediately`: skip fallbacks, go directly to step 5
5. On chain exhaustion (primary + fallbacks all failed, or skipped via `reroute_immediately`):
   - Call `ComputeRouter.route_inference()` again with updated ledger state
   - Try the new plan's primary once (no further recursion)
6. On total failure: return empty string / empty `InferenceResult`

## Error Handling

- **Provider call failures** (HTTP errors, timeouts, empty responses): caught inside `LocalLLMEngine.execute()`, logged. Facade walks to next target.
- **Rate limit hits** (429s, ledger exhaustion): recorded in rate ledger before moving to next target. Updates availability state for the re-query path.
- **Privacy violations** (sovereign task routed to cloud provider): enforced at `route_inference()` time, not execution time. The router never produces an `InferenceTarget` that violates privacy constraints.
- **Total failure** (all targets exhausted + re-query exhausted): returns empty string from `llm_generate()`, empty `InferenceResult` from `llm_generate_with_plan()`. Callers already handle empty returns — this matches current `LocalLLMEngine` behavior.

## Migration Path

### Phase 1: Add New Code, Change Nothing

- Add `RoutedPlan`, `InferenceTarget`, `InferenceResult` dataclasses to `heiwa_cognition`
- Add `ComputeRouter.route_inference()` method
- Add `LocalLLMEngine.execute(target, prompt, system)` method
- Add `llm_generate()` / `llm_generate_with_plan()` facade functions
- All new code, zero changes to existing callers. Old `generate(complexity=...)` still works.

### Phase 2: Migrate Callers One at a Time

Tests run after each migration.

1. `packages/heiwa_cognition/heiwa_cognition/intent.py` — uses `engine.generate_json()` via IntentNormalizer. Migrate to `llm_generate_json(prompt, intent="classification", risk="low")` and remove the `IntentNormalizer.engine` dependency once the facade path is live.
2. `packages/heiwa_cognition/heiwa_cognition/planner.py` — passes `LocalLLMEngine` instance to IntentNormalizer. After intent.py is migrated, planner no longer needs to construct or pass an engine instance. Remove the `LocalLLMEngine` import and stop passing an engine into IntentNormalizer.
3. `apps/heiwa_hub/agents/messenger.py` — currently reaches through `self.planner.engine.generate(...)`. Once planner stops owning a `LocalLLMEngine`, messenger should call the new facade directly for its low-risk chat fallback.
4. `packages/heiwa_sdk/heiwa_sdk/audit.py` — uses `engine.generate()` and `engine.generate_async()`. Migrate to `llm_generate(prompt, intent="audit", risk="low")` and `llm_generate_async()`.
5. `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/reflex.py` — currently imports `LocalLLMEngine` from `heiwa_hub.cognition.llm_local`, creating a cross-package dependency (`heiwa_sdk` -> `heiwa_hub`). Migrating to `llm_generate()` from `heiwa_cognition` fixes this dependency direction. This is a dependency graph change, not just a call-site change.
6. `apps/heiwa_hub/agents/heiwaclaw.py` — uses `engine.generate_async()`. Migrate to `llm_generate_async(prompt, intent=..., risk=...)`.
7. `apps/heiwa_hub/chat.py` — last, since it's already coupled and pending removal. Uses `generate()` and `generate_async()`.

Note: `enrichment.py` is NOT a LocalLLMEngine consumer. It uses `ComputeRouter.route()` for control-plane task routing and has no direct LLM inference calls. No migration needed.

### Phase 3: Remove Dead Code (Deferred)

Phase 3 happens strictly after all callers from Phase 2 are migrated and all tests pass. Not before.

- `generate_json()` and `generate_async()` stay as compatibility wrappers until Phase 3. They get facade equivalents before removal.
- Delete `LocalLLMEngine._tier_chain()`, `generate(complexity=...)`, `generate_json()`, `generate_async()`
- Remove `complexity` parameter from all signatures
- Update `apps/heiwa_hub/cognition/llm_local.py` and `apps/heiwa_hub/cognition/__init__.py` to re-export the facade helpers alongside `LocalLLMEngine` for compatibility

### Risk Mitigation

Phase 1 is purely additive. If anything goes wrong in Phase 2, callers can revert to `generate(complexity=...)` individually. Phase 3 only happens after all callers are migrated and tests are green.

## Testing Strategy

- **`route_inference()` unit tests**: given mock STDB tiers and provider registry, verify primary selection, fallback ordering, capability preservation, privacy filtering, runtime filtering
- **`LocalLLMEngine.execute()` unit tests**: given an `InferenceTarget`, verify correct provider method is called based on `transport`, verify `ProviderRegistry.resolve()` is used for `cli_command` and adapter details
- **Facade integration tests**: mock both router and engine, verify the full flow: route -> execute -> fallback -> re-query -> result
- **Migration regression**: after each Phase 2 caller migration, run existing tests for that module to verify behavior is unchanged
- **No new e2e tests** that hit real providers — all provider calls stay mocked

## Out of Scope

- Native STDB client subscriptions for model tier changes
- `gateway_websocket` transport for inference (that's HeiwaClaw/ToolMesh territory)
- Replacing `ProviderRegistry` or changing `ai_router.json` structure
- Async-first rewrite of `LocalLLMEngine` (can happen later if needed)
- `chat.py` removal (separate task, already tracked)
- Captain agent / Spine decomposition (separate architectural work)
