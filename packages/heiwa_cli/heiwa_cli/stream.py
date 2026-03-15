"""Streaming output renderer — handles both hub WebSocket and direct execution output."""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import Any

from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel

logger = logging.getLogger("CLI.Stream")
console = Console(stderr=True)


def render_output(output: str) -> str:
    """Extract readable text from potentially JSON-wrapped output."""
    text = str(output or "").strip()
    if not text:
        return ""
    start = text.find("{")
    end = text.rfind("}")
    if start != -1 and end > start:
        try:
            payload = json.loads(text[start : end + 1])
        except json.JSONDecodeError:
            return text
        payloads = payload.get("payloads")
        if isinstance(payloads, list):
            rendered = []
            for item in payloads:
                if isinstance(item, dict):
                    t = str(item.get("text") or "").strip()
                    if t:
                        rendered.append(t)
            if rendered:
                return "\n\n".join(rendered)
    return text


async def stream_from_hub(
    hub_url: str,
    task_id: str,
    token: str = "",
    timeout: float = 120.0,
    trace: bool = False,
) -> tuple[int, str]:
    """Connect to WS /ws/tasks/{task_id} and stream events. Returns (exit_code, summary)."""
    ws_url = hub_url.replace("https://", "wss://").replace("http://", "ws://")
    ws_url = f"{ws_url}/ws/tasks/{task_id}?token={token}"

    try:
        import websockets
    except ImportError:
        return await poll_from_hub(hub_url, task_id, token=token, timeout=timeout, trace=trace)

    try:
        async with websockets.connect(ws_url, open_timeout=10) as ws:
            deadline = time.time() + timeout
            while time.time() < deadline:
                try:
                    raw = await asyncio.wait_for(ws.recv(), timeout=35.0)
                except asyncio.TimeoutError:
                    continue
                event = json.loads(raw)
                status = event.get("status", "")
                evt_type = event.get("type", "")

                if evt_type == "heartbeat":
                    continue

                if status in {"ACKNOWLEDGED", "DISPATCHED_PLAN", "DISPATCHED_FALLBACK"}:
                    if trace:
                        console.print(f"[dim]{event.get('message', status)}[/dim]")
                elif status == "AWAITING_APPROVAL":
                    console.print(
                        f"[yellow]Awaiting approval.[/yellow] "
                        f"[dim]/approve {task_id}  ·  /reject {task_id}[/dim]"
                    )
                    return 2, ""
                elif status in {"REJECTED", "EXPIRED"}:
                    msg = render_output(event.get("message", "")) or "halted"
                    console.print(f"[yellow]{status}: {msg}[/yellow]")
                    return 1, ""
                elif status in {"DELIVERED", "PASS"}:
                    summary = render_output(event.get("summary", ""))
                    if summary:
                        console.print(Panel(Markdown(summary), border_style="green"))
                    return 0, summary
                elif status in {"FAIL", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}:
                    msg = render_output(event.get("message", ""))
                    console.print(f"[red]{status}: {msg}[/red]")
                    return 1, ""
                else:
                    content = event.get("content") or event.get("message") or ""
                    if content and trace:
                        console.print(f"[dim]{content}[/dim]")
    except Exception as e:
        logger.debug("WS error: %s — falling back to polling", e)

    return await poll_from_hub(hub_url, task_id, token=token, timeout=timeout, trace=trace)


async def poll_from_hub(
    hub_url: str,
    task_id: str,
    token: str = "",
    timeout: float = 120.0,
    trace: bool = False,
) -> tuple[int, str]:
    """Poll GET /tasks/{task_id} as fallback when WebSocket is unavailable."""
    import urllib.request
    import urllib.error

    headers = {"Authorization": f"Bearer {token}"} if token else {}
    deadline = time.time() + timeout

    while time.time() < deadline:
        try:
            req = urllib.request.Request(f"{hub_url}/tasks/{task_id}", headers=headers)
            with urllib.request.urlopen(req, timeout=10) as resp:
                payload = json.loads(resp.read().decode())
        except Exception:
            await asyncio.sleep(2.0)
            continue

        status = str(payload.get("status") or payload.get("run_status") or "").upper()
        summary = render_output(
            str(payload.get("summary") or payload.get("result") or payload.get("content") or "")
        )

        if status in {"PASS", "SUCCESS", "COMPLETED"}:
            if summary:
                console.print(Panel(Markdown(summary), border_style="green"))
            return 0, summary
        if status == "AWAITING_APPROVAL":
            console.print(
                f"[yellow]Awaiting approval.[/yellow] "
                f"[dim]/approve {task_id}  ·  /reject {task_id}[/dim]"
            )
            return 2, ""
        if status in {"REJECTED", "EXPIRED"}:
            console.print(f"[yellow]{status}: {summary or 'halted'}[/yellow]")
            return 1, ""
        if status in {"FAIL", "ERROR", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}:
            console.print(f"[red]{status}: {summary or 'failed'}[/red]")
            return 1, ""

        await asyncio.sleep(2.0)

    console.print("[yellow]Timed out waiting for result.[/yellow]")
    return 1, ""
