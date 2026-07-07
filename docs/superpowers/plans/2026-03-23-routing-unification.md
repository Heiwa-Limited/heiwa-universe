# Routing Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify Heiwa's LLM routing so `ComputeRouter` owns policy, `LocalLLMEngine` only executes provider calls, and every inference caller uses the same facade path.

**Architecture:** Add a small decision layer (`RoutedPlan` + `InferenceTarget`) on top of the existing STDB-backed router, then make `LocalLLMEngine` an execution-only adapter that resolves provider mechanics through `ProviderRegistry`. A thin facade (`llm_generate*`) becomes the default call path, with compatibility shims kept only until all consumers are migrated and the legacy methods can be removed.

**Tech Stack:** Python 3.14, pytest, JSON seed files (`ai_router.json`, `model_tiers.json`), existing Heiwa Cognition/SDK/Hub packages.

---

### Task 1: Register Gemini API in the runtime seed data

**Files:**

- Modify: `config/swarm/ai_router.json`
- Modify: `config/seeds/model_tiers.json`
- Create: `apps/heiwa_hub/tests/test_llm_routing_unification.py`

- [ ] **Step 1: Write the failing test**

Add a focused test that proves the runtime registry can resolve the Gemini HTTP API provider and its model tiers:

```python
def test_google_gemini_api_provider_is_registered():
    registry = ProviderRegistry()
    cfg = registry.resolve("google-gemini-api")
    assert cfg.transport == "api_http"
    assert cfg.auth_kind == "api_key"
    assert cfg.rate_group == "google_gemini_api"
```

Add a second assertion that the seed-backed model tier list includes the Gemini API canonical IDs the router will need.

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_unification.py -k gemini_api -v`

Expected: FAIL because `google-gemini-api` is not yet present in the router seed data.

- [ ] **Step 3: Write minimal implementation**

Add the missing `google-gemini-api` provider entry to `config/swarm/ai_router.json` and add the Gemini API model tiers to `config/seeds/model_tiers.json` so STDB can seed the routeable runtime inventory.

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_unification.py -k gemini_api -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add config/swarm/ai_router.json config/seeds/model_tiers.json apps/heiwa_hub/tests/test_llm_routing_unification.py
git commit -m "feat: register gemini api routing seed data"
```

### Task 2: Add the shared routing contracts and facade

**Files:**

- Modify: `packages/heiwa_cognition/heiwa_cognition/router.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/llm.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/__init__.py`
- Modify: `apps/heiwa_hub/cognition/llm_local.py`
- Modify: `apps/heiwa_hub/cognition/__init__.py`
- Modify: `packages/heiwa_cognition/CONTEXT.md`
- Modify: `apps/heiwa_hub/tests/test_llm_routing_unification.py`

- [ ] **Step 1: Write the failing test**

Extend `apps/heiwa_hub/tests/test_llm_routing_unification.py` with tests for:

```python
def test_route_inference_returns_plan():
    ...

def test_llm_generate_facade_executes_route_and_returns_text():
    ...

def test_llm_generate_with_plan_returns_decision_and_result():
    ...

def test_execute_uses_provider_registry_resolution():
    ...
```

The tests should assert:

- `ComputeRouter.route_inference()` returns a `RoutedPlan`
- `LocalLLMEngine.execute()` accepts an `InferenceTarget`
- the facade family (`llm_generate`, `llm_generate_json`, `llm_generate_async`, `llm_generate_with_plan`) exists and is imported from the package surface
- provider resolution goes through `ProviderRegistry.resolve(target.provider)`

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_unification.py -k "route_inference or facade or execute" -v`

Expected: FAIL because the new dataclasses and facade methods do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement the new shared routing layer:

- `RoutedPlan`, `InferenceTarget`, `InferenceResult`
- `ComputeRouter.route_inference()`
- `LocalLLMEngine.execute(target, prompt, system)`
- `llm_generate()`, `llm_generate_json()`, `llm_generate_async()`, `llm_generate_with_plan()`
- package and hub compatibility exports in `__init__.py` and `llm_local.py`
- context doc wording updates so the package docs describe the new facade path instead of the old tier chain

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_unification.py -k "route_inference or facade or execute" -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/router.py \
  packages/heiwa_cognition/heiwa_cognition/llm.py \
  packages/heiwa_cognition/heiwa_cognition/__init__.py \
  apps/heiwa_hub/cognition/llm_local.py \
  apps/heiwa_hub/cognition/__init__.py \
  packages/heiwa_cognition/CONTEXT.md \
  apps/heiwa_hub/tests/test_llm_routing_unification.py
git commit -m "feat: add unified llm routing facade"
```

### Task 3: Migrate the lightweight internal consumers

**Files:**

- Modify: `packages/heiwa_cognition/heiwa_cognition/intent.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/planner.py`
- Modify: `apps/heiwa_hub/agents/messenger.py`
- Modify: `apps/heiwa_hub/tests/test_intent_classifier.py`
- Modify: `apps/heiwa_hub/tests/test_chat_engine.py`
- Create: `apps/heiwa_hub/tests/test_llm_routing_migration.py`

- [ ] **Step 1: Write the failing test**

Create `apps/heiwa_hub/tests/test_llm_routing_migration.py` with focused assertions that:

- `IntentNormalizer` no longer needs a `LocalLLMEngine` instance to classify with JSON
- `LocalTaskPlanner` can be constructed without wiring an engine into the normalizer
- `messenger.py` uses the new facade for its low-risk chat fallback instead of reaching through `planner.engine.generate(...)`

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_migration.py -v`

Expected: FAIL because the old engine-based call paths are still in place.

- [ ] **Step 3: Write minimal implementation**

Update:

- `intent.py` to call `llm_generate_json()` and drop the engine dependency from `IntentNormalizer`
- `planner.py` to stop constructing/passing `LocalLLMEngine` into `IntentNormalizer`
- `messenger.py` to call the new facade directly for chat fallback

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_migration.py apps/heiwa_hub/tests/test_intent_classifier.py apps/heiwa_hub/tests/test_chat_engine.py -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/intent.py \
  packages/heiwa_cognition/heiwa_cognition/planner.py \
  apps/heiwa_hub/agents/messenger.py \
  apps/heiwa_hub/tests/test_intent_classifier.py \
  apps/heiwa_hub/tests/test_chat_engine.py \
  apps/heiwa_hub/tests/test_llm_routing_migration.py
git commit -m "feat: migrate internal callers to llm facade"
```

### Task 4: Migrate the remaining inference callers

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/audit.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/reflex.py`
- Modify: `apps/heiwa_hub/agents/heiwaclaw.py`
- Modify: `apps/heiwa_hub/chat.py`
- Modify: `apps/heiwa_hub/tests/test_heiwa_agent.py`
- Modify: `apps/heiwa_hub/tests/test_program_validation.py`
- Create: `apps/heiwa_hub/tests/test_llm_routing_external_consumers.py`

- [ ] **Step 1: Write the failing test**

Create `apps/heiwa_hub/tests/test_llm_routing_external_consumers.py` with targeted assertions that:

- `RepoAuditor` uses `llm_generate_async()`
- `ReflexAdapter` no longer imports `LocalLLMEngine` through the hub shim
- `HeiwaClawAgent` and `ChatEngine` route through the new facade family

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_external_consumers.py -v`

Expected: FAIL because the old direct `LocalLLMEngine` calls are still present.

- [ ] **Step 3: Write minimal implementation**

Update:

- `audit.py` to use `llm_generate_async()`
- `reflex.py` to import the facade from `heiwa_cognition` instead of `heiwa_hub.cognition.llm_local`
- `heiwaclaw.py` to use `llm_generate_async()`
- `chat.py` to use the facade family instead of the old tiered engine entrypoints

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_external_consumers.py apps/heiwa_hub/tests/test_heiwa_agent.py apps/heiwa_hub/tests/test_program_validation.py -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/audit.py \
  packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/reflex.py \
  apps/heiwa_hub/agents/heiwaclaw.py \
  apps/heiwa_hub/chat.py \
  apps/heiwa_hub/tests/test_heiwa_agent.py \
  apps/heiwa_hub/tests/test_program_validation.py \
  apps/heiwa_hub/tests/test_llm_routing_external_consumers.py
git commit -m "feat: migrate remaining llm consumers"
```

### Task 5: Remove legacy routing code and finalize compatibility cleanup

**Files:**

- Modify: `packages/heiwa_cognition/heiwa_cognition/llm.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/__init__.py`
- Modify: `apps/heiwa_hub/cognition/llm_local.py`
- Modify: `apps/heiwa_hub/cognition/__init__.py`
- Modify: `packages/heiwa_cognition/CONTEXT.md`
- Create: `apps/heiwa_hub/tests/test_llm_routing_cleanup.py`

- [ ] **Step 1: Write the failing test**

Create `apps/heiwa_hub/tests/test_llm_routing_cleanup.py` to assert that the old routing surface is gone:

```python
def test_legacy_llm_routing_methods_are_removed():
    from heiwa_cognition.llm import LocalLLMEngine
    assert not hasattr(LocalLLMEngine, "generate_json")
    assert not hasattr(LocalLLMEngine, "generate_async")
    assert not hasattr(LocalLLMEngine, "_tier_chain")
```

Add a second assertion that the hub shim re-exports the new facade helpers.

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_cleanup.py -v`

Expected: FAIL until the legacy methods are removed and the shims are updated.

- [ ] **Step 3: Write minimal implementation**

Remove the legacy `LocalLLMEngine` routing methods, keep only the execution path, and update package / hub compatibility surfaces and docs to point at the facade family.

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest apps/heiwa_hub/tests/test_llm_routing_cleanup.py -v`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/llm.py \
  packages/heiwa_cognition/heiwa_cognition/__init__.py \
  apps/heiwa_hub/cognition/llm_local.py \
  apps/heiwa_hub/cognition/__init__.py \
  packages/heiwa_cognition/CONTEXT.md \
  apps/heiwa_hub/tests/test_llm_routing_cleanup.py
git commit -m "refactor: remove legacy llm routing path"
```

### Task 6: Run the regression sweep and close out

**Files:**

- Test: `apps/heiwa_hub/tests/test_llm_routing_unification.py`
- Test: `apps/heiwa_hub/tests/test_llm_routing_migration.py`
- Test: `apps/heiwa_hub/tests/test_llm_routing_external_consumers.py`
- Test: `apps/heiwa_hub/tests/test_llm_routing_cleanup.py`
- Test: `apps/heiwa_hub/tests/test_intent_classifier.py`
- Test: `apps/heiwa_hub/tests/test_chat_engine.py`
- Test: `apps/heiwa_hub/tests/test_heiwa_agent.py`
- Test: `apps/heiwa_hub/tests/test_program_validation.py`

- [ ] **Step 1: Run the focused regression suite**

Run:

```bash
uv run pytest \
  apps/heiwa_hub/tests/test_llm_routing_unification.py \
  apps/heiwa_hub/tests/test_llm_routing_migration.py \
  apps/heiwa_hub/tests/test_llm_routing_external_consumers.py \
  apps/heiwa_hub/tests/test_llm_routing_cleanup.py \
  apps/heiwa_hub/tests/test_intent_classifier.py \
  apps/heiwa_hub/tests/test_chat_engine.py \
  apps/heiwa_hub/tests/test_heiwa_agent.py \
  apps/heiwa_hub/tests/test_program_validation.py -v
```

Expected: PASS.

- [ ] **Step 2: Run a diff sanity check**

Run: `git diff --check`

Expected: no whitespace or patch-format errors.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/plans/2026-03-23-routing-unification.md
git commit -m "docs: add routing unification implementation plan"
```
