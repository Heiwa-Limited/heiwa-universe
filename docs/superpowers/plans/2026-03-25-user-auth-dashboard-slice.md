# User Auth Dashboard Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Hub-issued Discord OAuth JWT usable by real user-facing web surfaces by adding a post-login dashboard page and converting the first read-only dashboard APIs to user-scoped auth.

**Architecture:** Keep Hub-owned Discord OAuth and JWT issuance as-is, then thread the authenticated `user_id` from `require_user()` through the FastAPI routes into `HubStateService`, `Database`, and STDB query wrappers. Add a lightweight `dashboard.html` that captures the URL-fragment token, persists it in browser storage, and points the user to the first authenticated surfaces.

**Tech Stack:** FastAPI, static HTML/JS, Heiwa SDK state facade, SpacetimeDB SQL/reducers, pytest

---

### Task 1: Add failing coverage for the user-auth dashboard slice

**Files:**

- Create: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`

- [ ] **Step 1: Write the failing tests**

Add pytest coverage for:

- `GET /dashboard.html` returns `200`
- `GET /auth/me` accepts a valid JWT and returns the matching user
- `GET /auth/providers`, `GET /missions`, `GET /missions/{id}`, and `GET /history` reject missing JWTs
- The same endpoints accept a valid JWT and only return rows matching the authenticated `user_id`

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py -q`
Expected: FAIL because `dashboard.html` does not exist and the read-only endpoints still use `HEIWA_AUTH_TOKEN` instead of user JWT auth/scoping.

### Task 2: Add the dashboard landing page

**Files:**

- Create: `apps/heiwa_web/clients/web/dashboard.html`
- Modify: `apps/heiwa_web/clients/web/assets/operator.js`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`

- [ ] **Step 1: Add the minimal page**

Create `dashboard.html` using the existing static web styling. The page should:

- load `site.js` and `operator.js` so `#token=` is captured automatically
- present itself as the authenticated dashboard landing page
- link to `connections.html`, `missions.html`, and `history.html`
- include an authenticated mount target so the page can optionally show current user info from `/auth/me`

- [ ] **Step 2: Extend the shared browser JS minimally**

Update `operator.js` so `data-view="dashboard"` fetches `/auth/me` with the JWT and renders a compact signed-in state instead of falling through to an unknown view.

- [ ] **Step 3: Run the focused tests**

Run: `pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py -q`
Expected: still FAIL because the user-facing API routes are not JWT-scoped yet.

### Task 3: Convert the first read-only user-facing routes to JWT auth

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/state.py`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`

- [ ] **Step 1: Thread `user_id` through the route layer**

Update these endpoints in `mcp_server.py` to use `require_user(request)` instead of `_validate_auth_token(...)`:

- `/auth/providers`
- `/auth/providers/{provider_id}/status`
- `/missions`
- `/missions/{mission_id}`
- `/history`

Each route should extract `claims["sub"]` and pass it as `user_id` into the state layer. `GET /missions/{mission_id}` must return `404` when the mission exists for another user or is missing.

- [ ] **Step 2: Add user-scoped state facade methods**

Update `HubStateService` to accept `user_id` for:

- `get_provider_accounts`
- `get_provider_status`
- `get_missions`
- `get_mission_detail`
- `get_history`

Make `get_mission_detail` build a response only after the mission itself matches the caller’s `user_id`.

- [ ] **Step 3: Run the focused tests**

Run: `pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py -q`
Expected: still FAIL because the database/STDB facade methods still return unscoped data.

### Task 4: Add user-scoped database and STDB query filters

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/db.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`
- Test: `apps/heiwa_hub/tests/test_stdb_mission_facade.py`

- [ ] **Step 1: Extend the Database facade**

Add optional `user_id` parameters and delegate them through to STDB for:

- `get_mission`
- `get_missions`
- `get_runs`
- `list_provider_accounts`
- `list_session_summaries`
- `list_artifacts`
- `get_cell_runs`

- [ ] **Step 2: Extend the STDB query wrappers**

Add optional `user_id` filters to the corresponding STDB SQL helpers and include `user_id` in the selected columns where needed. Use the existing SQL literal escaping consistently for this slice.

- [ ] **Step 3: Add/adjust delegation assertions**

Update `test_stdb_mission_facade.py` or companion tests so the facade coverage proves `user_id` is passed through on the new method signatures.

- [ ] **Step 4: Run the focused tests**

Run: `pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_stdb_mission_facade.py -q`
Expected: PASS

### Task 5: Verify the auth slice end-to-end

**Files:**

- Modify: `apps/heiwa_hub/tests/test_mcp_server_surface.py` (only if existing surface coverage needs updates)
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`
- Test: `apps/heiwa_hub/tests/test_stdb_mission_facade.py`
- Test: `apps/heiwa_hub/tests/test_mcp_server_surface.py`

- [ ] **Step 1: Run the targeted suite**

Run: `pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_stdb_mission_facade.py apps/heiwa_hub/tests/test_mcp_server_surface.py -q`
Expected: PASS

- [ ] **Step 2: Sanity-check the static asset route**

Run: `python - <<'PY'
from fastapi.testclient import TestClient
from apps.heiwa_hub.mcp_server import app
client = TestClient(app)
resp = client.get('/dashboard.html')
print(resp.status_code)
print('token=' in resp.text)
PY`
Expected: `200` and `False`

- [ ] **Step 3: Document remaining follow-ups**

Record the known next steps:

- move user/profile lookup off raw SQL where practical
- encrypt stored Discord OAuth tokens in `link_oauth_identity`
- convert the next write-capable user workflows after the read-only slice is stable
