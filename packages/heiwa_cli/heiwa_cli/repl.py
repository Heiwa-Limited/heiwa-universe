"""Heiwa interactive REPL — one text box, slash commands inline."""

from __future__ import annotations

import asyncio
import json
import logging
import time
import uuid
from pathlib import Path
from typing import Any

logger = logging.getLogger("CLI")

from prompt_toolkit import PromptSession
from prompt_toolkit.auto_suggest import AutoSuggestFromHistory
from prompt_toolkit.completion import Completer, Completion
from prompt_toolkit.formatted_text import HTML
from prompt_toolkit.history import FileHistory
from prompt_toolkit.patch_stdout import patch_stdout

from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel
from rich.text import Text

from heiwa_cli.approval_inline import request_inline_approval
from heiwa_cli.commands import command_names, get_commands
from heiwa_cli.context import CLIContext
from heiwa_cli.status_line import StatusLine
from heiwa_cli.stream import render_output, stream_from_hub

console = Console(stderr=True)

WELCOME_SUGGESTIONS = (
    "what should I work on next in Heiwa",
    "research how SpacetimeDB subscriptions work",
    "fix the SQLite fallback in db.py and verify it",
    "summarize Heiwa status and propose the next highest-leverage fix",
    "deploy the hub and report the result",
)


class HeiwaCompleter(Completer):
    def get_completions(self, document, complete_event):
        text = document.text_before_cursor
        if text.startswith("/"):
            for cmd in command_names():
                if cmd.startswith(text):
                    yield Completion(cmd, start_position=-len(text))


class HeiwaREPL:
    """Interactive REPL — the conversation IS the agent."""

    def __init__(self, ctx: CLIContext) -> None:
        self.ctx = ctx
        self.running = True
        self.status = StatusLine()

        history_path = Path.home() / ".heiwa" / "cli_history"
        history_path.parent.mkdir(parents=True, exist_ok=True)
        self.session = PromptSession(
            history=FileHistory(str(history_path)),
            auto_suggest=AutoSuggestFromHistory(),
            completer=HeiwaCompleter(),
            refresh_interval=0.5,
            bottom_toolbar=self.status.toolbar,
        )

    def _show_welcome(self) -> None:
        suggestions = "\n".join(f"  `{p}`" for p in WELCOME_SUGGESTIONS)
        header = Text.assemble(
            ("HEIWA", "bold cyan"),
            (" ", ""),
            ("single ingress \u00b7 route-aware \u00b7 scale-zero first", "dim"),
        )
        console.print(Panel(header, border_style="blue", padding=(0, 1)))
        console.print(Panel(
            Markdown(
                f"**Try these first**\n\n{suggestions}\n\n"
                "`/help` commands  \u00b7  `/status` health  \u00b7  `/models` registry  \u00b7  ctrl-d exit"
            ),
            title=f"[bold]{self.ctx.operator_name}[/bold]",
            subtitle=f"[dim]{self.ctx.node_id} \u00b7 {self.ctx.session_id}[/dim]",
            border_style="dim cyan",
            padding=(0, 1),
        ))

    async def _send_task(self, text: str) -> None:
        """Route input to the optimal model and display the result."""
        from heiwa_sdk.operator_surface import maybe_fast_path_turn

        self.status.turns += 1
        self.ctx.remember_turn("user", text)

        # Fast path — greetings, thanks, help
        fast = maybe_fast_path_turn(text, self.ctx.node_id)
        if fast:
            self.status.update(intent=fast.intent, tool="local", status="pass", latency_ms=0, identity="operator-general")
            self.ctx.remember_turn("assistant", fast.response, fast_path=True)
            console.print(Panel(
                Markdown(fast.response),
                border_style="cyan",
                padding=(0, 1),
            ))
            return

        started = time.perf_counter()
        task_id = f"cli-task-{uuid.uuid4().hex[:8]}"

        # single_node mode: skip hub, execute locally with enrichment
        if self.ctx.mode == "single_node":
            await self._direct_execute(text, task_id, started)
            return

        # distributed mode: try hub first, fall back to direct
        for hub_url in self.ctx.hub_url_candidates():
            try:
                result = await self._submit_to_hub(text, task_id, hub_url)
                if result:
                    route = result.get("route", {})
                    accepted_id = result.get("task_id", task_id)
                    self.status.update(
                        intent=str(route.get("intent_class", "?")),
                        tool=str(route.get("target_tool", "?")),
                        model=str(route.get("target_model", "auto")),
                        status="accepted",
                        hub=hub_url.replace("https://", "").replace("http://", "").strip("/"),
                        latency_ms=int((time.perf_counter() - started) * 1000),
                    )
                    if self.ctx.trace:
                        console.print(
                            f"[dim]route {route.get('intent_class', '?')} \u2192 "
                            f"{route.get('target_tool', '?')} \u2192 "
                            f"{route.get('target_model', 'default')} | hub[/dim]"
                        )
                    exit_code, summary = await stream_from_hub(hub_url, accepted_id, token=self.ctx.auth_token, trace=self.ctx.trace)
                    if exit_code == 2:
                        approval = await request_inline_approval(
                            self.ctx,
                            accepted_id,
                            route={**route, "raw_text": text},
                            source="hub",
                            prompt_session=self.session,
                        )
                        if approval.status == "approved":
                            exit_code, summary = await stream_from_hub(
                                hub_url,
                                accepted_id,
                                token=self.ctx.auth_token,
                                trace=self.ctx.trace,
                            )
                        else:
                            self.status.update(status=approval.status)
                            return
                    self.status.update(status="pass" if exit_code == 0 else "fail")
                    if summary:
                        self.ctx.remember_turn("assistant", summary, provider=route.get("target_tool"))
                    return
            except Exception:
                logger.debug("Hub %s failed", hub_url, exc_info=True)
                continue

        # Fallback to direct local execution
        await self._direct_execute(text, task_id, started)

    async def _submit_to_hub(self, text: str, task_id: str, hub_url: str) -> dict | None:
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.ctx.auth_token}",
        }
        body = json.dumps({
            "raw_text": text,
            "sender_id": self.ctx.node_id,
            "source_surface": "cli",
            "task_id": task_id,
        }).encode()

        try:
            import httpx
            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await client.post(f"{hub_url}/tasks", content=body, headers=headers)
                if resp.status_code == 200:
                    return resp.json()
        except ImportError:
            import urllib.request
            req = urllib.request.Request(f"{hub_url}/tasks", data=body, headers=headers, method="POST")
            try:
                with urllib.request.urlopen(req, timeout=10) as resp:
                    return json.loads(resp.read().decode())
            except Exception:
                logger.debug("urllib hub submit failed for %s", hub_url, exc_info=True)
        except Exception:
            logger.debug("httpx hub submit failed for %s", hub_url, exc_info=True)
        return None

    async def _direct_execute(self, text: str, task_id: str, started: float) -> None:
        from heiwa_protocol.routing import BrokerRouteRequest

        if self.ctx.trace:
            console.print("[yellow]Hub unreachable \u2014 direct local route.[/yellow]")

        request = BrokerRouteRequest(
            request_id=f"cli-{task_id}",
            task_id=task_id,
            raw_text=text,
            sender_id=self.ctx.node_id,
            source_surface="cli",
            auth_validated=bool(self.ctx.auth_token),
        )
        route = self.ctx.enrichment.enrich(request)
        dispatch = self.ctx.gateway.resolve(route)
        identity = self.ctx.identity_selector.select(route.intent_class, text)
        mission_metadata = {"route": route.to_dict(), "dispatch": dispatch.to_dict(), "identity": identity.id}
        try:
            self.ctx.db.create_mission(
                {
                    "mission_id": task_id,
                    "status": "waiting_approval" if route.requires_approval else "running",
                    "source_surface": "cli",
                    "node_id": self.ctx.node_id,
                    "prompt": text,
                    "intent_class": route.intent_class,
                    "risk_level": route.risk_level,
                    "active_cell_id": identity.id,
                    "target_tool": dispatch.adapter_tool,
                    "target_model": dispatch.target_model,
                    "metadata": mission_metadata,
                }
            )
            self.ctx.db.append_mission_step(
                {
                    "step_id": f"{task_id}:ingress",
                    "mission_id": task_id,
                    "position": 1,
                    "status": "completed",
                    "step_kind": "ingress",
                    "cell_role": identity.cells[0].role if identity.cells else "preflight",
                    "title": "Ingress",
                    "detail": f"{route.intent_class} via CLI",
                    "input": {"prompt": text},
                    "output": route.to_dict(),
                }
            )
        except Exception:
            logger.debug("Failed to record mission/ingress for %s", task_id, exc_info=True)

        self.status.update(
            intent=route.intent_class,
            tool=dispatch.adapter_tool,
            model=dispatch.target_model or "default",
            status="running",
            identity=identity.id,
            latency_ms=int((time.perf_counter() - started) * 1000),
        )
        if self.ctx.trace:
            console.print(
                f"[dim]{identity.id} \u2192 {route.intent_class} \u2192 {dispatch.provider} \u2192 "
                f"{dispatch.target_model or 'default'} | {dispatch.transport}[/dim]"
            )

        approval = await request_inline_approval(
            self.ctx,
            task_id,
            route={**route.to_dict(), "raw_text": text},
            source="local",
            prompt_session=self.session,
        )
        if approval.status != "approved":
            try:
                if approval.status == "rejected":
                    self.ctx.db.pause_mission(task_id, summary="Rejected in inline approval")
                else:
                    self.ctx.db.pause_mission(task_id, summary="Awaiting inline approval")
            except Exception:
                logger.debug("Failed to pause mission %s", task_id, exc_info=True)
            self.status.update(status=approval.status)
            return

        cell_run_id = f"cellrun-{uuid.uuid4().hex[:10]}"
        try:
            self.ctx.db.start_cell_run(
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
            self.ctx.db.append_mission_step(
                {
                    "step_id": f"{task_id}:execute",
                    "mission_id": task_id,
                    "position": 2,
                    "status": "running",
                    "step_kind": "execute",
                    "cell_role": identity.cells[0].role if identity.cells else "builder",
                    "title": "Execution",
                    "detail": f"{dispatch.provider} {dispatch.target_model}",
                    "input": {"prompt": text},
                    "output": {},
                }
            )
        except Exception:
            logger.debug("Failed to start cell run %s", cell_run_id, exc_info=True)

        system_prompt = self.ctx.build_system_prompt(
            identity_id=identity.id,
            intent_class=route.intent_class,
            risk_level=route.risk_level,
        )

        # Stream tokens live for local Ollama models
        import sys as _sys
        streamed_tokens: list[str] = []
        is_streaming = False

        def _on_token(token: str) -> None:
            nonlocal is_streaming
            if not is_streaming:
                is_streaming = True
                console.print()  # blank line before streaming output
            streamed_tokens.append(token)
            _sys.stdout.write(token)
            _sys.stdout.flush()

        result = await self.ctx.gateway.execute_streaming(
            route, text, system_prompt=system_prompt, on_token=_on_token,
        )
        exit_code, output = result.exit_code, result.output
        self.status.update(status="pass" if exit_code == 0 else "fail")

        if is_streaming:
            _sys.stdout.write("\n")
            _sys.stdout.flush()
            usage_line = result.usage.summary_line()
            if usage_line:
                console.print(f"[dim]{usage_line}[/dim]")
            rendered = "".join(streamed_tokens)
        else:
            rendered = render_output(output)

        try:
            self.ctx.db.finish_cell_run(
                {
                    "cell_run_id": cell_run_id,
                    "status": "completed" if exit_code == 0 else "failed",
                    "output_summary": rendered,
                }
            )
            if exit_code == 0:
                self.ctx.db.complete_mission(task_id, summary=rendered[:500] if rendered else "Completed")
            else:
                self.ctx.db.fail_mission(task_id, error=rendered[:500] if rendered else "Execution failed")
            self.ctx.db.register_artifact(
                {
                    "artifact_id": f"artifact-{uuid.uuid4().hex[:10]}",
                    "mission_id": task_id,
                    "cell_run_id": cell_run_id,
                    "artifact_type": "summary",
                    "title": "CLI execution summary",
                    "content": {"markdown": rendered or output, "exit_code": exit_code},
                }
            )
        except Exception:
            logger.debug("Failed to record execution results for %s", task_id, exc_info=True)
        if rendered:
            self.ctx.remember_turn("assistant", rendered, provider=dispatch.provider, model=dispatch.target_model)
            if not is_streaming:
                console.print(Panel(
                    Markdown(rendered),
                    border_style="green" if exit_code == 0 else "red",
                    padding=(0, 1),
                ))

    async def _execute_command(self, input_text: str) -> None:
        """Handle slash commands."""
        parts = input_text.split(" ", 1)
        cmd_name = parts[0]
        args = parts[1] if len(parts) > 1 else ""

        if cmd_name == "/exit":
            self.running = False
            return

        commands = get_commands()
        handler_entry = commands.get(cmd_name)
        if handler_entry:
            handler, _ = handler_entry
            await handler(self.ctx, args)
        else:
            console.print(f"[yellow]Unknown command: {cmd_name}[/yellow] \u2014 /help for list")

    async def run(self, initial_prompt: str = "") -> None:
        if initial_prompt.strip():
            await self._send_task(initial_prompt)
        else:
            self._show_welcome()

        identity = self.ctx.node_id.split("@")[0] if "@" in self.ctx.node_id else self.ctx.node_id

        with patch_stdout():
            while self.running:
                try:
                    user_input = await self.session.prompt_async(
                        HTML(f"<ansicyan><b>heiwa</b></ansicyan><ansigray>:</ansigray><ansiblue>{identity}</ansiblue> <ansigray>\u276f</ansigray> "),
                    )
                    text = user_input.strip()
                    if not text:
                        continue
                    if text.startswith("/"):
                        await self._execute_command(text)
                    else:
                        await self._send_task(text)
                except (EOFError, KeyboardInterrupt):
                    self.running = False

        console.print("\n[dim]Session closed.[/dim]")
