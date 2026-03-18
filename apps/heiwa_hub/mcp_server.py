import asyncio
import json
import logging
import time
from pathlib import Path
from typing import Any, Dict

import os
import uuid

import subprocess
import sys
from fastapi import FastAPI, HTTPException, Header, WebSocket, WebSocketDisconnect
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
from heiwa_sdk.heiwaclaw import HeiwaClawGateway
from heiwa_protocol.routing import BrokerRouteRequest

from heiwa_hub.cognition.enrichment import BrokerEnrichmentService
from heiwa_hub.cognition.approval import get_approval_registry
from heiwa_hub.transport import get_bus, get_worker_manager
from heiwa_protocol.protocol import Subject

# Initialized Environment
load_swarm_env()

logger = logging.getLogger("Hub.MCP")
app = FastAPI(title="Heiwa Core MCP Server")
db = Database()
state = HubStateService(db)
mcp_bridge = MCPBridge()
enrichment = BrokerEnrichmentService()
approvals = get_approval_registry()
TASK_SNAPSHOTS: dict[str, dict[str, Any]] = {}

ROOT = Path(__file__).resolve().parents[2]
bench = HeiwaBench(ROOT)
cells = HeiwaCellCatalog(ROOT)
claw_gateway = HeiwaClawGateway(ROOT)
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


def _operator_snapshot_payload() -> dict[str, Any]:
    from heiwa_sdk.rate_ledger import get_rate_ledger

    return {
        "providers": state.get_provider_accounts(),
        "missions": state.get_missions(limit=100),
        "approvals": [item for item in (_serialize_approval(task_id) for task_id in TASK_SNAPSHOTS) if item],
        "live_tasks": list(TASK_SNAPSHOTS.values())[-100:],
        "rate_groups": get_rate_ledger().status(),
        "history": state.get_history(limit=25),
        "timestamp": time.time(),
    }


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
    try:
        summary = str(payload.get("summary") or "")
        status = str(payload.get("status") or "").upper()
        db.finish_cell_run(
            {
                "cell_run_id": f"cellrun-{task_id}",
                "status": "completed" if status in {"PASS", "DELIVERED"} else "failed",
                "output_summary": summary,
            }
        )
        if status in {"PASS", "DELIVERED"}:
            db.complete_mission(task_id, summary=summary[:500] if summary else "Completed")
        elif status in {"FAIL", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}:
            db.fail_mission(task_id, error=summary[:500] if summary else status)
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


async def _record_progress_event(envelope: Dict[str, Any]):
    payload = envelope.get("data", envelope)
    task_id = payload.get("task_id")
    if not task_id:
        return
    _snapshot_task(task_id, progress=payload.get("content") or payload.get("message"))


@app.on_event("startup")
async def _register_task_snapshot_listeners():
    if getattr(app.state, "_task_snapshot_listeners", False):
        return
    bus = get_bus()
    await bus.subscribe(Subject.TASK_STATUS, _record_status_event)
    await bus.subscribe(Subject.TASK_EXEC_RESULT, _record_result_event)
    await bus.subscribe(Subject.TASK_PROGRESS, _record_progress_event)
    app.state._task_snapshot_listeners = True


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


@app.get("/")
@app.head("/")
async def root():
    index = _web_file("index.html")
    if index:
        return FileResponse(index)
    return {"name": "Heiwa", "status": "operational"}


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
async def status_stream(ws: WebSocket):
    await ws.accept()
    try:
        while True:
            await ws.send_json(_public_status_payload())
            await asyncio.sleep(2)
    except WebSocketDisconnect:
        logger.debug("Status websocket disconnected.")


@app.get("/tools")
async def list_tools():
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
async def call_tool(tool_name: str, arguments: Dict[str, Any]):
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
    Returns the expected token on success, raises HTTPException on failure."""
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
    return expected


# ---------------------------------------------------------------------------
#  Task ingress
# ---------------------------------------------------------------------------

class TaskRequest(BaseModel):
    raw_text: str
    sender_id: str = "cli"
    source_surface: str = "cli"
    privacy_level: str | None = None
    task_id: str | None = None


class ApprovalDecisionRequest(BaseModel):
    actor: str = "operator"
    reason: str | None = None


@app.post("/tasks")
async def create_task(req: TaskRequest, authorization: str | None = Header(None)):
    """Authenticated task ingress. Enriches once here — Spine skips re-enrichment."""
    auth_token = _validate_auth_token(authorization)
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
                }
            )
    except Exception:
        logger.exception("Failed to persist mission bootstrap for %s", task_id)

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
    items = []
    for state in approvals.list_states(status=status):
        serialized = _serialize_approval(state.task_id)
        if serialized:
            items.append(serialized)
    return {"approvals": items}


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
async def list_provider_accounts(authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    return {"providers": state.get_provider_accounts()}


@app.get("/auth/providers/{provider_id}/status")
async def get_provider_account_status(provider_id: str, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    payload = state.get_provider_status(provider_id)
    if not payload:
        raise HTTPException(status_code=404, detail=f"Provider {provider_id} not found")
    return payload


@app.get("/missions")
async def list_missions(status: str | None = None, limit: int = 50, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    return {"missions": state.get_missions(status=status, limit=limit)}


@app.get("/missions/{mission_id}")
async def get_mission(mission_id: str, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    payload = state.get_mission_detail(mission_id)
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
async def get_history(limit: int = 20, authorization: str | None = Header(None)):
    _validate_auth_token(authorization)
    return state.get_history(limit=limit)


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


@app.websocket("/ws/operator")
async def operator_events(ws: WebSocket, token: str | None = None):
    expected = os.getenv("HEIWA_AUTH_TOKEN", "")
    if not expected or not token or token != expected:
        await ws.close(code=4003)
        return

    await ws.accept()
    try:
        while True:
            await ws.send_json(_operator_snapshot_payload())
            await asyncio.sleep(2)
    except WebSocketDisconnect:
        logger.debug("Operator websocket disconnected.")


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
                wm.register(worker_id, ws, raw.get("capabilities", {}))
                await ws.send_json({"type": "registered", "worker_id": worker_id})

            elif not authenticated:
                await ws.send_json({"type": "error", "detail": "Must register with auth_token first"})
                await ws.close(code=4001)
                return

            elif msg_type == "heartbeat":
                if worker_id:
                    wm.heartbeat(worker_id)

            elif msg_type == "result":
                bus = get_bus()
                await bus.publish(Subject.TASK_EXEC_RESULT, raw.get("data", raw), sender_id=worker_id or "worker")

            elif msg_type == "pull":
                await ws.send_json({"type": "no_work"})

    except WebSocketDisconnect:
        if worker_id:
            wm.unregister(worker_id)
    except Exception as exc:
        logger.error("Worker socket error: %s", exc)
        if worker_id:
            wm.unregister(worker_id)


def start_mcp_server():
    import uvicorn
    logger.info("Heiwa MCP Server booting on port 8000...")
    uvicorn.run(app, host="0.0.0.0", port=8000)


if __name__ == "__main__":
    start_mcp_server()
