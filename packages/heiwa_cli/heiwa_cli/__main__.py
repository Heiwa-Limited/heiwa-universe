"""Heiwa CLI entry point.

Usage:
    heiwa                           → interactive REPL
    heiwa "deploy the status page"  → one-shot
    heiwa doctor                    → built-in diagnostic
"""

from __future__ import annotations

import asyncio
import sys

from heiwa_cli.context import CLIContext
from heiwa_cli.log import configure_logging

import logging

logger = logging.getLogger("CLI")


def main() -> None:
    configure_logging()

    # Load swarm env if available (CRITICAL: must happen before CLIContext)
    try:
        from heiwa_sdk.config import load_swarm_env
        load_swarm_env()
    except Exception:
        logger.debug("Failed to load swarm env", exc_info=True)

    ctx = CLIContext()

    # Drain any locally-queued approval decisions to the hub
    try:
        synced = ctx.drain_unsynced_approvals()
        if synced and ctx.trace:
            print(f"[dim]Synced {synced} deferred approval(s) to hub.[/dim]")
    except Exception:
        logger.debug("Failed to drain unsynced approvals", exc_info=True)

    args = sys.argv[1:]

    # No args → interactive REPL
    if not args:
        from heiwa_cli.repl import HeiwaREPL
        asyncio.run(HeiwaREPL(ctx).run())
        return

    first = args[0]

    # Handle --help / -h locally
    if first in {"--help", "-h"}:
        print(
            "Usage: heiwa [command|prompt]\n\n"
            "Commands:\n"
            "  heiwa                         Interactive REPL\n"
            "  heiwa \"prompt\"                One-shot: classify, route, execute\n"
            "  heiwa doctor [--fix]          Diagnose (and optionally repair) setup\n"
            "  heiwa status                  Hub health, Ollama, rate groups\n"
            "  heiwa providers               Provider installation and auth status\n"
            "  heiwa models                  Available models + rate group capacity\n"
            "  heiwa cells                   Identity profiles with cell hierarchies\n"
            "  heiwa route \"prompt\"          Dry-run: show how a prompt would route\n"
            "  heiwa cost                    Rate usage this session + today\n"
            "  heiwa vault [set|get|list]    Manage secrets in ~/.heiwa/vault.env\n"
            "  heiwa feedback good|bad       Record routing quality\n"
            "  heiwa memory                  View learned routing preferences\n"
            "  heiwa logs [N]                Tail ~/.heiwa/logs/heiwa.log\n"
            "  heiwa bench [suite]           Run release gate suites\n"
            "  heiwa auth <provider>         Manage provider authentication\n\n"
            f"Mode: {ctx.mode} | Node: {ctx.node_id}"
        )
        return

    if first == "panel":
        from heiwa_ui.dashboard import HeiwaDashboardApp
        app = HeiwaDashboardApp()
        app.run()
        return

    if first == "auth":
        from heiwa_cli.auth import handle_auth_command

        exit_code = asyncio.run(handle_auth_command(ctx, args[1:]))
        sys.exit(exit_code)

    if first.startswith("/") or first in {"doctor", "status", "cells", "bench", "models", "cost", "help", "vault", "logs", "feedback", "memory", "providers"}:
        # Map bare command names to slash commands
        cmd = first if first.startswith("/") else f"/{first}"
        rest = " ".join(args[1:])
        from heiwa_cli.commands import get_commands
        commands = get_commands()
        handler_entry = commands.get(cmd)
        if handler_entry:
            handler, _ = handler_entry
            asyncio.run(handler(ctx, rest))
            return
        # Unknown command — fall through to one-shot
        print(f"Unknown command: {cmd}")
        sys.exit(1)

    # "approve" / "reject" as bare subcommands
    if first in {"approve", "reject"}:
        cmd = f"/{first}"
        rest = " ".join(args[1:])
        from heiwa_cli.commands import get_commands
        handler, _ = get_commands()[cmd]
        asyncio.run(handler(ctx, rest))
        return

    # Everything else → one-shot dispatch
    prompt = " ".join(args)
    from heiwa_cli.oneshot import dispatch_once
    exit_code = asyncio.run(dispatch_once(ctx, prompt))
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
