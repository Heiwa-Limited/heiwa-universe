"""Slash command registry for the Heiwa CLI REPL."""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable

from rich.console import Console
from rich.markdown import Markdown
from rich.table import Table

from heiwa_sdk.spacetimedb import SpacetimeDB

if TYPE_CHECKING:
    from heiwa_cli.context import CLIContext

console = Console(stderr=True)
_COMMANDS: dict[str, tuple[Callable, str]] = {}


def command(name: str, help_text: str):
    def decorator(fn):
        _COMMANDS[name] = (fn, help_text)
        return fn

    return decorator


def get_commands() -> dict[str, tuple[Callable, str]]:
    return dict(_COMMANDS)


def command_names() -> list[str]:
    return sorted(_COMMANDS.keys())


@command("/help", "Print all commands")
async def cmd_help(ctx: CLIContext, args: str = "") -> None:
    lines = []
    for name in sorted(_COMMANDS):
        _, help_text = _COMMANDS[name]
        lines.append(f"  `{name}` \u2014 {help_text}")
    lines.append(f"  `/exit` \u2014 Close the session")
    lines.append("\nAnything else is sent as a task to the optimal model.")
    console.print(Markdown("\n".join(lines)))


@command("/status", "Hub health, Ollama status, rate group capacity")
async def cmd_status(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.rate_ledger import get_rate_ledger

    table = Table(title="Heiwa Status")
    table.add_column("Property")
    table.add_column("Value")
    table.add_row("Root", str(ctx.root))
    table.add_row("Node", ctx.node_id)
    table.add_row("Hub Auth", "set" if ctx.auth_token else "[red]not set[/red]")
    table.add_row("Session", ctx.session_id)

    hub_health = None
    for url in ctx.hub_url_candidates():
        try:
            import requests

            resp = requests.get(f"{url}/health", timeout=5)
            if resp.status_code == 200:
                hub_health = resp.json()
                table.add_row("Hub", f"[green]{hub_health.get('status', 'ok')}[/green] ({url})")
                break
        except Exception:
            continue
    if not hub_health:
        table.add_row("Hub", "[red]unreachable[/red]")

    ollama_url = os.getenv("HEIWA_OLLAMA_URL", "http://127.0.0.1:11434")
    try:
        import urllib.request

        with urllib.request.urlopen(f"{ollama_url}/api/tags", timeout=3) as resp:
            tags = json.loads(resp.read().decode())
            models = [m.get("name", "?") for m in tags.get("models", [])]
            table.add_row("Ollama", f"[green]{len(models)} model(s)[/green]: {', '.join(models[:4])}")
    except Exception:
        table.add_row("Ollama", "[yellow]offline[/yellow]")

    try:
        ledger = get_rate_ledger()
        rate_status = ledger.status()
        premium_available = sum(1 for name, data in rate_status.items() if not data.get("unlimited") and data.get("available"))
        premium_total = sum(1 for data in rate_status.values() if not data.get("unlimited"))
        table.add_row("Rate Groups", f"{premium_available}/{premium_total} premium groups available")
    except Exception:
        pass

    console.print(table)


@command("/models", "Available models + rate group remaining turns")
async def cmd_models(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.rate_ledger import get_rate_ledger

    registry = ctx.ai_router.get("models", {}).get("registry", {})
    providers = ctx.ai_router.get("providers", {})
    ledger = get_rate_ledger()
    status = ledger.status()

    table = Table(title="Model Registry")
    table.add_column("Worker")
    table.add_column("Model")
    table.add_column("Provider")
    table.add_column("Rate Group")
    table.add_column("Remaining")
    table.add_column("Reserve")

    for key, entry in registry.items():
        provider = str(entry.get("provider", ""))
        provider_cfg = providers.get(provider, {})
        rate_group = str(provider_cfg.get("rate_group", provider))
        group_status = status.get(rate_group, {})
        remaining = "∞" if group_status.get("unlimited") else str(group_status.get("remaining", 0))
        reserve = "-" if group_status.get("unlimited") else str(group_status.get("reserve", 0))
        table.add_row(key, str(entry.get("id", "")), provider, rate_group, remaining, reserve)

    console.print(table)


@command("/cells", "Identity profiles with cell hierarchies")
async def cmd_cells(ctx: CLIContext, args: str = "") -> None:
    identities = ctx.identity_selector.all_identities()
    if not identities:
        console.print("[yellow]No identities loaded from profiles.json[/yellow]")
        return

    for identity in identities:
        cells_str = " -> ".join(f"{cell.role} ({cell.model.split('/')[-1]})" for cell in identity.cells)
        console.print(f"[bold cyan]{identity.id}[/bold cyan]: {identity.description}")
        console.print(f"  cells: {cells_str}")
        console.print(f"  tool: {identity.tool} | runtime: {identity.runtime}")
        console.print()


@command("/cost", "Rate usage this session + today across all groups")
async def cmd_cost(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.rate_ledger import get_rate_ledger

    summary = get_rate_ledger().summary()
    if not summary:
        console.print("[dim]No rate group usage recorded yet.[/dim]")
        return

    table = Table(title="Rate Group Usage")
    table.add_column("Group")
    table.add_column("Used", justify="right")
    table.add_column("Remaining", justify="right")
    table.add_column("Reserve", justify="right")
    table.add_column("Surplus", justify="right")
    table.add_column("Status")

    for group, data in sorted(summary.items()):
        if data.get("unlimited"):
            table.add_row(group, "-", "∞", "-", "∞", "[green]local[/green]")
            continue
        status = "[green]interactive[/green]" if data.get("available") else "[yellow]cooldown[/yellow]"
        if data.get("background_allowed"):
            status += " / [cyan]background[/cyan]"
        table.add_row(
            group,
            str(data.get("used", 0)),
            str(data.get("remaining", 0)),
            str(data.get("reserve", 0)),
            str(data.get("surplus_headroom", 0)),
            status,
        )
    console.print(table)


@command("/doctor", "Diagnose hub, Ollama, STDB, Railway, provider auth (--fix to auto-repair)")
async def cmd_doctor(ctx: CLIContext, args: str = "") -> None:
    from heiwa_cli.auth import ProviderAuthManager
    from pathlib import Path
    import subprocess
    import shutil

    fix_mode = "--fix" in str(args or "")
    checks: list[tuple[str, str, str]] = []
    fixes_applied: list[str] = []
    manager = ProviderAuthManager(ctx)

    # --- Hub ---
    for url in ctx.hub_url_candidates()[:2]:
        try:
            import urllib.request
            with urllib.request.urlopen(f"{url}/health", timeout=5):
                checks.append(("Hub", url, "[green]OK[/green]"))
                break
        except Exception as exc:
            checks.append(("Hub", url, f"[red]{exc}[/red]"))

    # --- Ollama ---
    ollama_url = os.getenv("HEIWA_OLLAMA_URL", "http://127.0.0.1:11434")
    ollama_ok = False
    try:
        import urllib.request
        with urllib.request.urlopen(f"{ollama_url}/api/tags", timeout=3) as resp:
            import json as _json
            tags = _json.loads(resp.read().decode())
            models = [m.get("name", "?") for m in tags.get("models", [])]
            ollama_ok = True
            checks.append(("Ollama", ollama_url, f"[green]{len(models)} model(s)[/green]"))
    except Exception:
        checks.append(("Ollama", ollama_url, "[yellow]offline[/yellow]"))
        if fix_mode and shutil.which("ollama"):
            console.print("[yellow]Starting Ollama...[/yellow]")
            try:
                subprocess.Popen(
                    ["ollama", "serve"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
                import asyncio
                await asyncio.sleep(2)
                try:
                    with urllib.request.urlopen(f"{ollama_url}/api/tags", timeout=3):
                        ollama_ok = True
                        checks[-1] = ("Ollama", ollama_url, "[green]started[/green]")
                        fixes_applied.append("Started Ollama")
                except Exception:
                    checks[-1] = ("Ollama", ollama_url, "[yellow]starting (may need a moment)[/yellow]")
                    fixes_applied.append("Attempted to start Ollama")
            except Exception as e:
                console.print(f"[red]Failed to start Ollama: {e}[/red]")

    # --- Models ---
    default_model = os.getenv("HEIWA_OLLAMA_MODEL", "qwen3.5:4b")
    if ollama_ok:
        try:
            import urllib.request
            import json as _json
            with urllib.request.urlopen(f"{ollama_url}/api/tags", timeout=3) as resp:
                tags = _json.loads(resp.read().decode())
                model_names = [m.get("name", "") for m in tags.get("models", [])]
                has_default = any(default_model in m for m in model_names)
                if has_default:
                    checks.append(("Default Model", default_model, "[green]available[/green]"))
                else:
                    checks.append(("Default Model", default_model, "[yellow]not pulled[/yellow]"))
                    if fix_mode:
                        console.print(f"[yellow]Pulling {default_model}...[/yellow]")
                        try:
                            result = subprocess.run(
                                ["ollama", "pull", default_model],
                                capture_output=True, text=True, timeout=300,
                            )
                            if result.returncode == 0:
                                checks[-1] = ("Default Model", default_model, "[green]pulled[/green]")
                                fixes_applied.append(f"Pulled {default_model}")
                            else:
                                console.print(f"[red]Pull failed: {result.stderr[:200]}[/red]")
                        except Exception as e:
                            console.print(f"[red]Pull failed: {e}[/red]")
        except Exception:
            pass

    # --- .env ---
    env_path = ctx.root / ".env"
    example_path = ctx.root / ".env.example"
    if env_path.exists():
        checks.append((".env", str(env_path), "[green]OK[/green]"))
    else:
        checks.append((".env", str(env_path), "[yellow]missing[/yellow]"))
        if fix_mode and example_path.exists():
            shutil.copy2(example_path, env_path)
            checks[-1] = (".env", str(env_path), "[green]copied from .env.example[/green]")
            fixes_applied.append("Copied .env from .env.example")

    # --- Vault ---
    vault_path = Path.home() / ".heiwa" / "vault.env"
    if vault_path.exists():
        checks.append(("Vault", str(vault_path), "[green]OK[/green]"))
    else:
        checks.append(("Vault", str(vault_path), "[yellow]missing[/yellow]"))
        if fix_mode:
            vault_path.parent.mkdir(parents=True, exist_ok=True)
            vault_path.write_text("# Heiwa vault — managed by `heiwa vault set`\n", encoding="utf-8")
            checks[-1] = ("Vault", str(vault_path), "[green]created[/green]")
            fixes_applied.append("Created empty vault")

    # --- Venv ---
    venv_path = ctx.root / ".venv"
    if venv_path.exists() and (venv_path / "bin" / "python3").exists():
        checks.append(("Venv", str(venv_path), "[green]OK[/green]"))
    else:
        checks.append(("Venv", str(venv_path), "[yellow]missing[/yellow]"))
        if fix_mode:
            console.print("[yellow]Creating venv...[/yellow]")
            try:
                subprocess.run(
                    [sys.executable, "-m", "venv", str(venv_path)],
                    check=True, capture_output=True, text=True,
                )
                pip = str(venv_path / "bin" / "pip")
                req = str(ctx.root / "requirements.txt")
                if Path(req).exists():
                    console.print("[yellow]Installing requirements...[/yellow]")
                    subprocess.run(
                        [pip, "install", "-q", "-r", req],
                        capture_output=True, text=True, timeout=300,
                    )
                checks[-1] = ("Venv", str(venv_path), "[green]created[/green]")
                fixes_applied.append("Created venv and installed requirements")
            except Exception as e:
                console.print(f"[red]Venv creation failed: {e}[/red]")

    # --- Logs dir ---
    logs_dir = Path.home() / ".heiwa" / "logs"
    if logs_dir.exists():
        checks.append(("Logs", str(logs_dir), "[green]OK[/green]"))
    else:
        if fix_mode:
            logs_dir.mkdir(parents=True, exist_ok=True)
            checks.append(("Logs", str(logs_dir), "[green]created[/green]"))
            fixes_applied.append("Created logs directory")
        else:
            checks.append(("Logs", str(logs_dir), "[yellow]missing[/yellow]"))

    # --- Standard checks ---
    checks.append(("Auth Token", "HEIWA_AUTH_TOKEN", "[green]set[/green]" if ctx.auth_token else "[red]not set[/red]"))
    checks.append(("SOUL", str(ctx.context_paths["SOUL.md"]), "[green]OK[/green]" if ctx.context_paths["SOUL.md"].exists() else "[red]missing[/red]"))
    checks.append(("Profiles", str(ctx.context_paths["profiles.json"]), "[green]OK[/green]" if ctx.context_paths["profiles.json"].exists() else "[red]missing[/red]"))
    checks.append(("AI Router", str(ctx.context_paths["ai_router.json"]), "[green]OK[/green]" if ctx.context_paths["ai_router.json"].exists() else "[red]missing[/red]"))

    for provider in ("gemini", "codex", "claude", "antigravity"):
        try:
            record = manager.status(provider)
            status = str(record.get("status") or "unknown")
            styled = "[green]connected[/green]" if status == "connected" else "[yellow]pending[/yellow]" if status in {"disconnected", "unknown"} else "[red]error[/red]"
            checks.append((f"Auth:{provider}", str(record.get("default_model") or provider), styled))
        except Exception as exc:
            checks.append((f"Auth:{provider}", provider, f"[red]{exc}[/red]"))

    table = Table(title="Heiwa Doctor" + (" (--fix)" if fix_mode else ""))
    table.add_column("Check")
    table.add_column("Target")
    table.add_column("Status")
    for name, target, status in checks:
        table.add_row(name, target, status)
    console.print(table)

    if fixes_applied:
        console.print(f"\n[green]Applied {len(fixes_applied)} fix(es):[/green]")
        for fix in fixes_applied:
            console.print(f"  [green]+[/green] {fix}")
    elif fix_mode:
        console.print("\n[dim]Nothing to fix — all checks passed.[/dim]")
    else:
        # Count issues
        issue_count = sum(1 for _, _, s in checks if "red" in s or "yellow" in s)
        if issue_count:
            console.print(f"\n[dim]{issue_count} issue(s) found. Run [bold]heiwa doctor --fix[/bold] to auto-repair.[/dim]")


@command("/bench", "Run release gate suites")
async def cmd_bench(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.bench import HeiwaBench

    suite = str(args or "").strip() or None
    result = HeiwaBench(ctx.root).run(suite=suite)
    header = "[green]PASS[/green]" if result.get("ok") else "[red]FAIL[/red]"
    console.print(f"{header} {result.get('suite')}")
    for suite_result in result.get("results", []):
        status = "[green]PASS[/green]" if suite_result.get("ok") else "[red]FAIL[/red]"
        console.print(f"  {suite_result.get('suite')}: {status}")
    for failure in result.get("failures", [])[:10]:
        console.print(
            f"  [red]{failure.get('suite')}::{failure.get('case')}[/red] "
            f"{failure.get('field')} expected={failure.get('expected')} actual={failure.get('actual')}"
        )


@command("/approve", "Approve pending task")
async def cmd_approve(ctx: CLIContext, args: str = "") -> None:
    task_id = args.strip()
    if not task_id:
        from heiwa_cognition.approval import get_approval_registry

        pending = get_approval_registry().list_states("PENDING")
        if not pending:
            console.print("[dim]No pending approvals.[/dim]")
            return
        for state in pending:
            expires = max(0, int(state.expires_at - time.time()))
            console.print(f"  [yellow]{state.task_id}[/yellow] — expires in {expires}s")
        return

    body = json.dumps({"actor": ctx.operator_name, "reason": "slash_command"}).encode()
    for url in ctx.hub_url_candidates():
        try:
            import urllib.request

            req = urllib.request.Request(
                f"{url}/tasks/{task_id}/approve",
                data=body,
                headers={"Content-Type": "application/json", "Authorization": f"Bearer {ctx.auth_token}"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=10):
                console.print(f"[green]Approved: {task_id}[/green]")
                return
        except Exception:
            continue
    console.print(f"[red]Failed to approve {task_id} — hub unreachable.[/red]")


@command("/reject", "Reject pending task")
async def cmd_reject(ctx: CLIContext, args: str = "") -> None:
    task_id = args.strip()
    if not task_id:
        console.print("[yellow]Usage: /reject <task_id>[/yellow]")
        return

    body = json.dumps({"actor": ctx.operator_name, "reason": "slash_command"}).encode()
    for url in ctx.hub_url_candidates():
        try:
            import urllib.request

            req = urllib.request.Request(
                f"{url}/tasks/{task_id}/reject",
                data=body,
                headers={"Content-Type": "application/json", "Authorization": f"Bearer {ctx.auth_token}"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=10):
                console.print(f"[yellow]Rejected: {task_id}[/yellow]")
                return
        except Exception:
            continue
    console.print(f"[red]Failed to reject {task_id} — hub unreachable.[/red]")


@command("/trace", "Toggle route/dispatch visibility")
async def cmd_trace(ctx: CLIContext, args: str = "") -> None:
    ctx.trace = not ctx.trace
    console.print(f"[dim]Trace: {'on' if ctx.trace else 'off'}[/dim]")


@command("/logs", "Tail ~/.heiwa/logs/heiwa.log")
async def cmd_logs(ctx: CLIContext, args: str = "") -> None:
    from pathlib import Path

    log_file = Path.home() / ".heiwa" / "logs" / "heiwa.log"
    if not log_file.exists():
        console.print("[dim]No logs yet. Logs are written to ~/.heiwa/logs/heiwa.log[/dim]")
        return
    lines = int(args.strip()) if args.strip().isdigit() else 30
    try:
        content = log_file.read_text(encoding="utf-8")
        tail = "\n".join(content.splitlines()[-lines:])
        console.print(f"[dim]~/.heiwa/logs/heiwa.log (last {lines} lines):[/dim]")
        console.print(tail)
    except Exception as e:
        console.print(f"[red]Failed to read log: {e}[/red]")


@command("/providers", "Show provider installation, auth, and execution status")
async def cmd_providers(ctx: CLIContext, args: str = "") -> None:
    import shutil
    from pathlib import Path

    table = Table(title="Providers")
    table.add_column("Provider")
    table.add_column("Installed")
    table.add_column("Auth")
    table.add_column("Rate Group")
    table.add_column("Auto")

    providers_config = ctx.ai_router.get("providers", {})
    snm = ctx.ai_router.get("routing_policy", {}).get("single_node_mac", {})
    auto_enabled = set(snm.get("auto_enabled_providers", []))

    for name, cfg in sorted(providers_config.items()):
        cli_cmd = cfg.get("cli_command", "")
        rate_group = cfg.get("rate_group", name)
        auth_kind = cfg.get("auth_kind", "unknown")

        # Check installed
        if name in {"ollama", "local", "vllm", "litellm"}:
            installed = "[green]yes[/green]" if shutil.which("ollama") else "[yellow]no[/yellow]"
        elif cli_cmd:
            installed = "[green]yes[/green]" if shutil.which(cli_cmd) else "[yellow]no[/yellow]"
        else:
            installed = "[dim]n/a[/dim]"

        # Check auth
        if auth_kind == "local_runtime":
            auth = "[green]local[/green]"
        elif auth_kind == "oauth_cli":
            auth = "[green]oauth[/green]"
        elif auth_kind == "oauth_openclaw":
            auth = "[green]oauth[/green]"
        elif auth_kind == "api_via_openclaw":
            auth = "[yellow]api_key[/yellow]"
        else:
            auth = "[dim]unknown[/dim]"

        auto = "[green]yes[/green]" if name in auto_enabled else "[dim]manual[/dim]"

        table.add_row(name, installed, auth, rate_group, auto)

    console.print(table)
    console.print(f"\n[dim]Mode: {ctx.mode} | auto-enabled providers route without explicit provider selection[/dim]")


@command("/compact", "Compress conversation, write SessionSummary")
async def cmd_compact(ctx: CLIContext, args: str = "") -> None:
    summary = ctx.build_session_summary(summary_text=args or None)
    ctx.save_session_summary(summary)
    preview = str(summary.get("summary_text") or "").strip()
    console.print("[dim]Conversation context compacted.[/dim]")
    if preview:
        console.print(Markdown(f"```text\n{preview[:800]}\n```"))


@command("/route", "Dry-run: show how a prompt would be classified and routed")
async def cmd_route(ctx: CLIContext, args: str = "") -> None:
    text = args.strip()
    if not text:
        console.print("[yellow]Usage: /route <prompt>[/yellow]")
        return

    from heiwa_protocol.routing import BrokerRouteRequest

    request = BrokerRouteRequest(
        request_id="dry-run",
        task_id="dry-run",
        raw_text=text,
        sender_id=ctx.node_id,
        source_surface="cli",
    )
    route = ctx.enrichment.enrich(request)
    dispatch = ctx.gateway.resolve(route)
    identity = ctx.identity_selector.select(route.intent_class, text)

    table = Table(title="Route Preview", show_header=False)
    table.add_column("", style="dim")
    table.add_column("")
    table.add_row("Intent", route.intent_class)
    table.add_row("Risk", route.risk_level)
    table.add_row("Identity", identity.id)
    cells = " \u2192 ".join(f"{c.role} ({c.model.split('/')[-1]})" for c in identity.cells)
    table.add_row("Cells", cells)
    table.add_row("Compute Class", str(route.compute_class))
    table.add_row("Worker", route.assigned_worker)
    table.add_row("Model", route.target_model)
    table.add_row("Provider", dispatch.provider)
    table.add_row("Adapter", dispatch.adapter_tool)
    table.add_row("Transport", dispatch.transport)
    table.add_row("Rate Group", dispatch.rate_group)
    table.add_row("Runtime", route.target_runtime)
    table.add_row("Tier", route.target_tier)
    table.add_row("Approval", "[yellow]required[/yellow]" if route.requires_approval else "[green]auto[/green]")
    table.add_row("Rationale", route.rationale)
    console.print(table)


@command("/clear", "Reset conversation context")
async def cmd_clear(ctx: CLIContext, args: str = "") -> None:
    ctx.turn_history.clear()
    console.clear()


@command("/vault", "Manage secrets in ~/.heiwa/vault.env  (set|get|delete|list|export|sync)")
async def cmd_vault(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.vault import VaultStore

    parts = args.strip().split(None, 2)
    sub = parts[0].lower() if parts else ""
    store = VaultStore()

    if sub == "set":
        if len(parts) < 3:
            console.print("[yellow]Usage: /vault set KEY VALUE[/yellow]")
            return
        key, value = parts[1], parts[2]
        store.set(key, value)
        console.print(f"[green]vault:[/green] {key} set in ~/.heiwa/vault.env")

    elif sub == "get":
        if len(parts) < 2:
            console.print("[yellow]Usage: /vault get KEY[/yellow]")
            return
        val = store.get(parts[1])
        if val is None:
            console.print(f"[red]vault:[/red] {parts[1]} not found")
        else:
            console.print(val)

    elif sub == "delete":
        if len(parts) < 2:
            console.print("[yellow]Usage: /vault delete KEY[/yellow]")
            return
        removed = store.delete(parts[1])
        if removed:
            console.print(f"[yellow]vault:[/yellow] {parts[1]} deleted")
        else:
            console.print(f"[red]vault:[/red] {parts[1]} not found")

    elif sub == "list":
        keys = store.list_keys()
        if not keys:
            console.print("[dim]Vault is empty.[/dim]")
            return
        table = Table(title="~/.heiwa/vault.env", show_header=False)
        table.add_column("Key", style="cyan")
        for k in keys:
            table.add_row(k)
        console.print(table)

    elif sub == "export":
        content = store.export_env()
        if not content:
            console.print("[dim]Vault is empty.[/dim]")
        else:
            console.print(content)

    elif sub == "sync":
        # /vault sync --to railway [--service <name>]
        service = None
        if "--service" in args:
            idx = parts.index("--service") if "--service" in parts else -1
            if idx >= 0 and idx + 1 < len(parts):
                service = parts[idx + 1]
        code, msg = store.sync_to_railway(service=service)
        style = "green" if code == 0 else "red"
        console.print(f"[{style}]vault sync:[/{style}] {msg}")

    else:
        console.print(
            "[bold]heiwa vault[/bold] — sovereign secret management\n\n"
            "  [cyan]/vault set KEY VALUE[/cyan]    — store a secret\n"
            "  [cyan]/vault get KEY[/cyan]           — retrieve a secret\n"
            "  [cyan]/vault delete KEY[/cyan]        — remove a secret\n"
            "  [cyan]/vault list[/cyan]              — show all key names\n"
            "  [cyan]/vault export[/cyan]            — print KEY=VALUE (pipe to scp/node-b)\n"
            "  [cyan]/vault sync[/cyan]              — push to Railway via railway CLI\n"
        )


@command("/feedback", "Record routing quality: /feedback good|bad [note]")
async def cmd_feedback(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.memory import record_feedback

    parts = args.strip().split(None, 1)
    quality = parts[0].lower() if parts else ""
    note = parts[1] if len(parts) > 1 else ""

    if quality not in {"good", "bad"}:
        console.print(
            "[bold]heiwa feedback[/bold] — record routing quality\n\n"
            "  [cyan]/feedback good[/cyan]            — last response was helpful\n"
            "  [cyan]/feedback bad[/cyan]             — last response was poor\n"
            "  [cyan]/feedback bad wrong model[/cyan] — add a note\n"
        )
        return

    # Pull context from the last assistant turn
    last_turn = None
    for turn in reversed(ctx.turn_history):
        if turn.get("role") == "assistant":
            last_turn = turn
            break

    last_user = None
    for turn in reversed(ctx.turn_history):
        if turn.get("role") == "user":
            last_user = turn
            break

    entry = record_feedback(
        quality=quality,
        intent_class=last_turn.get("intent_class") if last_turn else None,
        model=last_turn.get("model") if last_turn else None,
        provider=last_turn.get("provider") if last_turn else None,
        prompt_snippet=last_user.get("content") if last_user else None,
        note=note,
    )
    style = "green" if quality == "good" else "yellow"
    console.print(f"[{style}]Feedback recorded: {quality}[/{style}]")
    if note:
        console.print(f"[dim]Note: {note}[/dim]")


@command("/memory", "View routing preferences learned from feedback")
async def cmd_memory(ctx: CLIContext, args: str = "") -> None:
    from heiwa_sdk.memory import summary as mem_summary

    data = mem_summary()
    if not data.get("total_feedback"):
        console.print("[dim]No feedback recorded yet. Use /feedback good|bad after a response.[/dim]")
        return

    table = Table(title="Routing Memory")
    table.add_column("Intent")
    table.add_column("Model")
    table.add_column("Score")

    prefs = data.get("intent_preferences", {})
    if prefs:
        for intent, models in sorted(prefs.items()):
            for model, score_str in sorted(models.items()):
                table.add_row(intent, model, score_str)
    console.print(table)
    console.print(
        f"\n[dim]Total feedback: {data['total_feedback']} "
        f"({data['good']} good, {data['bad']} bad)[/dim]"
    )


@command("/inspect", "Inspect an STDB table (model_tiers, rate_group_state, task_dispatches)")
async def cmd_inspect(ctx: CLIContext, args: str = "") -> None:
    table_name = args.strip() or "model_tiers"
    stdb = getattr(ctx, "stdb", None)
    if not stdb:
        # Try to find stdb in ctx.db if it exists
        db = getattr(ctx, "db", None)
        stdb = getattr(db, "stdb", None) if db else None

    if not stdb:
        console.print("  [red]STDB not connected.[/red] Run with HEIWA_STATE_BACKEND=spacetimedb.")
        return

    inspectors = {
        "model_tiers": lambda: stdb.get_model_tiers(enabled_only=False),
        "rate_group_state": lambda: stdb.query("SELECT * FROM rate_group_state"),
        "task_dispatches": lambda: stdb.query(
            "SELECT task_id, intent_class, status, assigned_model, effort_knob FROM task_dispatches ORDER BY created_at DESC LIMIT 20"
        ),
    }

    if table_name not in inspectors:
        console.print(f"  [yellow]Unknown table:[/yellow] {table_name}")
        console.print(f"  Available: {', '.join(inspectors.keys())}")
        return

    rows = inspectors[table_name]()
    if not rows:
        console.print(f"  {table_name}: (empty)")
        return

    # Print as formatted table using rich
    table = Table(title=f"STDB Inspect: {table_name}")
    keys = list(rows[0].keys())
    # Limit to first 6 columns for readability in CLI
    display_keys = keys[:6]
    for key in display_keys:
        table.add_column(key)

    for row in rows:
        vals = [str(row.get(k, "")) for k in display_keys]
        table.add_row(*vals)

    console.print(table)


@command("/start", "Start Heiwa hub locally (STDB + agents + HTTP server)")
async def cmd_start(ctx: CLIContext, args: str = "") -> None:
    repo_root = Path(ctx.root)

    # Step 1: Start local STDB if not running
    spacetime = os.path.expanduser("~/.local/bin/spacetime")
    if os.path.exists(spacetime):
        # Check if already running on 3000
        import socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        result = sock.connect_ex(('127.0.0.1', 3000))
        if result != 0:
            console.print("  [cyan]Starting local SpacetimeDB...[/cyan]")
            subprocess.Popen(
                [spacetime, "start", "--listen-addr", "127.0.0.1:3000"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            await asyncio.sleep(3)
        else:
            console.print("  [dim]SpacetimeDB already running on port 3000.[/dim]")
        sock.close()

    # Step 2: Set env for STDB backend
    os.environ["HEIWA_STATE_BACKEND"] = "spacetimedb"
    os.environ["STDB_SERVER"] = "local"
    os.environ["STDB_IDENTITY"] = "heiwaproductiondb"
    os.environ.setdefault("OLLAMA_BASE_URL", "http://localhost:11434")

    # Step 3: Start hub
    console.print("  [cyan]Starting Heiwa Hub on port 8080...[/cyan]")
    
    # Construct PYTHONPATH
    pkg_paths = [str(repo_root / "packages" / p) for p in
                 ["heiwa_cli", "heiwa_cognition", "heiwa_sdk", "heiwa_protocol", "heiwa_identity", "heiwa_ui"]]
    pkg_paths.append(str(repo_root / "apps"))
    
    env = os.environ.copy()
    env["PYTHONPATH"] = ":".join(pkg_paths)

    hub_proc = subprocess.Popen(
        [
            sys.executable, "-m", "apps.heiwa_hub.main",
        ],
        cwd=str(repo_root),
        env=env,
    )
    console.print(f"  [green]Hub started (PID {hub_proc.pid}).[/green]")
    console.print("  Health: [link=http://localhost:8080/health]http://localhost:8080/health[/link]")
