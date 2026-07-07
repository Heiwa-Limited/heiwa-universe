# Design: Strict User Vault Routing (BYOK Phase 1)

## Status: Revised (2026-03-29) - Finalizing Gaps (V4)

## 1. Overview

Implement strict per-user credential resolution in `OpenClaw`. All task executions will resolve their API keys through the `UserVault` in SpacetimeDB. The system will no longer fallback to environment variables for model providers, except for local providers (e.g., Ollama) and the canonical system operator.

## 2. Goals

- Enforce user-scoped credentials for all model provider requests.
- Prevent cross-user credential access.
- **`[P1]`** Ensure `owner_id` is propagated correctly from ingress through enrichment to execution.
- **`[P1]`** Scrub system environment variables from tool execution to prevent key leakage.
- **`[P1]`** Define an explicit identity mapping for the primary human operator (Devon) to `owner_id="0"`, covering both new and existing users.
- **`[P2]`** Unify the identity model for "system operator" across the stack.
- **`[P2]`** Implement structured `BLOCKED_AUTH` status propagation.
- Migrate current "personal" keys to `owner_id="0"`.

## 3. Architecture

### 3.1 Ingress & Enrichment (`[P1]`)

- **`mcp_server.py`**: Update `POST /tasks` to pass `owner_id` into `BrokerRouteRequest.from_payload`.
- **`BrokerRouteRequest` / `BrokerRouteResult`**: Ensure `owner_id` is mandatory and correctly serialized.
- **`ComputeRouter.route`**: Must receive `owner_id` to determine provider availability (local vs. vault-backed).
- **`BrokerEnrichmentService.enrich`**: Ensure `owner_id` is passed to both `IntentNormalizer` and `ComputeRouter`.

### 3.2 Environment Scrubbing & Runtime Bypass (`[P1]`)

- **`ToolMesh.execute`**: Replace `os.environ.copy()` with a strict allow-list of safe variables (`PATH`, `HOME`, `USER`, `LANG`, `LC_ALL`, `TERM`).
- **Vault Security**: `HEIWA_MASTER_KEY` will **NOT** be passed to child processes. The hub resolves credentials and injects only the specific provider key (e.g., `ANTHROPIC_API_KEY`) into the adapter environment.
- **`ReflexAdapter` / `LocalLLMEngine`**: Ensure `LocalLLMEngine.execute` uses `owner_id` to resolve keys and does NOT fall back to environment variables for non-operator users.
- **`LocalLLMEngine._resolve_api_key`**: Ensure it strictly enforces `UserVault` for all non-operator IDs.

### 3.3 Identity Model & Devon's Mapping (`[P1]`)

- **Canonical System Operator**: Define `operator` and `local-operator` as equivalent "system" IDs.
- **Identity Mapping**: Introduce `HEIWA_ADMIN_ID_MAPPINGS` (e.g., `discord:123456789=0`).
- **`auth.py:ensure_user`**:
  - **New User**: If the provider ID (e.g., Discord UID) matches an admin mapping, set the `user_id` to `"0"`.
  - **Existing User (Relink)**: If the provider ID matches an admin mapping, but the current `oauth_identity` is linked to a non-zero `user_id`, update the `oauth_identity` row to point to `user_id="0"`.
- **`owner_id="0"`**: Reserved for the primary human operator (Devon), which will hold the migrated personal keys in the `UserVault`.
- **Update `router.py` and `llm.py`**: Change `if owner_id == "operator":` to a helper `is_system_operator(owner_id)` that covers `operator`, `local-operator`, and `"0"`.

### 3.4 `BLOCKED_AUTH` Implementation (`[P2]`)

- **Status Detection**: `OpenClaw.execute` (or the adapters) should detect authentication failures (e.g., 401/403 or specific CLI exit codes).
- **Status Propagation**: Map these failures to a new `BLOCKED_AUTH` state in the `BrokerRouteResult` or task result payload.
- **`HeiwaClawAgent`**: Update result handling to narrate `BLOCKED_AUTH` specifically ("Your vault key for <provider> is missing or invalid") instead of a generic "FAIL".

## 4. Migration Strategy

- Store current environment keys (e.g., `GOOGLE_API_KEY`, `ANTHROPIC_API_KEY`) into STDB `provider_credentials` for `user_id="0"`.
- This ensures that even Devon's "local" runs use the vault path.

## 5. Testing Strategy

- **`test_byok_routing.py`**:
  - **Ingress Test**: Verify `POST /tasks` with user identity results in a route that uses user keys.
  - **Isolation Test**: User A's key vs. User B's missing key.
  - **Scrubbing Test**: Verify a tool cannot see `RAILWAY_AUTH_TOKEN` or `HEIWA_MASTER_KEY`.
  - **Operator Test**: Verify `local-operator` and `"0"` still have access to system keys (until fully migrated).
  - **Relink Test**: Verify an existing user matches an admin mapping and gets relinked to `"0"`.

## 6. Security

- `InstanceVault` (Fernet encryption) handles the `credential_enc` field in STDB.
- `HEIWA_MASTER_KEY` must be present in the environment but never exposed to child processes.
- **Allow-list for ToolMesh**: `['PATH', 'HOME', 'USER', 'LANG', 'LC_ALL', 'TERM']`.
