# BYOK Vault Routing Design

**Date:** 2026-03-29
**Status:** Draft
**Gate:** Required before scope enforcement flip (observe → enforce)

## Problem

Heiwa has four Class 3 CLI tools (Claude Code, Codex, Gemini CLI, Antigravity) running as separate sessions under Devon's OS user. Provider routing picks adapters from static `ai_router.json` config. API keys live in env vars (`GEMINI_API_KEY`, etc.) with no owner scoping.

This means:
- No path for a second user to bring their own credentials
- Devon's credentials are implicit (env vars, CLI sessions) instead of explicit owner-scoped records
- Routing can't prefer a user's own keys over fallback providers

## Goal

Owner-scoped credential storage and routing. Devon becomes `owner_id=user-<hash>` (resolved from Discord OAuth). His CLI tools and API keys are credentials in the vault. External users bring their own. The static registry becomes fallback for providers with no owner credential.

## Scope

All auth kinds:
- `api_key` — Gemini API, OpenRouter, Cerebras, Groq, SiliconFlow, Antigravity
- `oauth_cli` — Claude Code, Codex, Gemini CLI
- `local_runtime` — Ollama, vLLM (always available, no credential needed)

## Architecture

### Credential Storage

Two tiers:

1. **STDB `provider_credentials` table** (already exists) — encrypted credential rows keyed by `user_id` (to be treated as `owner_id`), `provider_id`, `credential_kind`, `status`. Used for future web-submitted keys.

2. **Railway env vars** — Devon's keys (`GEMINI_API_KEY`, `OPENAI_API_KEY`, etc.) and CLI tool availability. These are the bootstrap credentials for `owner_id=0` (first user). Not stored in STDB — read from env at runtime.

### Credential Resolution

New module: `packages/heiwa_sdk/heiwa_sdk/credential_resolver.py`

```
resolve_credential(owner_id, provider_id, credential_kind) → CredentialResult | None
```

Resolution order:
1. **STDB lookup** — `provider_credentials` where `user_id = owner_id` and `provider_id` matches and `status = 'active'`. Decrypt `credential_enc` via `InstanceVault`.
2. **Env var fallback** — for the operator's own keys. Map: `provider_id → env var name` (e.g., `"google-antigravity" → "GEMINI_API_KEY"`, `"openrouter" → "OPENROUTER_API_KEY"`). Only returns if caller is the operator owner (configurable via `HEIWA_OPERATOR_OWNER_ID`).
3. **CLI availability check** — for `oauth_cli` providers, check `shutil.which(cli_command)`. Returns a `CredentialResult` with `kind="cli_session"` and no secret material.
4. **None** — provider unavailable for this owner.

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

**Where it plugs in:** `OpenClaw.resolve()` in `gateway.py`

Currently:
```
route → provider_for(route) → ProviderRegistry.resolve(provider) → static ProviderConfig → OpenClawDispatch
```

After:
```
route → provider_for(route) → ProviderRegistry.resolve(provider) → credential_resolver.resolve(owner_id, provider) → OpenClawDispatch with adapter_env populated
```

Changes to `OpenClaw.resolve()`:
- Accept `owner_id` parameter
- Call `credential_resolver.resolve_credential(owner_id, provider, auth_kind)`
- If credential found: populate `adapter_env` with the secret (e.g., `{"GEMINI_API_KEY": "<decrypted>"}`) and set `auth_kind` from result
- If no credential: provider is unavailable for this owner → trigger cascade to next provider in rotation

Changes to `ComputeRouter`:
- Accept `owner_id` parameter in route selection
- Filter model tier candidates to providers where the owner has a credential
- This prevents routing to a provider the owner can't actually use

### Provider-to-Env Mapping

Static map in `credential_resolver.py`:

```python
PROVIDER_ENV_MAP = {
    "google-antigravity": "GEMINI_API_KEY",
    "siliconflow": "SILICONFLOW_API_KEY",
    "cerebras": "CEREBRAS_API_KEY",
    "openrouter": "OPENROUTER_API_KEY",
    "groq": "GROQ_API_KEY",
}

PROVIDER_CLI_MAP = {
    "claude-code": "claude",
    "anthropic": "claude",
    "codex": "codex",
    "google-gemini-cli": "gemini",
}
```

### STDB Bridge

New methods in `spacetimedb.py`:

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
- Encrypts `secret` via `InstanceVault`, stores in STDB
- Returns credential metadata (no secret)

**`GET /auth/credentials`** — list owner's credentials (no secrets returned)
- Auth: JWT required
- Returns: `[{ credential_id, provider_id, credential_kind, status, display_label, last_validated_at }]`

**`DELETE /auth/credentials/{credential_id}`** — revoke a credential
- Auth: JWT required
- Calls `revoke_provider_credential` reducer

### Task Flow (After)

```
CLI: heiwa "deploy the thing"
  → POST /tasks (with owner_id from JWT)
  → enrichment (ComputeRouter filters to owner's available providers)
  → OpenClaw.resolve(route, owner_id=owner_id)
  → credential_resolver.resolve_credential(owner_id, provider)
  → adapter gets env with decrypted key
  → execution
```

### Bootstrap: Devon as First User

On Railway deploy, these env vars exist:
- `HEIWA_MASTER_KEY` — encryption key for vault
- `HEIWA_OPERATOR_OWNER_ID` — Devon's owner_id (set after first Discord login, or `"local-operator"` as default)
- `GEMINI_API_KEY`, etc. — Devon's actual keys

The credential resolver checks `HEIWA_OPERATOR_OWNER_ID` to decide whether env-var fallback is allowed for a given `owner_id`. This means Devon's keys work immediately without STDB records, and external users must submit keys via the API.

### Security

- Secrets encrypted at rest via `InstanceVault` (Fernet, PBKDF2-derived from `HEIWA_MASTER_KEY`)
- Secrets never returned in API responses
- Secrets only decrypted at execution time, passed via `adapter_env` (in-memory, never logged)
- STDB `provider_credentials` table is `public` (STDB access control) but `credential_enc` is ciphertext
- Env var fallback restricted to operator owner only

### What Does NOT Change

- `ai_router.json` — stays as static provider metadata (adapter, transport, rate_group)
- `ProviderRegistry` — stays as config loader, no credential logic
- Local providers (Ollama, vLLM) — no credential needed, always available
- Internal bus paths — keep `local-operator` fallback (multi-tenant enforcement Phase B)

## Implementation Order

1. `credential_resolver.py` — new module with `resolve_credential()`, env map, CLI check
2. `spacetimedb.py` + `db.py` — STDB bridge for credential CRUD
3. `gateway.py` — `OpenClaw.resolve()` accepts `owner_id`, calls credential resolver, populates `adapter_env`
4. `router.py` — `ComputeRouter` filters providers by owner credential availability
5. `mcp_server.py` — credential HTTP endpoints + pass `owner_id` through task creation to OpenClaw
6. Tests — credential resolution, routing with/without credentials, endpoint auth scoping
