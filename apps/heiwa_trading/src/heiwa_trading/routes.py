"""FastAPI routes for Heiwa Trading cockpit.

Mounted on the Hub at /trading/*.
All endpoints require auth via HEIWA_AUTH_TOKEN.
"""
from __future__ import annotations

import asyncio
import json
import logging
import os
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Header, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, StreamingResponse

from heiwa_trading.cockpit import (
    build_cockpit_snapshot,
    append_cockpit_chat_message,
)
from heiwa_trading.supervisor import supervisor_tick, load_supervisor_state
from heiwa_trading.market_data import fetch_markets

logger = logging.getLogger("Trading.Routes")

router = APIRouter(prefix="/trading", tags=["trading"])

WEB_DIR = Path(__file__).parent / "web"


def _check_auth(authorization: str | None) -> None:
    """Validate bearer token against HEIWA_AUTH_TOKEN."""
    expected = os.environ.get("HEIWA_AUTH_TOKEN", "")
    if not expected:
        raise HTTPException(status_code=500, detail="HEIWA_AUTH_TOKEN not configured")
    if not authorization:
        raise HTTPException(status_code=401, detail="Missing Authorization header")
    raw = authorization.removeprefix("Bearer ").strip()
    if raw != expected:
        raise HTTPException(status_code=403, detail="Invalid auth token")


@router.get("/cockpit")
async def cockpit_page(authorization: str | None = Header(None)):
    """Serve the trading cockpit HTML."""
    _check_auth(authorization)
    return FileResponse(WEB_DIR / "cockpit.html", media_type="text/html")


@router.get("/cockpit.css")
async def cockpit_css(authorization: str | None = Header(None)):
    _check_auth(authorization)
    return FileResponse(WEB_DIR / "cockpit.css", media_type="text/css")


@router.get("/cockpit.js")
async def cockpit_js(authorization: str | None = Header(None)):
    _check_auth(authorization)
    return FileResponse(WEB_DIR / "cockpit.js", media_type="application/javascript")


@router.get("/api/state")
async def trading_state(authorization: str | None = Header(None)):
    """Return current supervisor state as JSON."""
    _check_auth(authorization)
    snapshot = build_cockpit_snapshot()
    return snapshot


@router.post("/api/action")
async def trading_action(request: Request, authorization: str | None = Header(None)):
    """Handle operator control actions (tick, init cohort, etc.)."""
    _check_auth(authorization)
    body = await request.json()
    action = body.get("action", "")
    result: dict[str, object] = {"status": "ok", "action": action}

    if action == "tick":
        from datetime import datetime, timezone
        state = load_supervisor_state()
        markets = fetch_markets()
        ts = datetime.now(timezone.utc).isoformat()
        new_state, summary = supervisor_tick(state=state, markets=markets, timestamp=ts)
        result["message"] = "Tick completed"
        result["summary"] = summary
    elif action == "chat":
        text = body.get("text", "")
        append_cockpit_chat_message(role="operator", text=text)
        result["message"] = f"Chat: {text}"
    else:
        result["status"] = "unknown_action"

    return result


@router.get("/sse")
async def trading_sse(authorization: str | None = Header(None)):
    """SSE stream pushing cockpit state updates."""
    _check_auth(authorization)

    async def event_generator():
        while True:
            snapshot = build_cockpit_snapshot()
            data = json.dumps(snapshot)
            yield f"data: {data}\n\n"
            await asyncio.sleep(3)

    return StreamingResponse(
        event_generator(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
        },
    )
