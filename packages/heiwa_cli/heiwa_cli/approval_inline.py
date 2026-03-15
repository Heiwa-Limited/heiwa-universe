"""Inline approval prompts for the Heiwa REPL and one-shot flows."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from typing import Any

from prompt_toolkit import PromptSession
from prompt_toolkit.formatted_text import HTML
from rich.console import Console

from heiwa_cognition.approval import auto_approved, get_approval_registry
from heiwa_cli.context import CLIContext

console = Console(stderr=True)


@dataclass(slots=True)
class InlineApprovalResult:
    status: str
    reason: str | None = None


async def request_inline_approval(
    ctx: CLIContext,
    task_id: str,
    *,
    route: dict[str, Any],
    source: str,
    prompt_session: PromptSession | None = None,
) -> InlineApprovalResult:
    """Pause inline for approval and publish the decision locally or to the hub."""
    risk = str(route.get("risk_level") or "unknown")
    if not route.get("requires_approval") or auto_approved("cli", risk):
        return InlineApprovalResult(status="approved", reason="auto-approved")

    registry = get_approval_registry()
    if not registry.get_state(task_id):
        registry.add(
            task_id,
            {
                "approval_id": f"approval-{task_id}",
                "risk_level": risk,
                "source_surface": source,
                "requested_by": ctx.operator_name,
                "raw_text": route.get("raw_text") or route.get("prompt") or "",
            },
        )

    console.print(
        f"[yellow]Awaiting approval[/yellow] "
        f"[dim](risk: {risk}, intent: {route.get('intent_class', 'unknown')})[/dim]"
    )

    if prompt_session is not None:
        try:
            reply = await prompt_session.prompt_async(HTML("<ansiyellow>[approve? y/n]: </ansiyellow>"))
        except KeyboardInterrupt:
            return _leave_pending(ctx, task_id, source, route, reason="interrupted")
    else:
        try:
            reply = input("[approve? y/n]: ")
        except KeyboardInterrupt:
            return _leave_pending(ctx, task_id, source, route, reason="interrupted")

    answer = str(reply or "").strip().lower()
    if not answer:
        return _leave_pending(ctx, task_id, source, route, reason="empty")
    if answer in {"y", "yes"}:
        return await _publish_decision(ctx, task_id, approved=True, source=source, route=route)
    if answer in {"n", "no"}:
        return await _publish_decision(ctx, task_id, approved=False, source=source, route=route)

    return _leave_pending(ctx, task_id, source, route, reason="invalid")


def _leave_pending(
    ctx: CLIContext,
    task_id: str,
    source: str,
    route: dict[str, Any],
    *,
    reason: str,
) -> InlineApprovalResult:
    ctx.append_unsynced_approval_event(
        {
            "task_id": task_id,
            "status": "waiting_approval",
            "source": source,
            "reason": reason,
            "route": route,
            "created_at": time.time(),
        }
    )
    console.print("[yellow]Approval left pending.[/yellow]")
    return InlineApprovalResult(status="waiting_approval", reason=reason)


async def _publish_decision(
    ctx: CLIContext,
    task_id: str,
    *,
    approved: bool,
    source: str,
    route: dict[str, Any],
) -> InlineApprovalResult:
    registry = get_approval_registry()
    registry.decide(task_id, approved=approved, actor=ctx.operator_name, reason="inline_cli")

    for hub_url in ctx.hub_url_candidates():
        try:
            import httpx

            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await client.post(
                    f"{hub_url}/tasks/{task_id}/{'approve' if approved else 'reject'}",
                    json={"actor": ctx.operator_name, "reason": "inline_cli"},
                    headers={"Authorization": f"Bearer {ctx.auth_token}"},
                )
                if resp.status_code == 200:
                    console.print("[green]Approved.[/green]" if approved else "[yellow]Rejected.[/yellow]")
                    return InlineApprovalResult(status="approved" if approved else "rejected", reason="hub")
        except Exception:
            continue

    ctx.append_unsynced_approval_event(
        {
            "task_id": task_id,
            "status": "approved" if approved else "rejected",
            "source": source,
            "route": route,
            "created_at": time.time(),
            "synced": False,
        }
    )
    console.print(
        "[green]Approved locally; hub sync deferred.[/green]"
        if approved
        else "[yellow]Rejected locally; hub sync deferred.[/yellow]"
    )
    return InlineApprovalResult(status="approved" if approved else "rejected", reason="local")
