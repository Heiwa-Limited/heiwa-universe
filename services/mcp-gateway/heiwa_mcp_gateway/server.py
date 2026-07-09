"""Heiwa MCP server — tools for Grok, Claude, Codex, and Grok Build."""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
from pathlib import Path
from typing import Any

from mcp.server.fastmcp import FastMCP

from . import __version__
from .providers import chat_completion, list_providers, pick_route

mcp = FastMCP(
    "heiwa",
    instructions=(
        "Heiwa is a local-first AI operating layer and multi-provider router. "
        "Use list_providers to see available inference routes (Ollama, OpenRouter, "
        "OpenAI, xAI/Grok, Anthropic, Gemini, subscription CLIs). "
        "Use chat to send a completion through Heiwa's router. "
        "Use heiwa_status for node health. Prefer cheapest acceptable route; "
        "local Ollama before paid APIs when quality allows."
    ),
)


def _heiwa_home() -> Path:
    return Path(os.environ.get("HOME", str(Path.home()))) / "heiwa"


@mcp.tool()
def heiwa_status() -> dict[str, Any]:
    """Report Heiwa node health: OS, paths, repo presence, provider counts."""
    home = Path.home()
    repo = _heiwa_home()
    providers = list_providers()
    enabled = [p.id for p in providers if p.enabled]
    return {
        "ok": True,
        "version": __version__,
        "platform": platform.platform(),
        "home": str(home),
        "heiwa_repo": str(repo),
        "heiwa_repo_present": (repo / "HEIWA.md").is_file(),
        "providers_enabled": enabled,
        "providers_total": len(providers),
        "default_provider": os.environ.get("HEIWA_DEFAULT_PROVIDER", "auto"),
        "default_model": os.environ.get("HEIWA_DEFAULT_MODEL", ""),
        "ollama_url": os.environ.get("HEIWA_OLLAMA_URL", "http://127.0.0.1:11434"),
        "binaries": {
            "heiwa": bool(shutil.which("heiwa")),
            "claude": bool(shutil.which("claude")),
            "codex": bool(shutil.which("codex")),
            "docker": bool(shutil.which("docker")),
            "ollama": bool(shutil.which("ollama")),
        },
    }


@mcp.tool()
def list_providers_tool(include_disabled: bool = False) -> dict[str, Any]:
    """Enumerate Heiwa routing targets (API keys, Ollama, subscription CLIs)."""
    rows = []
    for p in list_providers():
        if not include_disabled and not p.enabled:
            continue
        rows.append(
            {
                "id": p.id,
                "kind": p.kind,
                "enabled": p.enabled,
                "models": p.models[:12],
                "base_url": p.base_url,
                "notes": p.notes,
            }
        )
    return {"providers": rows}


@mcp.tool()
async def chat(
    prompt: str,
    system: str = "",
    model: str = "",
    provider: str = "auto",
    temperature: float = 0.2,
    max_tokens: int = 2048,
) -> dict[str, Any]:
    """Route a chat completion through Heiwa's multi-provider router.

    model examples: openrouter/anthropic/claude-sonnet-4, ollama/qwen3.5:4b,
    openai/gpt-4.1, xai/grok-3, anthropic/claude-sonnet-4-20250514
    provider: auto | openrouter | ollama | openai | xai | anthropic | gemini | groq
    """
    messages: list[dict[str, str]] = []
    if system.strip():
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})
    try:
        result = await chat_completion(
            messages=messages,
            model=model or None,
            preference=None if provider == "auto" else provider,
            temperature=temperature,
            max_tokens=max_tokens,
        )
        return {"ok": True, **result}
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool()
def route_plan(intent: str = "general", prefer_local: bool = False) -> dict[str, Any]:
    """Plan which provider/model Heiwa would pick without sending a request."""
    try:
        pref = "ollama" if prefer_local else "auto"
        p, model = pick_route(preference=pref)
        return {
            "ok": True,
            "intent": intent,
            "provider": p.id,
            "model": model,
            "kind": p.kind,
            "rationale": (
                "local-first" if prefer_local else "auto: openrouter>ollama>cloud APIs"
            ),
        }
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@mcp.tool()
def heiwa_repo_info() -> dict[str, Any]:
    """Summarize the local heiwa-universe checkout if present."""
    repo = _heiwa_home()
    if not (repo / ".git").exists() and not (repo / "HEIWA.md").exists():
        return {"ok": False, "error": f"No Heiwa repo at {repo}"}
    info: dict[str, Any] = {"ok": True, "path": str(repo)}
    try:
        head = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "--short", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
        branch = subprocess.run(
            ["git", "-C", str(repo), "branch", "--show-current"],
            capture_output=True,
            text=True,
            check=False,
        )
        remote = subprocess.run(
            ["git", "-C", str(repo), "remote", "get-url", "origin"],
            capture_output=True,
            text=True,
            check=False,
        )
        info["head"] = head.stdout.strip()
        info["branch"] = branch.stdout.strip()
        info["remote"] = remote.stdout.strip()
    except Exception as exc:
        info["git_error"] = str(exc)
    readme = repo / "README.md"
    if readme.is_file():
        info["readme_preview"] = readme.read_text(encoding="utf-8", errors="replace")[:600]
    return info


@mcp.tool()
def heiwa_cli(args: str = "doctor") -> dict[str, Any]:
    """Run the installed `heiwa` CLI if present (e.g. doctor, providers, connect status)."""
    bin_name = shutil.which("heiwa")
    if not bin_name:
        # Fall back to cargo-run path
        repo = _heiwa_home()
        cargo = shutil.which("cargo")
        if cargo and (repo / "apps/heiwa_shell").exists():
            cmd = [
                cargo,
                "run",
                "-q",
                "-p",
                "heiwa-shell",
                "--bin",
                "heiwa",
                "--",
                *args.split(),
            ]
            cwd = str(repo)
        else:
            return {
                "ok": False,
                "error": "heiwa binary not installed. Build with: cargo build -p heiwa-shell",
            }
    else:
        cmd = [bin_name, *args.split()]
        cwd = None
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=180,
        cwd=cwd,
        check=False,
    )
    return {
        "ok": proc.returncode == 0,
        "code": proc.returncode,
        "stdout": proc.stdout[-8000:],
        "stderr": proc.stderr[-4000:],
        "cmd": cmd,
    }


def providers_json() -> str:
    return json.dumps([p.__dict__ for p in list_providers()], indent=2)
