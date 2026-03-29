# BYOK Vault Routing Design

**Date:** 2026-03-29
**Status:** Draft (rev 2)
**Gate:** Required before scope enforcement flip (observe → enforce)

## Problem

Heiwa has four Class 3 CLI tools (Claude Code, Codex, Gemini CLI, Antigravity) running as separate sessions under Devon's OS user. Provider routing picks adapters from static `ai_router.json` config. API keys live in env vars (`GEMINI_API_KEY`, etc.) with no owner scoping.

Two execution paths exist:
1. **Task path** — `POST /tasks` → enrichment → `OpenClaw.resolve()` → adapter execution
2. **Lightweight inference path** — `llm_generate()` → `ComputeRouter.route_inference()` → `LocalLLMEngine.execute()` → direct env var reads (`self.gemini_key = os.getenv("GEMINI_API_KEY")`)

Both paths bypass owner scoping. The lightweight path is worse — it reads env keys directly in `LocalLLMEngine.__init__` and has its own hardcoded provider cascade in `_try_provider()`.

This means:
- No path for a second user to bring their own credentials
- Devon's credentials are implicit (env vars, CLI sessions) instead of explicit owner-scoped records
- Routing can't prefer a user's own keys over fallback providers
- Two independent provider cascades that will diverge

## Goal

Owner-scoped credential storage and routing across both execution paths. Devon becomes `owner_id=user-<hash>` (resolved from Discord OAuth). His CLI tools and API keys are credentials in the vault. External users bring their own. The static registry becomes fallback metadata only.

## Scope

All auth kinds:
- `api_key` — Gemini API, OpenRouter, Cerebras, Groq, SiliconFlow, Antigravity
- `oauth_cli` — Claude Code, Codex, Gemini CLI (operator-only — see Security)
- `local_runtime` — Ollama, vLLM (always available, no credential needed)

## Architecture

### Credential Storage

Two tiers:

1. **STDB `provider_credentials` table** (already exists) — encrypted credential rows keyed by `owner_id`, `provider_id`, `credential_kind`, `status`. The STDB column is currently named `user_id`. This tranche migrates the Python API surface to `owner_id` terminology; the STDB reducer accepts `user_id` as the parameter name but callers always pass the owner_id value. A future STDB schema migration can rename the column itself.

2. **Railway env vars** — Devon's keys (`GEMINI_API_KEY`, etc.) and CLI tool presence. These are the bootstrap credentials for the operator. Not stored in STDB — read from env at runtime, restricted to the operator owner only.

### Credential Resolution

New module: `packages/heiwa_sdk/heiwa_sdk/credential_resolver.py`

```
resolve_credential(owner_id, provider_id, auth_kind) → CredentialResult | None
```

Resolution order:
1. **STDB lookup** — `provider_credentials` where `owner_id` matches and `provider_id` matches and `status = 'active'`. Decrypt `credential_enc` via `InstanceVault`. Available to all owners.
2. **Env var fallback** — map `provider_id → env var name`. **Operator-only**: returns only if `owner_id == HEIWA_OPERATOR_OWNER_ID`.
3. **CLI session fallback** — for `oauth_cli` providers, check `shutil.which(cli_command)`. **Operator-only**: returns only if `owner_id == HEIWA_OPERATOR_OWNER_ID`. CLI tools run under the operator's OS session — exposing them to other tenants is a cross-tenant auth bypass.
4. **None** — provider unavailable for this owner. Routing must cascade to another provider or fail.

Non-operator users who want CLI-backed providers must submit their own API keys for those providers' API endpoints (e.g., Anthropic API key instead of Claude Code CLI, OpenAI API key instead of Codex CLI). The `oauth_cli` auth kind is never resolved for non-operator owners.

```python
@dataclass
class CredentialResult:
    provider_id: str
    credential_kind: str  # "api_key", "cli_session", "oauth_token"
    secret: str | None     # decrypted key (None for cli_session)
    rate_group: str
    source: str            # "stdb", "env", "cli"
```

### Routing Integration

**Single routing authority: `ComputeRouter`**

`ComputeRouter` is the sole authority for provider selection and cascade. `OpenClaw` binds auth material for the already-chosen provider — it does not cascade independently.

**Current flow (task path):**
```
POST /tasks → enrichment(ComputeRouter) → OpenClaw.resolve(route) → adapter
```

**Current flow (lightweight inference path):**
```
llm_generate() → ComputeRouter.route_inference() → LocalLLMEngine.execute()
                                                     ↳ reads GEMINI_API_KEY from env directly
                                                     ↳ has its own _try_provider() cascade
```

**After (both paths):**
```
ComputeRouter.route_inference(owner_id=...) → filters to credential-eligible providers
  → task path: OpenClaw.resolve(route, owner_id) → credential_resolver → adapter_env populated
  → inference path: LocalLLMEngine.execute(target, credential=...) → uses provided credential
```

#### Changes to `ComputeRouter`

- `route_inference()` and `_ranked_tier_candidates()` accept `owner_id` parameter
- Before ranking candidates, call `credential_resolver.list_available_providers(owner_id)` to get the set of providers this owner can actually use
- Filter model tier candidates to only those providers
- Provider rotation (`_rotation`, `_intent_rotations`) skips providers the owner lacks credentials for
- This is where cascade happens — not in `OpenClaw`, not in `LocalLLMEngine`

```python
def list_available_providers(owner_id: str) -> set[str]:
    """Return provider_ids where this owner has at least one active credential."""
```

#### Changes to `OpenClaw.resolve()`

- Accept `owner_id` parameter
- Call `credential_resolver.resolve_credential(owner_id, provider, auth_kind)`
- If credential found: populate `adapter_env` with the secret (e.g., `{"GEMINI_API_KEY": "<decrypted>"}`)
- If no credential: return dispatch with empty `adapter_env` and `auth_kind="none"` — the adapter will fail gracefully. This should not happen if `ComputeRouter` did its job, but defense in depth.
- No cascade logic. OpenClaw binds, it does not choose.

#### Changes to `LocalLLMEngine`

- Remove `self.gemini_key = os.getenv("GEMINI_API_KEY")` from `__init__`
- Remove the hardcoded provider cascade in `_try_provider()`
- `execute()` accepts an optional `credential: CredentialResult` parameter
- For Gemini API calls: use `credential.secret` instead of `self.gemini_key`
- For CLI calls: only proceed if `credential.credential_kind == "cli_session"`
- For Ollama: no credential needed (local_runtime)

#### Changes to `llm_generate_with_plan()`

- Accept `owner_id` parameter (default: `HEIWA_OPERATOR_OWNER_ID` for backward compat with internal callers)
- Pass `owner_id` to `ComputeRouter.route_inference()`
- For each target in the plan, resolve credential via `credential_resolver.resolve_credential()`
- Pass credential to `engine.execute(target, prompt, credential=credential)`

### Provider-to-Env Mapping

Static map in `credential_resolver.py`:

```python
PROVIDER_ENV_MAP: dict[str, str] = {
    "google-antigravity": "GEMINI_API_KEY",
    "siliconflow": "SILICONFLOW_API_KEY",
    "cerebras": "CEREBRAS_API_KEY",
    "openrouter": "OPENROUTER_API_KEY",
    "groq": "GROQ_API_KEY",
}

PROVIDER_CLI_MAP: dict[str, str] = {
    "claude-code": "claude",
    "anthropic": "claude",
    "codex": "codex",
    "google-gemini-cli": "gemini",
}
```

### STDB Bridge

New methods in `spacetimedb.py` (using `owner_id` in the Python API, passing as `user_id` to the STDB reducer):

```python
def list_provider_credentials(self, owner_id: str, provider_id: str | None = None, status: str = "active") -> list[dict]
def store_provider_credential(self, credential_id, owner_id, provider_id, credential_kind, credential_enc, rate_group, display_label=None) -> bool
def revoke_provider_credential(self, credential_id) -> bool
```

New methods in `db.py` (forwarding):

```python
def list_owner_credentials(self, owner_id: str, provider_id: str | None = None) -> list[dict]
def store_owner_credential(self, credential_data: dict) -> bool
def revoke_owner_credential(self, credential_id: str) -> bool
```

### HTTP Endpoints

**`POST /auth/credentials`** — store a new credential (encrypted)
- Auth: JWT required
- Body: `{ provider_id, credential_kind, secret, display_label? }`
- Encrypts `secret` via `InstanceVault`, stores in STDB with owner_id from JWT
- Returns credential metadata (no secret)

**`GET /auth/credentials`** — list owner's credentials (no secrets returned)
- Auth: JWT required
- Returns: `[{ credential_id, provider_id, credential_kind, status, display_label, last_validated_at }]`

**`DELETE /auth/credentials/{credential_id}`** — revoke a credential
- Auth: JWT required
- Verifies credential belongs to the authenticated owner before revoking
- Calls `revoke_provider_credential` reducer

### Task Flow (After)

```
CLI: heiwa "deploy the thing"
  → POST /tasks (with owner_id from JWT)
  → enrichment: ComputeRouter.route(owner_id=...) filters to owner's available providers
  → Spine dispatches with owner_id on bus message
  → OpenClaw.resolve(route, owner_id=...) → credential_resolver → adapter_env populated
  → adapter executes with owner's credential
```

```
Internal: llm_generate("classify this", intent="classification")
  → ComputeRouter.route_inference(owner_id=operator) filters to operator's providers
  → LocalLLMEngine.execute(target, credential=resolved_cred)
  → uses credential.secret for API call or credential.kind for CLI dispatch
```

### Bootstrap: Devon as First User

On Railway deploy, these env vars exist:
- `HEIWA_MASTER_KEY` — encryption key for vault
- `HEIWA_OPERATOR_OWNER_ID` — Devon's owner_id (set after first Discord login, or `"local-operator"` as default)
- `GEMINI_API_KEY`, etc. — Devon's actual API keys
- CLI tools installed in Docker image (claude, codex, gemini)

The credential resolver checks `owner_id == HEIWA_OPERATOR_OWNER_ID` for env var and CLI fallback. Devon's keys work immediately without STDB records. External users must submit keys via `POST /auth/credentials`.

### Security

- **Cross-tenant isolation**: env var fallback and CLI session resolution are operator-only. Non-operator owners can only use STDB-stored credentials scoped to their own `owner_id`.
- **Secrets encrypted at rest** via `InstanceVault` (Fernet, PBKDF2-derived from `HEIWA_MASTER_KEY`)
- **Secrets never returned** in API responses
- **Secrets only decrypted** at execution time, passed via `adapter_env` (in-memory, never logged)
- STDB `provider_credentials.credential_enc` is always ciphertext
- `DELETE /auth/credentials/{id}` verifies owner_id match before revocation

### Naming: `user_id` → `owner_id` Migration

The STDB `provider_credentials` table column remains `user_id` (Rust schema). The Python API surface uses `owner_id` exclusively:
- `credential_resolver.py` — all methods use `owner_id`
- `spacetimedb.py` bridge methods accept `owner_id`, pass as `user_id` to STDB reducers
- `db.py` forwarding methods use `owner_id`
- HTTP endpoints use `owner_id` from `resolve_identity_context()`

A future STDB schema migration can rename the column. This tranche does not touch the Rust module.

### What Does NOT Change

- `ai_router.json` — stays as static provider metadata (adapter, transport, rate_group, auth_kind)
- `ProviderRegistry` — stays as config loader, no credential logic
- Local providers (Ollama, vLLM) — no credential needed, always available
- Internal bus paths — keep `local-operator` fallback (multi-tenant enforcement Phase B)
- STDB Rust schema — column stays `user_id`, Python callers pass `owner_id` value

## Implementation Order

1. `credential_resolver.py` — new module: `resolve_credential()`, `list_available_providers()`, env/CLI maps, operator guard
2. `spacetimedb.py` + `db.py` — STDB bridge for credential CRUD (owner_id terminology)
3. `router.py` — `ComputeRouter` accepts `owner_id`, filters providers by credential availability (single cascade authority)
4. `llm.py` — `LocalLLMEngine` stops reading env keys directly; `execute()` accepts credential; `llm_generate_with_plan()` accepts `owner_id` and resolves credentials per target
5. `gateway.py` — `OpenClaw.resolve()` accepts `owner_id`, calls credential resolver, populates `adapter_env` (no cascade)
6. `mcp_server.py` — credential HTTP endpoints + pass `owner_id` through task enrichment and dispatch to OpenClaw
7. Tests — credential resolution (operator vs non-operator), routing with/without credentials, cross-tenant isolation, endpoint auth scoping, lightweight inference path with credentials
