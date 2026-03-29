"""One-shot execution: heiwa "do something" → result → exit."""

from __future__ import annotations

import asyncio
import json
import logging
import sys
import time
import uuid

from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel

from heiwa_cli.approval_inline import request_inline_approval
from heiwa_cli.context import CLIContext
from heiwa_cli.stream import render_output
from heiwa_cli.wire import WireClient

logger = logging.getLogger("CLI")
console = Console(stderr=True)


def _trace(ctx: CLIContext, msg: str) -> None:
    if ctx.trace:
        console.print(f"[dim]{msg}[/dim]")


async def _submit_to_hub(ctx: CLIContext, prompt: str, task_id: str, hub_url: str) -> dict | None:
    """POST task to hub."""
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {ctx.auth_token}",
    }
    body = json.dumps({
        "raw_text": prompt,
        "sender_id": ctx.node_id,
        "source_surface": "cli",
        "task_id": task_id,
    }).encode()

    try:
        import httpx
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.post(f"{hub_url}/tasks", content=body, headers=headers)
            if resp.status_code == 200:
                return resp.json()
            _trace(ctx, f"Hub returned HTTP {resp.status_code}")
            return None
    except ImportError:
        pass
    except Exception as e:
        _trace(ctx, f"httpx error: {e}")

    # Fallback to urllib
    try:
        import urllib.request
        req = urllib.request.Request(f"{hub_url}/tasks", data=body, headers=headers, method="POST")
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())
    except Exception:
        return None


async def _direct_execute(ctx: CLIContext, prompt: str, task_id: str) -> int:
    """Enrich and execute locally without the hub."""
    from heiwa_protocol.routing import BrokerRouteRequest

    ctx.remember_turn("user", prompt)
    request = BrokerRouteRequest(
        request_id=f"cli-{task_id}",
        task_id=task_id,
        raw_text=prompt,
        sender_id=ctx.node_id,
        source_surface="cli",
        auth_validated=bool(ctx.auth_token),
    )
    route = ctx.enrichment.enrich(request)
    dispatch = ctx.gateway.resolve(route)
    identity = ctx.identity_selector.select(route.intent_class, prompt)

    _trace(ctx, f"route {route.intent_class} → {dispatch.provider} → {dispatch.target_model or 'default'}")

    try:
        ctx.db.create_mission(
            {
                "mission_id": task_id,
                "status": "waiting_approval" if route.requires_approval else "running",
                "source_surface": "cli",
                "node_id": ctx.node_id,
                "prompt": prompt,
                "intent_class": route.intent_class,
                "risk_level": route.risk_level,
                "active_cell_id": identity.id,
                "target_tool": dispatch.adapter_tool,
                "target_model": dispatch.target_model,
                "metadata": {"route": route.to_dict(), "dispatch": dispatch.to_dict()},
            }
        )
    except Exception:
        logger.debug("Failed to create mission %s", task_id, exc_info=True)

    approval = await request_inline_approval(
        ctx,
        task_id,
        route={**route.to_dict(), "raw_text": prompt},
        source="local",
        prompt_session=None,
    )
    if approval.status != "approved":
        try:
            ctx.db.pause_mission(task_id, summary="Waiting for inline approval")
        except Exception:
            logger.debug("Failed to pause mission %s", task_id, exc_info=True)
        return 1

    cell_run_id = f"cellrun-{uuid.uuid4().hex[:10]}"
    try:
        ctx.db.start_cell_run(
            {
                "cell_run_id": cell_run_id,
                "mission_id": task_id,
                "step_id": f"{task_id}:execute",
                "status": "running",
                "cell_id": identity.id,
                "cell_role": identity.cells[0].role if identity.cells else "builder",
                "provider": dispatch.provider,
                "model": dispatch.target_model,
                "rate_group": dispatch.rate_group,
                "tool": dispatch.adapter_tool,
            }
        )
    except Exception:
        logger.debug("Failed to start cell run %s", cell_run_id, exc_info=True)

    system_prompt = ctx.build_system_prompt(
        identity_id=identity.id,
        intent_class=route.intent_class,
        risk_level=route.risk_level,
    )

    # Stream tokens live for local Ollama models
    streamed_tokens: list[str] = []
    is_streaming = False

    def _on_token(token: str) -> None:
        nonlocal is_streaming
        if not is_streaming:
            is_streaming = True
            console.print()  # blank line before streaming output
        streamed_tokens.append(token)
        sys.stdout.write(token)
        sys.stdout.flush()

    result = await ctx.gateway.execute_streaming(
        route, prompt, system_prompt=system_prompt, on_token=_on_token,
    )
    exit_code, output = result.exit_code, result.output

    if is_streaming:
        sys.stdout.write("\n")
        sys.stdout.flush()
        usage_line = result.usage.summary_line()
        if usage_line:
            console.print(f"[dim]{usage_line}[/dim]")
        rendered = "".join(streamed_tokens)
    else:
        rendered = render_output(output)

    try:
        ctx.db.finish_cell_run(
            {
                "cell_run_id": cell_run_id,
                "status": "completed" if exit_code == 0 else "failed",
                "output_summary": rendered,
            }
        )
        if exit_code == 0:
            ctx.db.complete_mission(task_id, summary=rendered[:500] if rendered else "Completed")
        else:
            ctx.db.fail_mission(task_id, error=rendered[:500] if rendered else "Execution failed")
        ctx.db.register_artifact(
            {
                "artifact_id": f"artifact-{uuid.uuid4().hex[:10]}",
                "mission_id": task_id,
                "cell_run_id": cell_run_id,
                "artifact_type": "summary",
                "title": "One-shot execution summary",
                "content": {"markdown": rendered or output, "exit_code": exit_code},
            }
        )
    except Exception:
        logger.debug("Failed to record execution results for %s", task_id, exc_info=True)
    if rendered:
        ctx.remember_turn("assistant", rendered, provider=dispatch.provider, model=dispatch.target_model)
        if not is_streaming:
            console.print(
                Panel(
                    Markdown(rendered),
                    border_style="green" if exit_code == 0 else "red",
                )
            )
    return exit_code


async def dispatch_once(ctx: CLIContext, prompt: str) -> int:
    """One-shot: classify, route, execute, exit."""
    from heiwa_sdk.operator_surface import maybe_fast_path_turn

    task_id = f"cli-task-{uuid.uuid4().hex[:8]}"

    # Fast path for greetings
    fast = maybe_fast_path_turn(prompt, ctx.node_id)
    if fast:
        ctx.remember_turn("user", prompt)
        ctx.remember_turn("assistant", fast.response, fast_path=True)
        print(fast.response)
        return 0

    # single_node mode: skip hub entirely, execute locally with enrichment
    if ctx.mode == "single_node":
        return await _direct_execute(ctx, prompt, task_id)

    # distributed mode: try hub first, fall back to direct
    for hub_url in ctx.hub_url_candidates():
        battlefield = ctx.battlefield_registration()
        wire = WireClient(
            base_url=hub_url,
            token=ctx.auth_token,
            battlefield_id=battlefield["battlefield_id"],
            session_id=ctx.session_id,
        )
        try:
            await wire.connect()
            await wire.register_battlefield(battlefield)
            result = await wire.submit_task(
                raw_text=prompt,
                sender_id=ctx.node_id,
                source_surface="cli",
                task_id=task_id,
            )
            if result:
                accepted_id = result.get("task_id", task_id)
                route = result.get("route", {})
                _trace(ctx, f"route {route.get('intent_class', '?')} → {route.get('target_tool', '?')} → {route.get('target_model', 'default')}")
                exit_code, summary = await wire.wait_for_task(accepted_id, trace=ctx.trace)
                if exit_code == 2:
                    approval = await request_inline_approval(
                        ctx,
                        accepted_id,
                        route={**route, "raw_text": prompt},
                        source="hub",
                        prompt_session=None,
                    )
                    if approval.status == "approved":
                        exit_code, summary = await wire.wait_for_task(accepted_id, trace=ctx.trace)
                    else:
                        return 1
                if summary:
                    ctx.remember_turn("assistant", summary, provider=route.get("target_tool"))
                return exit_code
        except Exception as e:
            _trace(ctx, f"Hub {hub_url}: {e}")
        finally:
            await wire.close()

    _trace(ctx, "Hub unreachable — direct local route.")
    return await _direct_execute(ctx, prompt, task_id)
