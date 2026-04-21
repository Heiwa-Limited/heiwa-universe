# Strict User Vault Routing (BYOK Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement strict per-user credential resolution in OpenClaw, ensuring all task executions use vault-backed keys and scrubbing system environment variables.

**Architecture:** Update ingress (mcp_server), enrichment (cognition), and execution (sdk/gateway) to propagate and honor `owner_id`. Implement a strict environment allow-list in ToolMesh and a relink path for admin identities in auth.

**Tech Stack:** Python, SpacetimeDB (STDB), FastAPI, Pydantic, Fernet encryption.

---

### Task 1: Protocol & Ingress Ownership

**Files:**
- Modify: `packages/heiwa_protocol/heiwa_protocol/routing.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Test: `tests/test_byok_routing.py`

- [ ] **Step 1: Update `BrokerRouteRequest` and `BrokerRouteResult` to make `owner_id` mandatory in `from_payload`.**

```python
# packages/heiwa_protocol/heiwa_protocol/routing.py

@dataclass(slots=True)
class BrokerRouteRequest:
    # ... existing fields ...
    owner_id: str  # Remove default "operator" to force awareness
    # ...
```

- [ ] **Step 2: Update `mcp_server.py` to pass `owner_id` from identity context to `BrokerRouteRequest`.**

```python
# apps/heiwa_hub/mcp_server.py:create_task

    broker_req = BrokerRouteRequest.from_payload({
        # ...
        "owner_id": identity["owner_id"],
        # ...
    })
```

- [ ] **Step 3: Create a basic regression test for ingress.**

```python
# tests/test_byok_routing.py

def test_mcp_task_ingress_carries_owner_id():
    # Mock auth and check BrokerRouteRequest construction
    pass
```

### Task 2: Identity Model & Relink Path

**Files:**
- Modify: `apps/heiwa_hub/auth.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/router.py`
- Modify: `packages/heiwa_cognition/heiwa_cognition/llm.py`

- [ ] **Step 1: Implement `is_system_operator(owner_id)` helper.**

```python
# packages/heiwa_protocol/heiwa_protocol/routing.py

def is_system_operator(owner_id: str) -> bool:
    return owner_id in {"operator", "local-operator", "0"}
```

- [ ] **Step 2: Update `router.py` and `llm.py` to use `is_system_operator`.**

- [ ] **Step 3: Implement Devon-to-"0" relink path in `auth.py`.**

```python
# apps/heiwa_hub/auth.py:ensure_user

    admin_mappings = _load_admin_mappings() # from HEIWA_ADMIN_ID_MAPPINGS
    target_user_id = admin_mappings.get(f"discord:{discord_uid}")
    
    # If it's Devon (target_user_id="0"), ensure oauth_identity points to "0"
    if target_user_id == "0":
        if existing and existing[0]["user_id"] != "0":
             stdb.call("update_oauth_identity_user", existing_oid, "0")
        return "0"
```

### Task 3: Environment Scrubbing

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/tool_mesh.py`

- [ ] **Step 1: Define `SAFE_ENV_ALLOWLIST`.**

```python
SAFE_ENV_ALLOWLIST = ['PATH', 'HOME', 'USER', 'LANG', 'LC_ALL', 'TERM', 'PYTHONPATH']
```

- [ ] **Step 2: Implement scrubbing in `ToolMesh.execute`.**

```python
# packages/heiwa_sdk/heiwa_sdk/tool_mesh.py

    def execute(self, tool: str, instruction: str, env: dict | None = None) -> tuple[int, str]:
        # ...
        full_env = {k: v for k, v in os.environ.items() if k in SAFE_ENV_ALLOWLIST}
        if env:
            full_env.update(env)
        # ...
```

### Task 4: BLOCKED_AUTH Propagation

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py`
- Modify: `apps/heiwa_hub/agents/heiwaclaw.py`

- [ ] **Step 1: Update `OpenClaw.execute` to detect auth failure.**

```python
# packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py

        if exit_code in {401, 403}: # or adapter-specific markers
             return exit_code, "BLOCKED_AUTH: Missing or invalid API key."
```

- [ ] **Step 2: Update `HeiwaClawAgent._handle_exec` to handle `BLOCKED_AUTH`.**

```python
# apps/heiwa_hub/agents/heiwaclaw.py

        if "BLOCKED_AUTH" in full_result:
             exec_status = "BLOCKED_AUTH"
```

### Task 5: Migration Script & Final Verification

- [ ] **Step 1: Create `scripts/migrate_personal_keys.py`.**
- [ ] **Step 2: Run migration for Devon's keys to `owner_id="0"`.**
- [ ] **Step 3: Run full regression suite `pytest tests/test_byok_routing.py`.**
