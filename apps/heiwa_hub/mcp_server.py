import asyncio
import datetime
import json
import logging
import time
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any, Dict

import os
import uuid

import subprocess
import sys
from fastapi import FastAPI, HTTPException, Header, Request, WebSocket, WebSocketDisconnect
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from heiwa_sdk import (
    HubStateService,
    MCPBridge,
    Database,
    MemoryService,
    HeiwaBench,
    HeiwaCellCatalog,
    redact_any,
    load_swarm_env,
)
from heiwa_sdk.heiwaclaw import OpenClaw
from heiwa_protocol.routing import BrokerRouteRequest

from heiwa_hub.cognition.enrichment import BrokerEnrichmentService
from heiwa_hub.cognition.approval import get_approval_registry
from heiwa_hub.approval_views import list_approvals_from_stdb
from heiwa_hub.transport import get_bus, get_worker_manager
from heiwa_hub.auth import (
    auth_discord_redirect,
    auth_discord_callback,
    get_current_user,
    require_user,
    resolve_identity_context,
    verify_jwt,
)
from heiwa_protocol.protocol import Subject
from heiwa_sdk.proposal_dispatch import dispatch_routable_proposals, sync_remote_worker_node

# Initialized Environment
load_swarm_env()

logger = logging.getLogger("Hub.MCP")


@asynccontextmanager
async def _lifespan(application: FastAPI):
    # Register event bus listeners for task snapshots
    bus = get_bus()
    await bus.subscribe(Subject.TASK_STATUS, _record_status_event)
    await bus.subscribe(Subject.TASK_EXEC_RESULT, _record_result_event)
    await bus.subscribe(Subject.TASK_PROGRESS, _record_progress_event)
    application.state._task_snapshot_listeners = True
    yield


app = FastAPI(title="Heiwa Core MCP Server", lifespan=_lifespan)
db = Database()
state = HubStateService(db)
mcp_bridge = MCPBridge()
enrichment = BrokerEnrichmentService()
approvals = get_approval_registry()
TASK_SNAPSHOTS: dict[str, dict[str, Any]] = {}
OPERATOR_STREAMS: set[asyncio.Queue] = set()
CLIENT_STREAM_HEARTBEAT_SEC = 30.0
CLIENT_EVENT_STREAMS: dict[str, set[tuple[asyncio.AbstractEventLoop, asyncio.Queue[dict[str, Any]]]]] = {}

ROOT = Path(__file__).resolve().parents[2]
bench = HeiwaBench(ROOT)
cells = HeiwaCellCatalog(ROOT)
claw_gateway = OpenClaw(ROOT)
WEB_ROOT = ROOT / "apps" / "heiwa_web" / "clients" / "web"
ASSETS_ROOT = WEB_ROOT / "assets"

if ASSETS_ROOT.exists():
    app.mount("/assets", StaticFiles(directory=str(ASSETS_ROOT)), name="assets")


def _web_file(name: str) -> Path | None:
    candidate = WEB_ROOT / name
    return candidate if candidate.exists() else None


class MCPTool(BaseModel):
    name: str
    description: str
    input_schema: Dict[str, Any]


def _snapshot_task(task_id: str, **patch: Any) -> dict[str, Any]:
    record = dict(TASK_SNAPSHOTS.get(task_id, {}))
    record["task_id"] = task_id
    record["updated_at"] = time.time()
    for key, value in patch.items():
        if value is not None:
            record[key] = value
    TASK_SNAPSHOTS[task_id] = record
    return record


def _page_or_404(name: str) -> FileResponse:
    page = _web_file(name)
    if page:
        return FileResponse(page)
    raise HTTPException(status_code=404, detail=f"{name} unavailable")


def _approval_state_uses_stdb() -> bool:
    return bool(getattr(db, "stdb", None)) and getattr(db, "state_backend", None) == "spacetimedb"


def _list_approvals_snapshot(status: str | None = None) -> list[dict[str, Any]]:
    if _approval_state_uses_stdb():
        try:
            approvals_from_stdb = list_approvals_from_stdb(db.stdb, status=status, limit=100)
            if approvals_from_stdb:
                return approvals_from_stdb
        except Exception:
            logger.exception("Failed to load approvals from STDB")
    items: list[dict[str, Any]] = []
    for state in approvals.list_states(status=status):
        serialized = _serialize_approval(state.task_id)
        if serialized:
            items.append(serialized)
    return items


def _operator_snapshot_payload() -> dict[str, Any]:
    from heiwa_sdk.rate_ledger import get_rate_ledger

    return {
        "providers": state.get_provider_accounts(),
        "missions": state.get_missions(limit=100),
        "approvals": _list_approvals_snapshot(),
        "live_tasks": list(TASK_SNAPSHOTS.values())[-100:],
        "rate_groups": get_rate_ledger().status(),
        "history": state.get_history(limit=25),
        "timestamp": time.time(),
    }


def _notify_operator_streams() -> None:
    if not OPERATOR_STREAMS:
        return
    payload = _operator_snapshot_payload()
    stale: list[asyncio.Queue] = []
    for queue in list(OPERATOR_STREAMS):
        try:
            if queue.full():
                try:
                    queue.get_nowait()
                except Exception:
                    pass
            queue.put_nowait(payload)
        except Exception:
            stale.append(queue)
    for queue in stale:
        OPERATOR_STREAMS.discard(queue)

def _offer_client_event(queue: asyncio.Queue[dict[str, Any]], payload: dict[str, Any]) -> None:
    if queue.full():
        try:
            queue.get_nowait()
        except asyncio.QueueEmpty:
            pass
    queue.put_nowait(payload)


def _notify_client_streams(owner_id: str | None, payload: dict[str, Any]) -> None:
    if not owner_id:
        return
    subscribers = CLIENT_EVENT_STREAMS.get(owner_id)
    if not subscribers:
        return
    stale: list[tuple[asyncio.AbstractEventLoop, asyncio.Queue[dict[str, Any]]]] = []
    for loop, queue in list(subscribers):
        try:
            loop.call_soon_threadsafe(_offer_client_event, queue, payload)
        except RuntimeError:
            stale.append((loop, queue))
    for subscriber in stale:
        subscribers.discard(subscriber)
    if not subscribers:
        CLIENT_EVENT_STREAMS.pop(owner_id, None)
def _append_wire_event(
    *,
    event_type: str,
    payload: dict[str, Any],
    owner_id: str | None = None,
    principal_id: str | None = None,
    session_id: str | None = None,
    mission_id: str | None = None,
    battlefield_id: str | None = None,
) -> None:
    event_record = {
        "event_id": f"evt-{uuid.uuid4().hex[:12]}",
        "owner_id": owner_id,
        "principal_id": principal_id,
        "session_id": session_id,
        "mission_id": mission_id,
        "battlefield_id": battlefield_id,
        "event_type": event_type,
        "payload_json": payload,
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    try:
        saved = db.append_event(event_record)
        if saved is not False:
            _notify_client_streams(owner_id, event_record)
    except Exception:
        logger.debug("Failed to append wire event %s", event_type, exc_info=True)


async def _record_status_event(envelope: Dict[str, Any]):
    payload = envelope.get("data", envelope)
    task_id = payload.get("task_id")
    if not task_id:
        return
    patch = {
        "delivery_status": payload.get("status"),
        "message": payload.get("message"),
        "runtime": payload.get("runtime"),
        "step_id": payload.get("step_id"),
    }
    if str(payload.get("status") or "").upper() in {"AWAITING_APPROVAL", "APPROVED", "REJECTED", "EXPIRED"}:
        patch["approval_status"] = payload.get("status")
        patch["approval_id"] = payload.get("approval_id")
    current = TASK_SNAPSHOTS.get(task_id, {})
    if not current.get("run_status"):
        patch["status"] = payload.get("status")
    _snapshot_task(task_id, **patch)
    _append_wire_event(
        event_type="task_status",
        payload=payload,
        owner_id=payload.get("owner_id"),
        principal_id=payload.get("principal_id"),
        session_id=payload.get("session_id"),
        mission_id=task_id,
        battlefield_id=payload.get("battlefield_id"),
    )
    try:
        status = str(payload.get("status") or "").upper()
        if status == "AWAITING_APPROVAL":
            db.set_mission_status(task_id, "waiting_approval", summary=payload.get("message"))
        elif status == "APPROVED":
            db.set_mission_status(task_id, "running", summary="Approval granted")
            snapshot = TASK_SNAPSHOTS.get(task_id, {})
            cell = dict((snapshot.get("route") or {}))
            dispatch = dict(snapshot.get("dispatch") or {})
            db.start_cell_run(
                {
                    "cell_run_id": f"cellrun-{task_id}",
                    "mission_id": task_id,
                    "step_id": f"{task_id}:execute",
                    "status": "running",
                    "cell_id": snapshot.get("assigned_worker") or cell.get("assigned_worker") or dispatch.get("provider") or "worker",
                    "cell_role": snapshot.get("assigned_worker") or cell.get("assigned_worker") or "executor",
                    "provider": dispatch.get("provider", ""),
                    "model": dispatch.get("target_model", ""),
                    "rate_group": dispatch.get("rate_group", ""),
                    "tool": dispatch.get("adapter_tool", ""),
                }
            )
        elif status in {"REJECTED", "EXPIRED"}:
            db.pause_mission(task_id, summary=str(payload.get("message") or status))
    except Exception:
        pass
    _notify_operator_streams()


async def _record_result_event(envelope: Dict[str, Any]):
    payload = envelope.get("data", envelope)
    task_id = payload.get("task_id")
    if not task_id:
        return
    _snapshot_task(
        task_id,
        status=payload.get("status"),
        run_status=payload.get("status"),
        summary=payload.get("summary"),
        runtime=payload.get("runtime"),
        provider=payload.get("provider"),
        target_tool=payload.get("target_tool"),
        target_model=payload.get("target_model"),
        intent_class=payload.get("intent_class"),
        elapsed_sec=payload.get("elapsed_sec"),
    )
    _append_wire_event(
        event_type="task_result",
        payload=payload,
        owner_id=payload.get("owner_id"),
        principal_id=payload.get("principal_id"),
        session_id=payload.get("session_id"),
        mission_id=task_id,
        battlefield_id=payload.get("battlefield_id"),
    )
    try:
        summary = str(payload.get("summary") or "")
        status = str(payload.get("status") or "").upper()
        
        # 1. Attempt to close the specific cell-run (execution-level)
        try:
            db.finish_cell_run(
                {
                    "cell_run_id": f"cellrun-{task_id}",
                    "status": "completed" if status in {"PASS", "DELIVERED"} else "failed",
                    "output_summary": summary,
                }
            )
        except Exception as e:
            # Log but don't block — the mission result is more important than the execution log.
            logger.warning("Failed to finish cell-run %s (orphaned?): %s", task_id, e)

        # 2. Always attempt to close the mission (user-level)
        if status in {"PASS", "DELIVERED"}:
            db.complete_mission(task_id, summary=summary[:500] if summary else "Completed")
        elif status in {"FAIL", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}:
            db.fail_mission(task_id, error=summary[:500] if summary else status)
            
        # 3. Always register the artifact
        db.register_artifact(
            {
                "artifact_id": f"artifact-{task_id}",
                "mission_id": task_id,
                "cell_run_id": f"cellrun-{task_id}",
                "artifact_type": "summary",
                "title": "Hub task summary",
                "content": {
                    "summary": summary,
                    "status": status,
                    "provider": payload.get("provider"),
                    "target_model": payload.get("target_model"),
                },
            }
        )
    except Exception:
        pass
    _notify_operator_streams()


async def _record_progress_event(envelope: Dict[str, Any]):
    payload = envelope.get("data", envelope)
    task_id = payload.get("task_id")
    if not task_id:
        return
    _snapshot_task(task_id, progress=payload.get("content") or payload.get("message"))
    _append_wire_event(
        event_type="task_progress",
        payload=payload,
        owner_id=payload.get("owner_id"),
        principal_id=payload.get("principal_id"),
        session_id=payload.get("session_id"),
        mission_id=task_id,
        battlefield_id=payload.get("battlefield_id"),
    )
    _notify_operator_streams()


async def _check_stdb_health() -> bool:
    """Verify STDB connectivity with 2-second timeout."""
    if db.state_backend != "spacetimedb" or not db.stdb:
        return True  # Non-STDB backends are always "healthy"
    try:
        # Note: model_tiers is a known existing table in all environments
        result = await asyncio.wait_for(
            asyncio.to_thread(db.stdb.query, "SELECT * FROM model_tiers LIMIT 1"),
            timeout=2.0,
        )
        return result is not None
    except Exception:
        return False


@app.get("/health")
@app.head("/health")
async def health():
    stdb_ok = await _check_stdb_health()
    if not stdb_ok:
        from starlette.responses import JSONResponse
        return JSONResponse(
            status_code=503,
            content={
                "status": "degraded",
                "service": "heiwa-core-hub",
                "stdb": "unreachable",
                "timestamp": time.time(),
            },
        )
    return {
        "status": "alive",
        "service": "heiwa-core-hub",
        "state_backend": db.state_backend,
        "stdb": "connected",
        "gateway_transport": "websocket",
        "timestamp": time.time(),
    }


# ---------------------------------------------------------------------------
#  Auth routes — Discord OAuth2 + JWT sessions
# ---------------------------------------------------------------------------

@app.get("/auth/discord")
async def start_discord_oauth():
    """Redirect to Discord OAuth consent screen."""
    return auth_discord_redirect()


@app.get("/auth/discord/callback")
async def discord_oauth_callback(code: str = "", state: str = ""):
    """Exchange Discord OAuth code for JWT session token."""
    if not code or not state:
        raise HTTPException(status_code=400, detail="Missing code or state parameter")
    stdb = getattr(db, "stdb", None)
    if not stdb:
        raise HTTPException(status_code=503, detail="STDB not available")
    return await auth_discord_callback(code, state, stdb)


@app.get("/auth/me")
async def get_me(request: Request):
    """Return the authenticated user's profile from their JWT."""
    claims = require_user(request)
    user_id = claims.get("sub", "")
    # Optionally fetch full profile from STDB
    stdb = getattr(db, "stdb", None)
    if stdb:
        rows = stdb.query(
            f"SELECT * FROM users WHERE user_id = '{stdb._escape_sql_literal(user_id)}'"
        )
        if rows:
            return {"user": rows[0], "claims": claims}
    return {"user": {"user_id": user_id}, "claims": claims}


@app.get("/")
@app.head("/")
async def root(request: Request):
    index = _web_file("index.html")
    if index:
        return FileResponse(index)
    return {"name": "Heiwa", "status": "operational"}


@app.get("/index.html")
async def index_page():
    return _page_or_404("index.html")


@app.get("/domains")
@app.get("/domains.html")
async def domains_page():
    page = _web_file("domains.html")
    if page:
        return FileResponse(page)
    raise HTTPException(status_code=404, detail="domains page unavailable")


@app.get("/governance")
@app.get("/governance.html")
async def governance_page():
    page = _web_file("governance.html")
    if page:
        return FileResponse(page)
    raise HTTPException(status_code=404, detail="governance page unavailable")


@app.get("/status.html")
async def status_page():
    page = _web_file("status.html")
    if page:
        return FileResponse(page)
    raise HTTPException(status_code=404, detail="status page unavailable")


@app.get("/connections.html")
async def connections_page():
    return _page_or_404("connections.html")


@app.get("/dashboard")
@app.get("/dashboard.html")
async def dashboard_page():
    return _page_or_404("dashboard.html")


@app.get("/missions.html")
async def missions_page():
    return _page_or_404("missions.html")


@app.get("/live")
@app.get("/live.html")
async def live_page():
    return _page_or_404("live.html")


@app.get("/approvals.html")
async def approvals_page():
    return _page_or_404("approvals.html")


@app.get("/rate-groups.html")
async def rate_groups_page():
    return _page_or_404("rate-groups.html")


@app.get("/cells.html")
async def cells_page():
    return _page_or_404("cells.html")


@app.get("/history.html")
async def history_page():
    return _page_or_404("history.html")


@app.get("/status")
async def get_public_status():
    return _public_status_payload()


def _public_status_payload() -> dict[str, Any]:
    from heiwa_sdk.rate_ledger import get_rate_ledger
    status = state.get_public_status(minutes=60)
    try:
        status["rate_groups"] = get_rate_ledger().status()
    except Exception:
        pass
    return status


@app.websocket("/ws/status")
@app.websocket("/status/ws")
@app.websocket("/events")
async def status_stream(ws: WebSocket, token: str | None = None):
    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    if not expected or not token or token != expected:
        await ws.close(code=4003)
        return
    await ws.accept()
    try:
        while True:
            await ws.send_json(_public_status_payload())
            await asyncio.sleep(2)
    except WebSocketDisconnect:
        logger.debug("Status websocket disconnected.")


@app.get("/tools")
async def list_tools(authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    native_tools = [
        {
            "name": "heiwa_get_swarm_status",
            "description": "Retrieve the public Heiwa state snapshot.",
            "input_schema": {"type": "object", "properties": {}},
        },
        {
            "name": "heiwa_get_latest_tasks",
            "description": "Fetch recent Heiwa task runs from the active state backend.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 10},
                },
            },
        },
        {
            "name": "heiwa_resolve_route",
            "description": "Resolve a raw task into the typed HeiwaClaw route contract.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "task_id": {"type": "string"},
                    "request_id": {"type": "string"},
                    "raw_text": {"type": "string"},
                    "source_surface": {"type": "string", "default": "cli"},
                    "privacy_level": {"type": "string"},
                },
                "required": ["raw_text"],
            },
        },
        {
            "name": "heiwa_run_bench",
            "description": "Run the HeiwaBench release-gate suites for routes and cell selection.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "suite": {"type": "string", "description": "Optional suite name such as routing_matrix or cells_catalog"}
                },
            },
        },
        {
            "name": "heiwa_get_cells_catalog",
            "description": "List the current HeiwaCells catalog and optionally recommend a cell for a prompt.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "description": "Optional prompt to match against the cell catalog"}
                },
            },
        },
    ]

    try:
        bridged_tools = mcp_bridge.list_tools()
    except Exception as exc:
        logger.error("Failed to list bridged tools: %s", exc)
        bridged_tools = []

    return {"native": native_tools, "bridged": bridged_tools}


@app.post("/call/{tool_name}")
async def call_tool(tool_name: str, arguments: Dict[str, Any], authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    safe_args = redact_any(arguments)

    if tool_name == "heiwa_get_swarm_status":
        return {"content": [{"type": "text", "text": json.dumps(state.get_public_status(minutes=60), indent=2)}]}

    if tool_name == "heiwa_get_latest_tasks":
        limit = int(arguments.get("limit", 10) or 10)
        tasks = state.get_recent_runs(limit=limit)
        return {"content": [{"type": "text", "text": json.dumps(tasks, indent=2)}]}

    if tool_name == "heiwa_resolve_route":
        request = BrokerRouteRequest.from_payload(
            {
                "request_id": arguments.get("request_id", f"mcp-route-{int(time.time() * 1000)}"),
                "task_id": arguments.get("task_id", f"mcp-task-{int(time.time() * 1000)}"),
                "raw_text": arguments.get("raw_text", ""),
                "source_surface": arguments.get("source_surface", "cli"),
                "privacy_level": arguments.get("privacy_level"),
                "auth_validated": True,
                "timestamp": time.time(),
            }
        )
        result = await enrichment.enrich(request)
        return {"content": [{"type": "text", "text": json.dumps(result.to_dict(), indent=2)}]}

    if tool_name == "heiwa_query_memory":
        query = arguments.get("query", "")
        limit = arguments.get("limit", 5)
        memory = MemoryService(stdb=db.stdb) if db.stdb else None
        if not memory:
            return {"content": [{"type": "text", "text": "Memory service not available."}], "isError": True}
        
        relevant = await memory.query_knowledge(query, limit=int(limit))
        return {"content": [{"type": "text", "text": json.dumps(relevant, indent=2)}]}

    if tool_name == "heiwa_read_state":
        table = arguments.get("table", "model_tiers")
        limit = arguments.get("limit", 20)
        if not db.stdb:
            return {"content": [{"type": "text", "text": "STDB not available."}], "isError": True}
        
        rows = db.stdb.query(f"SELECT * FROM {table} LIMIT {int(limit)}")
        return {"content": [{"type": "text", "text": json.dumps(rows, indent=2)}]}

    if tool_name == "heiwa_run_bench":
        suite = arguments.get("suite")
        result = bench.run(suite=str(suite) if suite else None)
        return {"content": [{"type": "text", "text": json.dumps(result, indent=2)}]}

    if tool_name == "heiwa_get_cells_catalog":
        prompt = str(arguments.get("prompt") or "").strip()
        payload: dict[str, Any] = cells.to_public_dict()
        if prompt:
            payload["recommendation"] = cells.recommend(prompt)
        return {"content": [{"type": "text", "text": json.dumps(payload, indent=2)}]}

    try:
        result = mcp_bridge.call_tool(tool_name, safe_args)
        if result["ok"]:
            return {"content": [{"type": "text", "text": json.dumps(result["result"], indent=2)}]}
        raise HTTPException(status_code=500, detail=result.get("stderr", "MCP tool failed"))
    except Exception as exc:
        if "Tool not found" in str(exc):
            raise HTTPException(status_code=404, detail="Tool not found")
        raise


# ---------------------------------------------------------------------------
#  Auth helper
# ---------------------------------------------------------------------------

def _validate_auth_token(token: str | None) -> str:
    """Validate the Authorization bearer token against HEIWA_AUTH_TOKEN.
    Returns the raw token on success, raises HTTPException on failure."""
    from heiwa_sdk.security import SecurityService
    ss = SecurityService()
    expected = ss.get_expected_token()
    
    if not expected:
        raise HTTPException(status_code=500, detail="Hub auth not configured (HEIWA_AUTH_TOKEN unset)")
    if not token:
        raise HTTPException(status_code=401, detail="Missing Authorization header")
    # Accept "Bearer <token>" or raw token
    raw = token.removeprefix("Bearer ").strip() if token.startswith("Bearer ") else token.strip()
    
    if not ss.validate_token(raw):
        raise HTTPException(status_code=403, detail="Invalid auth token")
    return raw


# ---------------------------------------------------------------------------
#  Task ingress
# ---------------------------------------------------------------------------

class TaskRequest(BaseModel):
    raw_text: str
    sender_id: str = "cli"
    source_surface: str = "cli"
    privacy_level: str | None = None
    task_id: str | None = None
    session_id: str | None = None
    battlefield_id: str | None = None


class ApprovalDecisionRequest(BaseModel):
    actor: str = "operator"
    reason: str | None = None


class BattlefieldRequest(BaseModel):
    battlefield_id: str
    name: str
    repo_url: str | None = None
    root_path: str | None = None
    node_id: str | None = None
    status: str = "active"


@app.post("/battlefields")
async def register_battlefield(req: BattlefieldRequest, authorization: str | None = Header(None)):
    auth_token = _validate_auth_token(authorization)
    claims = _ws_client_claims(auth_token) or {"owner_id": "local-operator", "principal_id": "local-operator"}
    identity = resolve_identity_context(claims)
    payload = {
        "battlefield_id": req.battlefield_id,
        "owner_id": identity["owner_id"],
        "principal_id": identity["principal_id"],
        "name": req.name,
        "repo_url": req.repo_url,
        "root_path": req.root_path,
        "node_id": req.node_id,
        "status": req.status,
    }
    if not db.upsert_battlefield(payload):
        raise HTTPException(status_code=500, detail="Failed to register battlefield")
    row = db.get_battlefield(req.battlefield_id, owner_id=identity["owner_id"]) or payload
    _append_wire_event(
        event_type="battlefield_registered",
        payload=row,
        owner_id=identity["owner_id"],
        principal_id=identity["principal_id"],
        battlefield_id=req.battlefield_id,
    )
    return row


@app.post("/tasks")
async def create_task(req: TaskRequest, authorization: str | None = Header(None)):
    """Authenticated task ingress. Enriches once here — Spine skips re-enrichment."""
    auth_token = _validate_auth_token(authorization)
    claims = _ws_client_claims(auth_token) or {"owner_id": "local-operator", "principal_id": "local-operator"}
    identity = resolve_identity_context(claims)
    task_id = req.task_id or f"cli-task-{uuid.uuid4().hex[:8]}"

    # Single source of truth: enrich here, pass route fields through the bus
    broker_req = BrokerRouteRequest.from_payload({
        "request_id": f"http-{task_id}",
        "task_id": task_id,
        "raw_text": req.raw_text,
        "sender_id": req.sender_id,
        "source_surface": req.source_surface,
        "privacy_level": req.privacy_level,
        "auth_validated": True,
        "timestamp": time.time(),
    })
    route = await enrichment.enrich(broker_req)
    dispatch = claw_gateway.resolve(route)
    cell_recommendation = cells.recommend(req.raw_text)
    cell = dict(cell_recommendation.get("cell") or {})

    # Publish to local bus — include auth_token so Spine's barrier passes,
    # and include enrichment fields so Spine skips double-enrichment.
    bus = get_bus()
    route_payload = route.to_dict()
    route_payload.update({
        "task_id": task_id,
        "raw_text": req.raw_text,
        "source": req.source_surface,
        "source_surface": req.source_surface,
        "sender_id": req.sender_id,
        "requested_by": req.sender_id,
        "auth_token": auth_token,
        "owner_id": identity["owner_id"],
        "principal_id": identity["principal_id"],
        "session_id": req.session_id,
        "battlefield_id": req.battlefield_id,
        "_pre_enriched": True,
    })
    await bus.publish(Subject.CORE_REQUEST, route_payload, sender_id=req.sender_id)

    _snapshot_task(
        task_id,
        status="ACCEPTED",
        route=route.to_dict(),
        dispatch=dispatch.to_dict(),
        sender_id=req.sender_id,
        source_surface=req.source_surface,
        raw_text=req.raw_text,
        risk_level=route.risk_level,
        requires_approval=route.requires_approval,
        provider=dispatch.provider,
        assigned_worker=route.assigned_worker,
        target_tool=dispatch.adapter_tool,
        target_model=dispatch.target_model,
        rate_group=dispatch.rate_group,
        mission_id=task_id,
        owner_id=identity["owner_id"],
        principal_id=identity["principal_id"],
        session_id=req.session_id,
        battlefield_id=req.battlefield_id,
    )

    try:
        db.create_mission(
            {
                "mission_id": task_id,
                "status": "waiting_approval" if route.requires_approval else "running",
                "source_surface": req.source_surface,
                "node_id": req.sender_id,
                "prompt": req.raw_text,
                "intent_class": route.intent_class,
                "risk_level": route.risk_level,
                "active_cell_id": cell.get("cell_id") or route.assigned_worker,
                "target_tool": dispatch.adapter_tool,
                "target_model": dispatch.target_model,
                "owner_id": identity["owner_id"],
                "principal_id": identity["principal_id"],
                "summary": None,
                "metadata": {"route": route.to_dict(), "dispatch": dispatch.to_dict(), "cell": cell},
            }
        )
        db.append_mission_step(
            {
                "step_id": f"{task_id}:ingress",
                "mission_id": task_id,
                "position": 1,
                "status": "completed",
                "step_kind": "ingress",
                "cell_role": (cell.get("roster") or [{}])[0].get("role", route.assigned_worker),
                "title": "Ingress",
                "detail": f"{route.intent_class} via {req.source_surface}",
                "input": {"prompt": req.raw_text},
                "output": route.to_dict(),
                "owner_id": identity["owner_id"],
                "principal_id": identity["principal_id"],
            }
        )
        db.append_mission_step(
            {
                "step_id": f"{task_id}:execute",
                "mission_id": task_id,
                "position": 2,
                "status": "waiting_approval" if route.requires_approval else "running",
                "step_kind": "execute",
                "cell_role": (cell.get("roster") or [{}])[0].get("role", route.assigned_worker),
                "title": "Execution",
                "detail": f"{dispatch.provider} {dispatch.target_model}",
                "input": route.to_dict(),
                "output": {},
                "owner_id": identity["owner_id"],
                "principal_id": identity["principal_id"],
            }
        )
        if not route.requires_approval:
            db.start_cell_run(
                {
                    "cell_run_id": f"cellrun-{task_id}",
                    "mission_id": task_id,
                    "step_id": f"{task_id}:execute",
                    "status": "running",
                    "cell_id": cell.get("cell_id") or route.assigned_worker,
                    "cell_role": (cell.get("roster") or [{}])[0].get("role", route.assigned_worker),
                    "provider": dispatch.provider,
                    "model": dispatch.target_model,
                    "rate_group": dispatch.rate_group,
                    "tool": dispatch.adapter_tool,
                    "owner_id": identity["owner_id"],
                    "principal_id": identity["principal_id"],
                }
            )
    except Exception:
        logger.exception("Failed to persist mission bootstrap for %s", task_id)

    _append_wire_event(
        event_type="task_accepted",
        payload={
            "task_id": task_id,
            "status": "ACCEPTED",
            "raw_text": req.raw_text,
            "source_surface": req.source_surface,
            "sender_id": req.sender_id,
            "intent_class": route.intent_class,
            "target_tool": dispatch.adapter_tool,
            "target_model": dispatch.target_model,
        },
        owner_id=identity["owner_id"],
        principal_id=identity["principal_id"],
        session_id=req.session_id,
        mission_id=task_id,
        battlefield_id=req.battlefield_id,
    )

    return {
        "task_id": task_id,
        "status": "ACCEPTED",
        "route": route.to_dict(),
    }


@app.get("/tasks/{task_id}")
async def get_task(task_id: str, authorization: str | None = Header(None)):
    """Retrieve task status/results from state backend."""
    _validate_auth_token(authorization)
    if task_id in TASK_SNAPSHOTS:
        return TASK_SNAPSHOTS[task_id]
    runs = state.get_recent_runs(limit=50)
    for run in runs:
        if run.get("proposal_id") == task_id or run.get("run_id", "").endswith(task_id):
            return run
    raise HTTPException(status_code=404, detail=f"Task {task_id} not found")


def _serialize_approval(task_id: str) -> dict[str, Any] | None:
    if _approval_state_uses_stdb():
        try:
            approvals_from_stdb = list_approvals_from_stdb(db.stdb, limit=100)
            for item in approvals_from_stdb:
                if item.get("task_id") == task_id:
                    return item
        except Exception:
            logger.exception("Failed to serialize approval from STDB for %s", task_id)
    state = approvals.get_state(task_id)
    if not state:
        return None
    payload = approvals.get_payload(task_id) or {}
    return {
        "task_id": task_id,
        "status": state.status,
        "created_at": state.created_at,
        "expires_at": state.expires_at,
        "decision_by": state.decision_by,
        "decision_at": state.decision_at,
        "reason": state.reason,
        "approval_id": payload.get("approval_id") or f"approval-{task_id}",
        "risk_level": payload.get("risk_level"),
        "source_surface": payload.get("source_surface") or payload.get("source"),
        "requested_by": payload.get("requested_by"),
        "raw_text_excerpt": str(payload.get("raw_text") or "")[:200],
    }


@app.get("/approvals")
async def list_approvals(status: str | None = None, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    return {"approvals": _list_approvals_snapshot(status=status)}


async def _submit_approval_decision(
    task_id: str,
    *,
    approved: bool,
    request: ApprovalDecisionRequest,
    authorization: str | None,
) -> dict[str, Any]:
    _validate_auth_token(authorization)
    state = approvals.get_state(task_id)
    if not state:
        raise HTTPException(status_code=404, detail=f"No pending approval for {task_id}")
    if state.status != "PENDING":
        serialized = _serialize_approval(task_id)
        return serialized or {"task_id": task_id, "status": state.status}

    applied = approvals.decide(
        task_id,
        approved=approved,
        actor=request.actor,
        reason=request.reason,
    )
    if not applied:
        raise HTTPException(status_code=404, detail=f"No pending approval for {task_id}")

    bus = get_bus()
    await bus.publish(Subject.TASK_APPROVAL_DECISION, {
        "task_id": task_id,
        "approved": approved,
        "actor": request.actor,
        "reason": request.reason,
        "_state_applied": True,
    }, sender_id=request.actor or "operator")
    _notify_operator_streams()
    serialized = _serialize_approval(task_id)
    return serialized or {"task_id": task_id, "status": applied.status}


@app.post("/tasks/{task_id}/approve")
async def approve_task(
    task_id: str,
    request: ApprovalDecisionRequest,
    authorization: str | None = Header(None),
):
    return await _submit_approval_decision(
        task_id,
        approved=True,
        request=request,
        authorization=authorization,
    )


@app.post("/tasks/{task_id}/reject")
async def reject_task(
    task_id: str,
    request: ApprovalDecisionRequest,
    authorization: str | None = Header(None),
):
    return await _submit_approval_decision(
        task_id,
        approved=False,
        request=request,
        authorization=authorization,
    )


@app.get("/auth/providers")
async def list_provider_accounts(request: Request):
    claims = require_user(request)
    user_id = str(claims.get("sub") or "")
    return {"providers": state.get_provider_accounts(user_id=user_id)}


@app.get("/auth/providers/{provider_id}/status")
async def get_provider_account_status(provider_id: str, request: Request):
    claims = require_user(request)
    user_id = str(claims.get("sub") or "")
    payload = state.get_provider_status(provider_id, user_id=user_id)
    if not payload:
        raise HTTPException(status_code=404, detail=f"Provider {provider_id} not found")
    return payload


@app.get("/missions")
async def list_missions(request: Request, status: str | None = None, limit: int = 50):
    claims = require_user(request)
    user_id = str(claims.get("sub") or "")
    return {"missions": state.get_missions(status=status, limit=limit, user_id=user_id)}


@app.get("/missions/{mission_id}")
async def get_mission(mission_id: str, request: Request):
    claims = require_user(request)
    user_id = str(claims.get("sub") or "")
    payload = state.get_mission_detail(mission_id, user_id=user_id)
    if not payload:
        raise HTTPException(status_code=404, detail=f"Mission {mission_id} not found")
    return payload


@app.post("/missions/{mission_id}/pause")
async def pause_mission(mission_id: str, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    if not state.pause_mission(mission_id, summary="Paused by operator"):
        raise HTTPException(status_code=404, detail=f"Mission {mission_id} not found")
    return state.get_mission_detail(mission_id) or {"mission_id": mission_id, "status": "paused"}


@app.post("/missions/{mission_id}/resume")
async def resume_mission(mission_id: str, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    if not state.resume_mission(mission_id, summary="Resumed by operator"):
        raise HTTPException(status_code=404, detail=f"Mission {mission_id} not found")
    return state.get_mission_detail(mission_id) or {"mission_id": mission_id, "status": "running"}


@app.get("/rate-groups")
async def list_rate_groups(authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    from heiwa_sdk.rate_ledger import get_rate_ledger

    return {"rate_groups": get_rate_ledger().status()}


@app.get("/history")
async def get_history(request: Request, limit: int = 20):
    claims = require_user(request)
    user_id = str(claims.get("sub") or "")
    return state.get_history(limit=limit, user_id=user_id)


def _ws_client_claims(token: str | None) -> dict[str, Any] | None:
    if token:
        claims = verify_jwt(token)
        if claims:
            return claims
    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    if expected and token == expected:
        return {"owner_id": "local-operator", "principal_id": "local-operator"}
    return None


@app.websocket("/ws/client")
async def client_events(
    ws: WebSocket,
    token: str | None = None,
    last_seen_event_id: str | None = None,
):
    claims = _ws_client_claims(token)
    if not claims:
        await ws.close(code=4003)
        return

    identity = resolve_identity_context(claims)
    await ws.accept()
    event_queue: asyncio.Queue[dict[str, Any]] = asyncio.Queue(maxsize=100)
    subscriber = (asyncio.get_running_loop(), event_queue)
    CLIENT_EVENT_STREAMS.setdefault(identity["owner_id"], set()).add(subscriber)
    try:
        events = db.list_events(
            after_event_id=last_seen_event_id,
            owner_id=identity["owner_id"],
            limit=200,
        )
        for event in events:
            await ws.send_json(event)
        while True:
            try:
                event = await asyncio.wait_for(event_queue.get(), timeout=CLIENT_STREAM_HEARTBEAT_SEC)
                await ws.send_json(event)
            except asyncio.TimeoutError:
                await ws.send_json({"type": "heartbeat"})
    except WebSocketDisconnect:
        logger.debug("Client websocket disconnected.")
    finally:
        subscribers = CLIENT_EVENT_STREAMS.get(identity["owner_id"])
        if subscribers is not None:
            subscribers.discard(subscriber)
            if not subscribers:
                CLIENT_EVENT_STREAMS.pop(identity["owner_id"], None)


@app.websocket("/ws/tasks/{task_id}")
async def task_events(ws: WebSocket, task_id: str, token: str | None = None):
    """Stream task events for a specific task to the CLI.

    Auth is via query parameter: ``ws://.../ws/tasks/{id}?token=<bearer>``.
    """
    # --- Auth gate ---
    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    if not expected or not token or token != expected:
        await ws.close(code=4003)
        return

    await ws.accept()
    event_queue: asyncio.Queue = asyncio.Queue()

    async def _capture(data: Dict[str, Any]):
        payload = data.get("data", data)
        if payload.get("task_id") == task_id:
            await event_queue.put(payload)

    bus = get_bus()
    await bus.subscribe(Subject.TASK_STATUS, _capture)
    await bus.subscribe(Subject.TASK_EXEC_RESULT, _capture)
    await bus.subscribe(Subject.TASK_PROGRESS, _capture)

    terminal_statuses = {"DELIVERED", "PASS", "FAIL", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}
    try:
        existing = TASK_SNAPSHOTS.get(task_id)
        if existing:
            await ws.send_json(existing)
            existing_status = str(existing.get("run_status") or existing.get("status") or "")
            if existing_status in terminal_statuses:
                return
        while True:
            try:
                event = await asyncio.wait_for(event_queue.get(), timeout=30.0)
                await ws.send_json(event)
                if event.get("status") in terminal_statuses:
                    break
            except asyncio.TimeoutError:
                await ws.send_json({"type": "heartbeat", "task_id": task_id})
    except WebSocketDisconnect:
        pass
    finally:
        # Unsubscribe to avoid leaked callbacks
        for subj in (Subject.TASK_STATUS, Subject.TASK_EXEC_RESULT, Subject.TASK_PROGRESS):
            bus.unsubscribe(subj, _capture)


@app.websocket("/ws/chat")
async def chat_socket(ws: WebSocket):
    """
    WebSocket chat endpoint — fast, bidirectional conversation.

    Auth via first frame: client sends {"type": "auth", "token": "<bearer>"}
    Then chat frames:     {"content": "yo", "session_id": "optional-session-key"}
    Server sends:         {"content": "response text", "session_id": "...", "timestamp": ...}
    """
    import hmac
    await ws.accept()

    # First frame must be auth
    try:
        auth_frame = await asyncio.wait_for(ws.receive_json(), timeout=10.0)
    except (asyncio.TimeoutError, Exception):
        await ws.send_json({"error": "Auth timeout"})
        await ws.close(code=4003)
        return

    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    token = auth_frame.get("token", "")
    if not expected or not token or not hmac.compare_digest(token.encode(), expected.encode()):
        await ws.send_json({"error": "Invalid auth"})
        await ws.close(code=4003)
        return

    await ws.send_json({"type": "auth_ok"})

    from heiwa_hub.chat import get_chat_engine
    engine = get_chat_engine()
    session_id = f"ws-{uuid.uuid4().hex[:8]}"

    try:
        while True:
            raw = await ws.receive_json()
            content = raw.get("content", "").strip()
            if not content:
                continue

            sid = raw.get("session_id") or session_id

            reply = await engine.respond(session_id=sid, content=content)
            await ws.send_json({
                "content": reply,
                "session_id": sid,
                "timestamp": time.time(),
            })
    except WebSocketDisconnect:
        logger.debug("Chat WebSocket disconnected (session=%s)", session_id)


@app.websocket("/ws/operator")
async def operator_events(ws: WebSocket, token: str | None = None):
    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    if not expected or not token or token != expected:
        await ws.close(code=4003)
        return

    await ws.accept()
    queue: asyncio.Queue = asyncio.Queue(maxsize=1)
    OPERATOR_STREAMS.add(queue)
    try:
        await ws.send_json(_operator_snapshot_payload())
        while True:
            payload = await queue.get()
            await ws.send_json(payload)
    except WebSocketDisconnect:
        logger.debug("Operator websocket disconnected.")
    finally:
        OPERATOR_STREAMS.discard(queue)


# ---------------------------------------------------------------------------
#  Worker WebSocket — hybrid push/pull for Mac/WSL nodes
# ---------------------------------------------------------------------------

@app.websocket("/ws/worker")
async def worker_socket(ws: WebSocket):
    """
    Remote worker registration and task delivery.

    Protocol:
      1. Worker sends: {"type": "register", "worker_id": "...", "auth_token": "...", "capabilities": {...}}
      2. Hub pushes:   {"type": "task_assignment", "data": {...}}
      3. Worker sends: {"type": "result", "task_id": "...", "status": "...", "summary": "..."}
      4. Worker sends: {"type": "heartbeat"} periodically
    """
    await ws.accept()
    wm = get_worker_manager()
    worker_id = None
    authenticated = False
    from heiwa_sdk.security import SecurityService
    ss = SecurityService()

    try:
        while True:
            raw = await ws.receive_json()
            msg_type = raw.get("type", "")

            if msg_type == "register":
                # Auth gate: first message must include valid token
                token = raw.get("auth_token", "")
                if not ss.validate_token(token):
                    await ws.send_json({"type": "error", "detail": "Invalid auth token"})
                    await ws.close(code=4003)
                    return
                authenticated = True
                worker_id = raw.get("worker_id", f"worker-{uuid.uuid4().hex[:8]}")
                capabilities = raw.get("capabilities", {})
                wm.register(worker_id, ws, capabilities)
                sync_remote_worker_node(db, worker_id, capabilities, status="ONLINE")
                dispatch_routable_proposals(db)
                await ws.send_json({"type": "registered", "worker_id": worker_id})

            elif not authenticated:
                await ws.send_json({"type": "error", "detail": "Must register with auth_token first"})
                await ws.close(code=4001)
                return

            elif msg_type == "heartbeat":
                if worker_id:
                    capabilities = raw.get("capabilities") or {}
                    wm.heartbeat(worker_id, capabilities)
                    sync_remote_worker_node(
                        db,
                        worker_id,
                        wm.get_capabilities(worker_id),
                        status="ONLINE",
                    )
                    dispatch_routable_proposals(db)

            elif msg_type == "result":
                bus = get_bus()
                await bus.publish(Subject.TASK_EXEC_RESULT, raw.get("data", raw), sender_id=worker_id or "worker")

            elif msg_type == "llm_response":
                # Boost node responding to an LLM proxy request
                req_id = raw.get("request_id", "")
                text = raw.get("text", "")
                wm.resolve_llm_response(req_id, text)

            elif msg_type == "pull":
                await ws.send_json({"type": "no_work"})

    except WebSocketDisconnect:
        if worker_id:
            wm.unregister(worker_id)
            db.set_node_status(worker_id, "OFFLINE")
            dispatch_routable_proposals(db)
    except Exception as exc:
        logger.error("Worker socket error: %s", exc)
        if worker_id:
            wm.unregister(worker_id)
            db.set_node_status(worker_id, "OFFLINE")
            dispatch_routable_proposals(db)


def start_mcp_server():
    import uvicorn
    logger.info("Heiwa MCP Server booting on port 8000...")
    uvicorn.run(app, host="0.0.0.0", port=8000)


if __name__ == "__main__":
    start_mcp_server()
