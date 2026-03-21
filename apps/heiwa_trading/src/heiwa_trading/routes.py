"""FastAPI routes for the Heiwa Trading cockpit service.

Static assets (HTML/CSS/JS) are public — auth is handled client-side via localStorage.
API endpoints require Bearer auth via HEIWA_AUTH_TOKEN.
"""
from __future__ import annotations

import asyncio
import hmac
import json
import logging
import os
import time
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Header, HTTPException, Query, Request
from fastapi.responses import FileResponse, HTMLResponse, StreamingResponse

from heiwa_trading.cockpit import (
    MAC_AGENT_ROOT,
    build_cockpit_snapshot,
    append_cockpit_chat_message,
    load_cockpit_settings,
    run_cockpit_action,
)
from heiwa_trading.supervisor import supervisor_tick, load_supervisor_state, save_supervisor_state
from heiwa_trading.market_data import fetch_markets

logger = logging.getLogger("Trading.Routes")

router = APIRouter(prefix="/trading", tags=["trading"])

WEB_DIR = Path(__file__).parent / "web"


def _constant_time_compare(a: str, b: str) -> bool:
    """Timing-safe string comparison to prevent timing attacks."""
    return hmac.compare_digest(a.encode(), b.encode())


# Simple in-memory rate limiter for auth attempts
_auth_attempts: dict[str, list[float]] = {}
_AUTH_MAX_ATTEMPTS = 5
_AUTH_WINDOW_SEC = 300  # 5 minutes


def _check_auth_rate_limit(client_ip: str) -> None:
    """Block brute-force auth attempts: max 5 per 5 minutes per IP."""
    now = time.time()
    attempts = _auth_attempts.get(client_ip, [])
    # Prune old attempts
    attempts = [t for t in attempts if now - t < _AUTH_WINDOW_SEC]
    _auth_attempts[client_ip] = attempts
    if len(attempts) >= _AUTH_MAX_ATTEMPTS:
        logger.warning("Auth rate limit hit for %s", client_ip)
        raise HTTPException(status_code=429, detail="Too many attempts. Try again later.")


def _record_auth_attempt(client_ip: str) -> None:
    """Record a failed auth attempt."""
    _auth_attempts.setdefault(client_ip, []).append(time.time())


def _check_auth(authorization: str | None) -> None:
    """Validate bearer token against HEIWA_AUTH_TOKEN."""
    expected = os.environ.get("HEIWA_AUTH_TOKEN", "")
    if not expected:
        raise HTTPException(status_code=500, detail="HEIWA_AUTH_TOKEN not configured")
    if not authorization:
        raise HTTPException(status_code=401, detail="Missing Authorization header")
    raw = authorization.removeprefix("Bearer ").strip()
    if not _constant_time_compare(raw, expected):
        raise HTTPException(status_code=403, detail="Invalid auth token")


# --- Static assets (public — auth handled in JS via localStorage) ---

@router.get("/cockpit")
async def cockpit_page():
    """Serve the trading cockpit HTML."""
    return FileResponse(WEB_DIR / "cockpit.html", media_type="text/html")


@router.get("/cockpit.css")
async def cockpit_css():
    return FileResponse(WEB_DIR / "cockpit.css", media_type="text/css")


@router.get("/cockpit.js")
async def cockpit_js():
    return FileResponse(WEB_DIR / "cockpit.js", media_type="application/javascript")


# --- Auth validation endpoint (for the JS login gate) ---

@router.post("/api/auth")
async def validate_token(request: Request):
    """Validate a token without exposing internals. Used by the login gate."""
    client_ip = request.client.host if request.client else "unknown"
    _check_auth_rate_limit(client_ip)

    body = await request.json()
    token = body.get("token", "")
    expected = os.environ.get("HEIWA_AUTH_TOKEN", "")
    if not expected or not _constant_time_compare(token, expected):
        _record_auth_attempt(client_ip)
        raise HTTPException(status_code=403, detail="Invalid token")
    return {"status": "ok"}


# --- API endpoints (auth required) ---

@router.get("/api/state")
async def trading_state(authorization: str | None = Header(None)):
    """Return current supervisor state as JSON."""
    _check_auth(authorization)
    snapshot = build_cockpit_snapshot()
    return snapshot


@router.get("/api/settings")
async def trading_settings(authorization: str | None = Header(None)):
    """Return persisted cockpit settings for the current operator."""
    _check_auth(authorization)
    return load_cockpit_settings()


@router.post("/api/action")
async def trading_action(request: Request, authorization: str | None = Header(None)):
    """Handle operator control actions (tick, init cohort, etc.)."""
    _check_auth(authorization)
    body = await request.json()
    action = body.get("action", "")
    if action == "tick":
        from datetime import datetime, timezone
        state = load_supervisor_state()
        markets = fetch_markets()
        ts = datetime.now(timezone.utc).isoformat()
        new_state, summary = supervisor_tick(state=state, markets=markets, timestamp=ts)
        save_supervisor_state(new_state)
        return {
            "status": "ok",
            "action": action,
            "message": "Tick completed",
            "summary": summary,
        }
    if action == "chat":
        text = body.get("text", "")
        append_cockpit_chat_message(role="operator", text=text)
        return {"status": "ok", "action": action, "message": f"Chat: {text}"}

    try:
        result = run_cockpit_action(MAC_AGENT_ROOT, action=str(action), payload=dict(body))
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return {"status": "ok", **result}


@router.get("/sse")
async def trading_sse(token: str | None = Query(None)):
    """SSE stream pushing cockpit state updates.

    Auth via query param: /trading/sse?token=<bearer>
    (SSE EventSource API doesn't support custom headers)
    """
    expected = os.environ.get("HEIWA_AUTH_TOKEN", "")
    if not expected or not token or token != expected:
        raise HTTPException(status_code=401, detail="Missing or invalid token")

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
