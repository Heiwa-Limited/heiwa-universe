# Heiwa Product Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved Hub-first Heiwa architecture by adding owner/principal identity, Wire event transport, Battlefield registration, Node/Executor layering, Captain/Pulse refactors, and single-owner deployment packaging.

**Architecture:** This spec is too broad for one giant execution batch, so the plan is decomposed into four shippable tracks: substrate contracts, client/Wire transport, control-plane refactor, and deployment/docs. The implementation keeps STDB authoritative, treats Sessions as transport only, makes Missions the durable execution unit, and separates runtime Nodes from provider/model Executors.

**Tech Stack:** Rust (SpacetimeDB module), Python 3.11+ (FastAPI, asyncio, CLI packages), WebSockets/HTTP, Railway, Discord OAuth, CLI tool wrappers

**Spec:** `docs/superpowers/specs/2026-03-28-heiwa-product-architecture-design.md`

---

## Scope and sequencing

This approved spec spans multiple subsystems. Instead of one monolithic plan, this document is organized into four independently verifiable tracks that should still be executed in order:

1. **Track A: Substrate contracts** — identity, events, battlefields, STDB bridge
2. **Track B: Client transport** — Wire endpoint, replay/catch-up, CLI battlefield client
3. **Track C: Control plane** — Node/Executor model, Captain merge, Pulse extraction, Discord adapter
4. **Track D: Vault, packaging, docs** — BYOK plumbing, Railway template, companion-doc updates

Each track should leave the repo in a working, testable state before the next begins.

## File Structure

### New Files
| File | Responsibility |
|------|----------------|
| `apps/heiwa_hub/agents/captain.py` | Unified Captain runtime replacing Spine + HeiwaClaw behavior |
| `apps/heiwa_hub/agents/pulse.py` | Pulse subsystem replacing Telemetry agent responsibilities |
| `apps/heiwa_hub/adapters/discord.py` | Discord client adapter over Wire and REST approval endpoints |
| `packages/heiwa_cli/heiwa_cli/wire.py` | Shared CLI Wire client: connect/auth/catch-up/battlefield registration |
| `apps/heiwa_hub/tests/test_wire_client_protocol.py` | Wire replay, auth, catch-up, battlefield registration integration tests |
| `apps/heiwa_hub/tests/test_stdb_wire_events.py` | STDB event append/replay tests |
| `apps/heiwa_hub/tests/test_stdb_battlefields.py` | Battlefield lifecycle tests |
| `apps/heiwa_hub/tests/test_captain_runtime.py` | Captain event-loop and lease/mission persistence tests |

### Modified Files
| File | Changes |
|------|---------|
| `apps/heiwa_hub/spacetimedb/src/lib.rs` | Add `owner_id`/`principal_id`, `events`, `battlefields`, and Executor-related contract updates |
| `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` | Add bridge methods for new reducers/queries; migrate `user_id` calls to `owner_id`/`principal_id` |
| `packages/heiwa_sdk/heiwa_sdk/db.py` | Expose new identity-aware helpers, Wire event persistence, battlefield accessors |
| `packages/heiwa_sdk/heiwa_sdk/state.py` | Identity-aware reads for missions/history/providers and new battlefield/event lookups |
| `packages/heiwa_sdk/heiwa_sdk/main.py` | Align REST control endpoints with owner/principal and Node/Executor vocabulary |
| `packages/heiwa_sdk/heiwa_sdk/provider_registry.py` | Surface Executor metadata separately from provider accounts |
| `packages/heiwa_sdk/heiwa_sdk/tool_mesh.py` | Execute against Executors hosted on Nodes |
| `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py` | Resolve routes to Executors, not Nodes |
| `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py` | Sync Node and Executor state into STDB/Pulse |
| `packages/heiwa_sdk/heiwa_sdk/hooks.py` | Continue lease enforcement against mission-owned leases |
| `packages/heiwa_sdk/heiwa_sdk/vault.py` | Hub-owned BYOK handling and Railway sync refinements |
| `apps/heiwa_hub/mcp_server.py` | Add `/ws/client`, event replay, battlefield/session plumbing, owner/principal extraction |
| `apps/heiwa_hub/auth.py` | Make owner/principal identity explicit in auth/session claims |
| `apps/heiwa_hub/main.py` | Boot Captain + Pulse + Discord adapter instead of Spine/HeiwaClaw/Telemetry/Messenger |
| `apps/heiwa_hub/transport.py` | Runtime Node registration + capability advertisement feeding Executor selection |
| `apps/heiwa_hub/agents/spine.py` | Convert to compatibility shim or retire after Captain lands |
| `apps/heiwa_hub/agents/heiwaclaw.py` | Convert to compatibility shim or retire after Captain lands |
| `apps/heiwa_hub/agents/telemetry.py` | Convert to compatibility shim or retire after Pulse lands |
| `apps/heiwa_hub/agents/messenger.py` | Convert to compatibility shim or retire after Discord adapter lands |
| `packages/heiwa_cli/heiwa_cli/context.py` | Track Session/Battlefield identity and hub auth context |
| `packages/heiwa_cli/heiwa_cli/repl.py` | Use Wire stream and battlefield registration instead of ad hoc task-only hub calls |
| `packages/heiwa_cli/heiwa_cli/oneshot.py` | Submit via mission/session contract and stream via Wire |
| `packages/heiwa_cli/heiwa_cli/auth.py` | Align provider status with owner/principal and Vault-backed credentials |
| `packages/heiwa_protocol/heiwa_protocol/protocol.py` | Update topology language if required by Captain/Pulse/Discord adapter merge |
| `apps/heiwa_hub/tests/test_stdb_user_scoping.py` | Replace `user_id` assertions with owner/principal coverage |
| `apps/heiwa_hub/tests/test_stdb_option_encoding.py` | Cover new bridge argument ordering and optional identity fields |
| `apps/heiwa_hub/tests/test_mcp_server_surface.py` | Add `/ws/client` and replay endpoint coverage |
| `apps/heiwa_hub/tests/test_user_auth_dashboard.py` | Cover owner/principal claim extraction and provider filtering |
| `apps/heiwa_hub/tests/test_rate_group_routing.py` | Assert Executor-first routing semantics |
| `apps/heiwa_hub/tests/test_reactive_assignment.py` | Assert Node reachability vs Executor capacity selection |
| `apps/heiwa_hub/tests/test_discord_smoke_imports.py` | Point imports at adapter module or compatibility shim |
| `apps/heiwa_hub/tests/test_discord_smoke_payload.py` | Assert Discord adapter event rendering against Wire events |
| `config/swarm/END_STATE_2026-03.md` | Update product topology to Captain/Pulse/Executor/Hub-first model |
| `ops/context/HEIWA.md` | Update operator guidance and routing assumptions |
| `CLAUDE.md` | Update agent inventory and Railway-primary operating model |
| `apps/heiwa_hub/Dockerfile` | Boot new runtime modules and required env defaults |
| `railway.toml` | Reflect deployment/runtime assumptions from the new architecture |
| `infra/cloud/railway/README.md` | Document template boot and owner-provided infra model |

---

## Track A: Substrate Contracts

### Task 1: Migrate identity from `user_id` to `owner_id` + `principal_id`

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/db.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/state.py`
- Modify: `apps/heiwa_hub/auth.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Test: `apps/heiwa_hub/tests/test_stdb_user_scoping.py`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`
- Test: `apps/heiwa_hub/tests/test_stdb_option_encoding.py`

- [ ] **Step 1: Write failing identity tests**

```python
def test_owner_filter_and_principal_attribution(stdb):
    mission = stdb.create_mission({
        "mission_id": "m1",
        "owner_id": "owner-devon",
        "principal_id": "discord:123",
        "source_surface": "discord",
        "node_id": "railway-hub",
        "prompt": "status",
        "intent_class": "inspect",
        "risk_level": "low",
    })
    row = stdb.get_mission("m1", owner_id="owner-devon")
    assert row["owner_id"] == "owner-devon"
    assert row["principal_id"] == "discord:123"
```

- [ ] **Step 2: Run the focused failing tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_stdb_user_scoping.py apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_stdb_option_encoding.py -q`
Expected: FAIL on missing `owner_id` / `principal_id` fields or stale `user_id` assertions

- [ ] **Step 3: Add `owner_id` and `principal_id` to every tenant-scoped STDB table**

Use the same pattern across `MissionRecord`, `MissionStep`, `RunRecord`, `ArtifactRecord`, `Proposal`, `RouteDecision`, `SessionSummary`, `ProviderAccount`, and `CapabilityLease`:

```rust
#[default(None::<String>)]
#[index(btree)]
pub owner_id: Option<String>,
#[default(None::<String>)]
#[index(btree)]
pub principal_id: Option<String>,
```

- [ ] **Step 4: Update STDB reducers and SQL helpers to stamp/filter on the new fields**

```python
def get_mission(self, mission_id: str, owner_id: str | None = None) -> dict[str, Any] | None:
    clauses = [f"mission_id = '{self._escape_sql_literal(mission_id)}'"]
    if owner_id:
        clauses.append(f"owner_id = '{self._escape_sql_literal(owner_id)}'")
```

- [ ] **Step 5: Introduce auth-context helpers that always produce both IDs**

Add a small helper in `apps/heiwa_hub/auth.py` / `apps/heiwa_hub/mcp_server.py` with this contract:

```python
def resolve_identity_context(claims: dict[str, Any], *, autonomous: bool = False) -> dict[str, str]:
    owner_id = str(claims["owner_id"])
    principal_id = "captain" if autonomous else str(claims["principal_id"])
    return {"owner_id": owner_id, "principal_id": principal_id}
```

- [ ] **Step 6: Replace the remaining hub API `user_id` query/filter usage**

Touch `/auth/me`, provider status routes, mission/history routes, and any state facade that currently threads `user_id`.

- [ ] **Step 7: Re-run the focused identity tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_stdb_user_scoping.py apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_stdb_option_encoding.py -q`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs \
  packages/heiwa_sdk/heiwa_sdk/spacetimedb.py \
  packages/heiwa_sdk/heiwa_sdk/db.py \
  packages/heiwa_sdk/heiwa_sdk/state.py \
  apps/heiwa_hub/auth.py \
  apps/heiwa_hub/mcp_server.py \
  apps/heiwa_hub/tests/test_stdb_user_scoping.py \
  apps/heiwa_hub/tests/test_user_auth_dashboard.py \
  apps/heiwa_hub/tests/test_stdb_option_encoding.py
git commit -m "feat(identity): split owner and principal scoping"
```

### Task 2: Add event log and battlefield contracts to STDB + SDK

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/db.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/state.py`
- Test: `apps/heiwa_hub/tests/test_stdb_wire_events.py`
- Test: `apps/heiwa_hub/tests/test_stdb_battlefields.py`

- [ ] **Step 1: Write failing event and battlefield tests**

```python
def test_replay_events_after_event_id(stdb):
    stdb.append_event({"event_id": "e1", "owner_id": "owner-devon", "event_type": "mission_created"})
    stdb.append_event({"event_id": "e2", "owner_id": "owner-devon", "event_type": "task_started"})
    rows = stdb.list_events(after_event_id="e1", owner_id="owner-devon")
    assert [row["event_id"] for row in rows] == ["e2"]

def test_register_and_reattach_battlefield(stdb):
    row = stdb.upsert_battlefield({
        "battlefield_id": "bf-1",
        "owner_id": "owner-devon",
        "principal_id": "cli",
        "name": "heiwa",
        "repo_url": "git@github.com:dev/heiwa.git",
        "root_path": "/workspace/heiwa",
        "node_id": "railway-hub",
        "status": "active",
    })
    assert row["battlefield_id"] == "bf-1"
```

- [ ] **Step 2: Run the new failing tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_stdb_wire_events.py apps/heiwa_hub/tests/test_stdb_battlefields.py -q`
Expected: FAIL on missing tables/reducers/helpers

- [ ] **Step 3: Add `events` and `battlefields` STDB tables plus reducers**

Implement minimal append/list semantics and battlefield upsert/archive:

```rust
#[table(accessor = events, public)]
pub struct EventRecord { /* event_id, owner_id, principal_id, session_id, mission_id, battlefield_id, event_type, payload_json, timestamp */ }

#[table(accessor = battlefields, public)]
pub struct BattlefieldRecord { /* battlefield_id, owner_id, principal_id, name, repo_url, root_path, node_id, status, created_at, last_active_at */ }
```

- [ ] **Step 4: Add Python bridge/db/state helpers**

Required methods:
- `append_event(...)`
- `list_events(after_event_id=None, owner_id=..., limit=...)`
- `upsert_battlefield(...)`
- `list_battlefields(owner_id=...)`
- `get_battlefield(battlefield_id, owner_id=...)`

- [ ] **Step 5: Verify Rust schema builds cleanly**

Run: `cd apps/heiwa_hub/spacetimedb && spacetime build`
Expected: Build succeeds

- [ ] **Step 6: Re-run event/battlefield tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_stdb_wire_events.py apps/heiwa_hub/tests/test_stdb_battlefields.py -q`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs \
  packages/heiwa_sdk/heiwa_sdk/spacetimedb.py \
  packages/heiwa_sdk/heiwa_sdk/db.py \
  packages/heiwa_sdk/heiwa_sdk/state.py \
  apps/heiwa_hub/tests/test_stdb_wire_events.py \
  apps/heiwa_hub/tests/test_stdb_battlefields.py
git commit -m "feat(stdb): add event log and battlefields"
```

---

## Track B: Client Transport

### Task 3: Implement `/ws/client` Wire stream with replay/catch-up

**Files:**
- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `apps/heiwa_hub/auth.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/state.py`
- Test: `apps/heiwa_hub/tests/test_mcp_server_surface.py`
- Test: `apps/heiwa_hub/tests/test_wire_client_protocol.py`

- [ ] **Step 1: Add failing Wire protocol tests**

```python
async def test_ws_client_replays_after_last_seen_event_id(ws_client, seeded_events):
    conn = await ws_client.connect("/ws/client?last_seen_event_id=e1", token="test-token")
    payload = await conn.receive_json()
    assert payload["event_id"] == "e2"
```

- [ ] **Step 2: Run the failing protocol tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_mcp_server_surface.py apps/heiwa_hub/tests/test_wire_client_protocol.py -q`
Expected: FAIL on missing `/ws/client` endpoint or replay behavior

- [ ] **Step 3: Implement authenticated `/ws/client` in `mcp_server.py`**

Behavior:
- validate bearer/session token on connect
- resolve `owner_id` + `principal_id`
- accept `last_seen_event_id`
- replay persisted events from STDB
- stream new events as they are appended

Pseudo-shape:

```python
@app.websocket("/ws/client")
async def ws_client(ws: WebSocket):
    claims = await authenticate_websocket(ws)
    ctx = resolve_identity_context(claims)
    await replay_events(ws, owner_id=ctx["owner_id"], after_event_id=last_seen)
    await stream_live_events(ws, owner_id=ctx["owner_id"])
```

- [ ] **Step 4: Persist every hub-visible event before emission**

Update the task, approval, mission, and operator notification paths in `mcp_server.py` so they call `db.append_event(...)` before fan-out.

- [ ] **Step 5: Re-run the Wire tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_mcp_server_surface.py apps/heiwa_hub/tests/test_wire_client_protocol.py -q`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/mcp_server.py \
  apps/heiwa_hub/auth.py \
  packages/heiwa_sdk/heiwa_sdk/state.py \
  apps/heiwa_hub/tests/test_mcp_server_surface.py \
  apps/heiwa_hub/tests/test_wire_client_protocol.py
git commit -m "feat(wire): add authenticated client websocket replay"
```

### Task 4: Connect Heiwa CLI to Wire and battlefield registration

**Files:**
- Create: `packages/heiwa_cli/heiwa_cli/wire.py`
- Modify: `packages/heiwa_cli/heiwa_cli/context.py`
- Modify: `packages/heiwa_cli/heiwa_cli/repl.py`
- Modify: `packages/heiwa_cli/heiwa_cli/oneshot.py`
- Test: `apps/heiwa_hub/tests/test_cli_start.py`
- Test: `apps/heiwa_hub/tests/test_cli_import_chain.py`
- Test: `apps/heiwa_hub/tests/test_wire_client_protocol.py`

- [ ] **Step 1: Add a failing CLI battlefield registration test**

```python
async def test_cli_registers_battlefield_on_connect(monkeypatch):
    ctx = CLIContext()
    wire = WireClient(ctx)
    await wire.connect()
    assert wire.battlefield_id is not None
```

- [ ] **Step 2: Run the targeted CLI/Wire tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cli_start.py apps/heiwa_hub/tests/test_cli_import_chain.py apps/heiwa_hub/tests/test_wire_client_protocol.py -q`
Expected: FAIL because CLI still posts tasks directly and has no Wire client

- [ ] **Step 3: Add a reusable `WireClient`**

Implement `packages/heiwa_cli/heiwa_cli/wire.py` with:
- auth handshake
- battlefield registration/reattachment
- event replay cursor
- mission/task event subscription

- [ ] **Step 4: Move REPL and one-shot hub streaming onto the new client**

Replace the current task-specific websocket polling path with `WireClient.submit_task(...)` + `WireClient.stream_mission(...)`.

- [ ] **Step 5: Record battlefield metadata from the current repo**

`CLIContext` should derive:
- repo root
- repo remote URL if available
- root path on current node
- stable battlefield display name

- [ ] **Step 6: Re-run the CLI/Wire tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cli_start.py apps/heiwa_hub/tests/test_cli_import_chain.py apps/heiwa_hub/tests/test_wire_client_protocol.py -q`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add packages/heiwa_cli/heiwa_cli/wire.py \
  packages/heiwa_cli/heiwa_cli/context.py \
  packages/heiwa_cli/heiwa_cli/repl.py \
  packages/heiwa_cli/heiwa_cli/oneshot.py \
  apps/heiwa_hub/tests/test_cli_start.py \
  apps/heiwa_hub/tests/test_cli_import_chain.py \
  apps/heiwa_hub/tests/test_wire_client_protocol.py
git commit -m "feat(cli): add Wire client and battlefield registration"
```

---

## Track C: Control Plane

### Task 5: Introduce Executor-first routing on top of Nodes

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/provider_registry.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/tool_mesh.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py`
- Modify: `apps/heiwa_hub/transport.py`
- Test: `apps/heiwa_hub/tests/test_heiwaclaw_gateway.py`
- Test: `apps/heiwa_hub/tests/test_rate_group_routing.py`
- Test: `apps/heiwa_hub/tests/test_reactive_assignment.py`

- [ ] **Step 1: Write failing routing tests that distinguish Node from Executor**

```python
def test_cascade_selects_executor_not_host():
    route = registry.resolve_executor("research")
    assert route.executor_id == "railway:codex:gpt-5.4"
    assert route.node_id == "railway-hub"
```

- [ ] **Step 2: Run the routing tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_heiwaclaw_gateway.py apps/heiwa_hub/tests/test_rate_group_routing.py apps/heiwa_hub/tests/test_reactive_assignment.py -q`
Expected: FAIL on missing Executor vocabulary or stale Node assumptions

- [ ] **Step 3: Add explicit Executor metadata to the provider registry**

Model it as a small resolved object:

```python
@dataclass(slots=True)
class ExecutorConfig:
    executor_id: str
    node_id: str
    provider_id: str
    model: str
    rate_group: str
```

- [ ] **Step 4: Update ToolMesh / HeiwaClaw gateway to execute against Executors**

The gateway should still validate node reachability, but actual selection should happen at the Executor layer.

- [ ] **Step 5: Re-run routing tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_heiwaclaw_gateway.py apps/heiwa_hub/tests/test_rate_group_routing.py apps/heiwa_hub/tests/test_reactive_assignment.py -q`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/provider_registry.py \
  packages/heiwa_sdk/heiwa_sdk/tool_mesh.py \
  packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py \
  packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py \
  apps/heiwa_hub/transport.py \
  apps/heiwa_hub/tests/test_heiwaclaw_gateway.py \
  apps/heiwa_hub/tests/test_rate_group_routing.py \
  apps/heiwa_hub/tests/test_reactive_assignment.py
git commit -m "refactor(routing): separate nodes from executors"
```

### Task 6: Merge Spine + HeiwaClaw into Captain and extract Pulse

**Files:**
- Create: `apps/heiwa_hub/agents/captain.py`
- Create: `apps/heiwa_hub/agents/pulse.py`
- Modify: `apps/heiwa_hub/main.py`
- Modify: `apps/heiwa_hub/agents/spine.py`
- Modify: `apps/heiwa_hub/agents/heiwaclaw.py`
- Modify: `apps/heiwa_hub/agents/telemetry.py`
- Test: `apps/heiwa_hub/tests/test_hub_bootstrap_imports.py`
- Test: `apps/heiwa_hub/tests/test_phase5_integration.py`
- Test: `apps/heiwa_hub/tests/test_captain_runtime.py`

- [ ] **Step 1: Add failing Captain boot/runtime tests**

```python
async def test_hub_boots_captain_and_pulse(monkeypatch):
    started = await boot_runtime()
    assert "Captain" in started
    assert "Pulse" in started
```

- [ ] **Step 2: Run the failing runtime tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_hub_bootstrap_imports.py apps/heiwa_hub/tests/test_phase5_integration.py apps/heiwa_hub/tests/test_captain_runtime.py -q`
Expected: FAIL because boot still instantiates Spine/HeiwaClaw/Telemetry

- [ ] **Step 3: Create `captain.py` by moving the durable control loop into one runtime**

Captain owns:
- event intake
- mission planning
- approval escalation
- executor dispatch
- housekeeping tick

- [ ] **Step 4: Create `pulse.py` for health/rate monitoring**

Pulse owns:
- node liveness snapshots
- executor capacity snapshots
- periodic monitoring and event emission

- [ ] **Step 5: Update `main.py` to boot Captain + Pulse + optional Discord adapter**

Keep compatibility shims in `spine.py`, `heiwaclaw.py`, and `telemetry.py` until downstream imports are cleaned.

- [ ] **Step 6: Re-run runtime tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_hub_bootstrap_imports.py apps/heiwa_hub/tests/test_phase5_integration.py apps/heiwa_hub/tests/test_captain_runtime.py -q`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/agents/captain.py \
  apps/heiwa_hub/agents/pulse.py \
  apps/heiwa_hub/main.py \
  apps/heiwa_hub/agents/spine.py \
  apps/heiwa_hub/agents/heiwaclaw.py \
  apps/heiwa_hub/agents/telemetry.py \
  apps/heiwa_hub/tests/test_hub_bootstrap_imports.py \
  apps/heiwa_hub/tests/test_phase5_integration.py \
  apps/heiwa_hub/tests/test_captain_runtime.py
git commit -m "refactor(hub): merge captain runtime and extract pulse"
```

### Task 7: Decompose Messenger into a Discord adapter

**Files:**
- Create: `apps/heiwa_hub/adapters/discord.py`
- Modify: `apps/heiwa_hub/agents/messenger.py`
- Modify: `apps/heiwa_hub/main.py`
- Test: `apps/heiwa_hub/tests/test_discord_smoke_imports.py`
- Test: `apps/heiwa_hub/tests/test_discord_smoke_payload.py`
- Test: `apps/heiwa_hub/tests/test_operator_approval_source.py`

- [ ] **Step 1: Write failing adapter import/render tests**

```python
def test_discord_adapter_renders_wire_event():
    event = {"event_type": "approval_needed", "payload": {"mission_id": "m1"}}
    embed = DiscordAdapter.render_event(event)
    assert embed.title == "Approval needed"
```

- [ ] **Step 2: Run the Discord tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_discord_smoke_imports.py apps/heiwa_hub/tests/test_discord_smoke_payload.py apps/heiwa_hub/tests/test_operator_approval_source.py -q`
Expected: FAIL because Discord behavior still lives in `MessengerAgent`

- [ ] **Step 3: Create a thin adapter that consumes Wire/REST**

`DiscordAdapter` should:
- subscribe to Wire events
- render embeds/buttons
- translate button presses/slash commands to hub REST calls
- avoid any planning/execution logic

- [ ] **Step 4: Reduce `MessengerAgent` to compatibility shim or remove it**

Prefer a shim first to avoid import breakage during the refactor.

- [ ] **Step 5: Re-run the Discord tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_discord_smoke_imports.py apps/heiwa_hub/tests/test_discord_smoke_payload.py apps/heiwa_hub/tests/test_operator_approval_source.py -q`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/adapters/discord.py \
  apps/heiwa_hub/agents/messenger.py \
  apps/heiwa_hub/main.py \
  apps/heiwa_hub/tests/test_discord_smoke_imports.py \
  apps/heiwa_hub/tests/test_discord_smoke_payload.py \
  apps/heiwa_hub/tests/test_operator_approval_source.py
git commit -m "refactor(discord): replace messenger agent with adapter"
```

---

## Track D: Vault, Packaging, Docs

### Task 8: Connect BYOK Vault and provider auth to owner/principal and Executors

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/vault.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/provider_registry.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/main.py`
- Modify: `packages/heiwa_cli/heiwa_cli/auth.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Test: `apps/heiwa_hub/tests/test_user_auth_dashboard.py`
- Test: `apps/heiwa_hub/tests/test_rate_group_routing.py`

- [ ] **Step 1: Add failing BYOK/routing tests**

```python
def test_executor_visibility_is_scoped_to_owner_credentials():
    rows = state.get_provider_accounts(owner_id="owner-devon")
    assert all(row["owner_id"] == "owner-devon" for row in rows)
```

- [ ] **Step 2: Run the targeted auth/routing tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_rate_group_routing.py -q`
Expected: FAIL on stale `user_id`/provider-account plumbing

- [ ] **Step 3: Ensure Vault/provider auth writes stamp `owner_id` + `principal_id`**

Apply the same identity context to provider auth sync, validation, and read paths.

- [ ] **Step 4: Feed owner-scoped provider accounts into Executor selection**

Only Executors backed by credentials for the active owner should enter the rate cascade.

- [ ] **Step 5: Re-run the targeted tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_user_auth_dashboard.py apps/heiwa_hub/tests/test_rate_group_routing.py -q`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/vault.py \
  packages/heiwa_sdk/heiwa_sdk/provider_registry.py \
  packages/heiwa_sdk/heiwa_sdk/main.py \
  packages/heiwa_cli/heiwa_cli/auth.py \
  apps/heiwa_hub/mcp_server.py \
  apps/heiwa_hub/tests/test_user_auth_dashboard.py \
  apps/heiwa_hub/tests/test_rate_group_routing.py
git commit -m "feat(vault): scope provider credentials by owner and executor"
```

### Task 9: Package the runtime and update companion documents

**Files:**
- Modify: `apps/heiwa_hub/Dockerfile`
- Modify: `railway.toml`
- Modify: `infra/cloud/railway/README.md`
- Modify: `config/swarm/END_STATE_2026-03.md`
- Modify: `ops/context/HEIWA.md`
- Modify: `CLAUDE.md`
- Test: `apps/heiwa_hub/tests/test_cloud_hq_start_script.py`
- Test: `apps/heiwa_hub/tests/test_hub_bootstrap_imports.py`

- [ ] **Step 1: Add a failing deployment/bootstrap expectation**

Document and test that Railway boots Captain + Pulse + Discord adapter hooks, not the retired agent set.

- [ ] **Step 2: Run deployment/bootstrap tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cloud_hq_start_script.py apps/heiwa_hub/tests/test_hub_bootstrap_imports.py -q`
Expected: FAIL until packaging/docs align

- [ ] **Step 3: Update runtime packaging**

Make sure Docker/Railway boot the new modules and required env defaults.

- [ ] **Step 4: Update the three companion documents called out in the spec**

Required docs:
- `config/swarm/END_STATE_2026-03.md`
- `ops/context/HEIWA.md`
- `CLAUDE.md`

- [ ] **Step 5: Re-run deployment/bootstrap tests**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cloud_hq_start_script.py apps/heiwa_hub/tests/test_hub_bootstrap_imports.py -q`
Expected: PASS

- [ ] **Step 6: Run the end-to-end focused regression sweep**

Run:

```bash
.venv/bin/python -m pytest \
  apps/heiwa_hub/tests/test_stdb_user_scoping.py \
  apps/heiwa_hub/tests/test_stdb_wire_events.py \
  apps/heiwa_hub/tests/test_stdb_battlefields.py \
  apps/heiwa_hub/tests/test_mcp_server_surface.py \
  apps/heiwa_hub/tests/test_wire_client_protocol.py \
  apps/heiwa_hub/tests/test_rate_group_routing.py \
  apps/heiwa_hub/tests/test_phase5_integration.py -q
cd apps/heiwa_hub/spacetimedb && spacetime build
```

Expected: All targeted tests PASS; STDB build succeeds

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/Dockerfile \
  railway.toml \
  infra/cloud/railway/README.md \
  config/swarm/END_STATE_2026-03.md \
  ops/context/HEIWA.md \
  CLAUDE.md \
  apps/heiwa_hub/tests/test_cloud_hq_start_script.py \
  apps/heiwa_hub/tests/test_hub_bootstrap_imports.py
git commit -m "docs(deploy): align packaging and operator docs with hub architecture"
```

---

## Implementation notes

- Keep compatibility shims for `spine.py`, `heiwaclaw.py`, `telemetry.py`, and `messenger.py` until imports and tests are green. Remove them only in a later cleanup pass.
- Do not combine Track A schema migration with Track C runtime refactor in one commit. Keep state-contract changes independently publishable.
- Preserve existing observe-mode lease behavior until Task 9. The plan assumes `observe` remains default until schema + Wire + Captain plumbing are in place.
- Treat the local persistent STDB caveat as unresolved operational debt. If local schema resets are needed during Track A, document them in commit messages and operator docs rather than hiding them.

## Manual review checklist

Before execution, verify this plan still matches the approved spec:

- `owner_id` scopes records; `principal_id` attributes actions
- Sessions are transport only; Missions own leases
- Nodes are runtime endpoints; Executors are provider/model surfaces
- Hub is the product; Discord/Web/CLI are clients over Wire
- Captain + Pulse replace Spine/HeiwaClaw/Telemetry as runtime concepts

