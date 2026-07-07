# E2E Fix-Forward Design

**Date:** 2026-03-31
**Status:** Approved
**Approach:** Fix-Forward — wire existing infrastructure, no new subsystems

## Context

Heiwa's target user E2E: Discord OAuth signup, BYOK providers, 24/7 Discord agent + terminal REPL. Research reveals 80%+ of the infrastructure already exists. This spec wires the pieces together and fixes 3 known bugs/gaps.

## Architectural Razor

_Works for one, doesn't block N._

Every decision here must work for Devon's single-operator proving ground AND not block multi-operator scaling.

## Existing Infrastructure (No Changes Needed)

| Component            | Location                    | State                                             |
| -------------------- | --------------------------- | ------------------------------------------------- |
| Discord OAuth2 + JWT | `auth.py`                   | Full flow implemented                             |
| `ensure_user()`      | `auth.py:240`               | Creates `users` + `oauth_identities` rows         |
| BYOK vault           | `vault.py`, `mcp_server.py` | Fernet encryption, per-user STDB storage          |
| ChatEngine           | `chat.py`                   | In-memory sessions, 20 msg limit                  |
| AgentMemory          | `agent_memory.py`           | STDB persistent, rolling compression at 8K tokens |
| Terminal REPL        | `heiwa_cli/__main__.py`     | Interactive mode, auth commands                   |
| Capability dispatch  | `transport.py`, `spine.py`  | Dynamic GPU detection, set intersection matching  |

## Changes Required

### Step 1: Identity Resolution (DM -> Real User)

**File:** `apps/heiwa_hub/agents/messenger.py`

**Problem:** DM handler uses synthetic IDs (`discord-{author.id}`) not linked to the `users` table. The OAuth flow creates real users via `ensure_user()`; DMs don't.

**Fix:** Call `ensure_user()` in the DM fast path. When a Discord user DMs for the first time, they get a real `user_id` — same identity they'd get through OAuth.

```python
# Before
owner_id = f"discord-{author.id}"
principal_id = f"discord-user-{author.id}"

# After
from heiwa_hub.auth import ensure_user
discord_data = {
    "discord_user_id": str(author.id),
    "username": str(author),
}
user_id = ensure_user(self.db.stdb, discord_data)
owner_id = user_id
principal_id = user_id
```

No OAuth token is available from DMs. `ensure_user()` already handles this — it creates the user with whatever data is available and links the full OAuth identity later when the user completes the web flow.

`_track_identity()` remains as fire-and-forget for the legacy `upsert_discord_user` tracking table. No migration needed — both paths coexist.

### Step 2: Project Table

**File:** STDB schema + `auth.py`

**Problem:** No `projects` table. The E2E flow requires a project as the organizational unit.

**Fix:** Add `projects` table to STDB:

- `project_id: String` (PK)
- `owner_id: String` (FK to users)
- `name: String`
- `created_at: u64`
- `settings_json: String`

Auto-create a default project in `ensure_user()` when creating a new user:

```python
project_id = f"proj-{uuid.uuid4().hex[:12]}"
stdb.call("create_project", project_id, user_id, "default", "{}")
```

### Step 3: Session Fix

**File:** `apps/heiwa_hub/agents/messenger.py:308`

**Problem:** `session_id = f"discord-session-{author.id}-{int(time.time())}"` — timestamp makes every message a new session, breaking conversation context.

**Fix:**

```python
# Before
session_id = f"discord-session-{author.id}-{int(time.time())}"

# After
session_id = f"discord-dm-{author.id}"
```

Both context layers start working correctly:

- ChatEngine: in-memory history persists across messages (20 msg limit, last 10 for LLM prompt)
- AgentMemory: STDB rolling compression accumulates real conversation history

**Ordering rationale:** This comes AFTER identity resolution so the stable session attaches to a real `user_id` from the start. No orphaned synthetic records to migrate.

### Step 4: Per-User Isolation Verification

**Files:** ChatEngine, AgentMemory, HeiwaClaw dispatch path

**Problem:** Session isolation depends on `owner_id` being a real `user_id`. After Steps 1-3, the IDs are real but we need to verify the downstream chain scopes correctly.

**Fix:** Thread `owner_id` verification through:

1. ChatEngine session lookup — already keyed by `session_id`, which is now per-user stable
2. AgentMemory STDB queries — already accept `owner_id` parameter
3. BYOK vault resolution — `UserVault` already scoped by `user_id`

Add integration test: two synthetic users DM simultaneously, verify their sessions, memories, and vault lookups don't cross.

### Step 5: BYOK Routing via DM

**File:** `apps/heiwa_hub/agents/messenger.py`

**Problem:** No way to manage providers from Discord DMs.

**Fix:** Add DM command handler for `/providers` or `!providers`:

1. Query `UserVault.list_credentials(user_id)` for current provider status
2. Return status summary: which providers are configured, which are available
3. Route to setup: "Run `heiwa auth <provider>` in your terminal" or future web magic link

**Security constraint:** NEVER accept API keys in Discord DMs. Discord logs all messages on their servers. The chat interface is for execution and status, not credential injection. Keys go through:

- Terminal: `heiwa auth <provider>` (secure local input)
- Web (future): single-use HTTPS magic link to Railway control plane

## Security Boundaries

- API keys never transit Discord — terminal or HTTPS only
- Per-user STDB scoping via `owner_id` on all tables
- BYOK vault encryption via Fernet (`HEIWA_MASTER_KEY`)
- Untrusted code execution in E2B sandboxes only
- JWT session auth for web flow, HEIWA_AUTH_TOKEN for operator CLI

## What This Does NOT Include

- Web dashboard (`app.heiwa.ltd`) — next milestone after DM + REPL proves out
- New auth middleware — existing `ensure_user()` + JWT flow sufficient
- External message brokers — STDB proposal/lease state machine is the queue
- Mesh VPNs — `/ws/worker` outbound dial handles all networks

## Success Criteria

1. Devon DMs Heiwa on Discord, gets a real `user_id` in STDB `users` table
2. Conversation context persists across DM messages (no session reset)
3. `!providers` in DMs shows provider status and routes to terminal auth
4. Two simulated users can DM simultaneously without session cross-contamination
5. `heiwa` terminal REPL uses the same `user_id` for the operator
