"""Multi-provider inference routing for Heiwa MCP gateway.

Priority:
1. Explicit model prefix (openrouter/, ollama/, anthropic/, openai/, xai/, groq/)
2. Env-configured default provider
3. First available healthy route among: ollama, openrouter, openai, xai, anthropic, groq

Subscription CLIs (claude, codex) are reported as providers when binaries exist;
chat may shell out to them when selected.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import httpx

# Load .env from common locations (never commit secrets)
for candidate in (
    Path.home() / "heiwa" / ".env",
    Path.home() / ".heiwa" / ".env",
    Path("/mnt/c/Users/devon/heiwa/.env") if Path("/mnt/c/Users/devon").exists() else None,
    Path.cwd() / ".env",
):
    if candidate and candidate.is_file():
        try:
            from dotenv import load_dotenv

            load_dotenv(candidate, override=False)
        except Exception:
            pass


@dataclass
class Provider:
    id: str
    kind: str  # api | local | cli
    enabled: bool
    models: list[str]
    base_url: str | None = None
    notes: str = ""


def _env(name: str) -> str | None:
    v = os.environ.get(name, "").strip()
    return v or None


def list_providers() -> list[Provider]:
    out: list[Provider] = []

    if _env("OPENROUTER_API_KEY"):
        out.append(
            Provider(
                id="openrouter",
                kind="api",
                enabled=True,
                models=[
                    "openrouter/auto",
                    "anthropic/claude-sonnet-4",
                    "openai/gpt-4.1",
                    "google/gemini-2.5-pro",
                    "x-ai/grok-3",
                ],
                base_url="https://openrouter.ai/api/v1",
                notes="BYOK multi-model router (recommended for one-key multi-provider)",
            )
        )

    if _env("OPENAI_API_KEY"):
        out.append(
            Provider(
                id="openai",
                kind="api",
                enabled=True,
                models=["gpt-4.1", "gpt-4.1-mini", "o4-mini"],
                base_url=os.environ.get("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            )
        )

    if _env("XAI_API_KEY") or _env("GROK_API_KEY"):
        out.append(
            Provider(
                id="xai",
                kind="api",
                enabled=True,
                models=["grok-4", "grok-3", "grok-3-mini"],
                base_url="https://api.x.ai/v1",
                notes="xAI / Grok API",
            )
        )

    if _env("ANTHROPIC_API_KEY"):
        out.append(
            Provider(
                id="anthropic",
                kind="api",
                enabled=True,
                models=["claude-sonnet-4-20250514", "claude-opus-4-20250514"],
                base_url="https://api.anthropic.com/v1",
            )
        )

    if _env("GEMINI_API_KEY") or _env("GOOGLE_API_KEY"):
        out.append(
            Provider(
                id="gemini",
                kind="api",
                enabled=True,
                models=["gemini-2.5-pro", "gemini-2.5-flash"],
                base_url="https://generativelanguage.googleapis.com/v1beta/openai",
                notes="Gemini OpenAI-compatible endpoint",
            )
        )

    if _env("GROQ_API_KEY"):
        out.append(
            Provider(
                id="groq",
                kind="api",
                enabled=True,
                models=["llama-3.3-70b-versatile", "qwen/qwen3-32b"],
                base_url="https://api.groq.com/openai/v1",
            )
        )

    if _env("CEREBRAS_API_KEY"):
        out.append(
            Provider(
                id="cerebras",
                kind="api",
                enabled=True,
                models=["llama3.1-70b", "qwen-3-32b"],
                base_url="https://api.cerebras.ai/v1",
            )
        )

    ollama_url = os.environ.get("HEIWA_OLLAMA_URL", "http://127.0.0.1:11434").rstrip("/")
    ollama_ok = False
    models: list[str] = []
    try:
        r = httpx.get(f"{ollama_url}/api/tags", timeout=1.5)
        if r.status_code == 200:
            ollama_ok = True
            models = [m.get("name", "") for m in r.json().get("models", []) if m.get("name")]
    except Exception:
        pass
    out.append(
        Provider(
            id="ollama",
            kind="local",
            enabled=ollama_ok,
            models=models or ["qwen3.5:4b", "llama3.2:3b"],
            base_url=f"{ollama_url}/v1",
            notes="Local Ollama OpenAI-compatible API",
        )
    )

    # Subscription CLIs (OAuth) — detect presence only
    if shutil.which("claude"):
        out.append(
            Provider(
                id="claude-cli",
                kind="cli",
                enabled=True,
                models=["claude-cli/default"],
                notes="Anthropic Claude Code CLI (uses your Claude subscription login)",
            )
        )
    if shutil.which("codex"):
        out.append(
            Provider(
                id="codex-cli",
                kind="cli",
                enabled=True,
                models=["codex-cli/default"],
                notes="OpenAI Codex CLI (uses your ChatGPT/Codex login)",
            )
        )

    # Detect Windows-side Claude/Codex auth for status reporting
    claude_json = Path("/mnt/c/Users/devon/.claude.json")
    if claude_json.is_file():
        try:
            data = json.loads(claude_json.read_text(encoding="utf-8-sig", errors="replace"))
            if isinstance(data, dict) and data.get("oauthAccount"):
                out.append(
                    Provider(
                        id="claude-subscription",
                        kind="cli",
                        enabled=True,
                        models=["claude-subscription"],
                        notes="Claude desktop/CLI OAuth present on Windows host",
                    )
                )
        except Exception:
            pass

    codex_auth = Path("/mnt/c/Users/devon/.codex/auth.json")
    if codex_auth.is_file():
        try:
            data = json.loads(codex_auth.read_text(encoding="utf-8-sig", errors="replace"))
            if isinstance(data, dict) and data.get("tokens"):
                out.append(
                    Provider(
                        id="codex-subscription",
                        kind="cli",
                        enabled=True,
                        models=["codex-subscription"],
                        notes="Codex OAuth tokens present on Windows host",
                    )
                )
        except Exception:
            pass

    return out


def pick_route(model: str | None = None, preference: str | None = None) -> tuple[Provider, str]:
    """Return (provider, model_id) for a chat request."""
    providers = {p.id: p for p in list_providers() if p.enabled}
    pref = (preference or os.environ.get("HEIWA_DEFAULT_PROVIDER") or "auto").lower()
    model = (model or os.environ.get("HEIWA_DEFAULT_MODEL") or "").strip() or None

    if model and "/" in model:
        prefix, rest = model.split("/", 1)
        alias = {
            "openrouter": "openrouter",
            "ollama": "ollama",
            "openai": "openai",
            "xai": "xai",
            "grok": "xai",
            "anthropic": "anthropic",
            "claude": "anthropic",
            "gemini": "gemini",
            "google": "gemini",
            "groq": "groq",
            "cerebras": "cerebras",
        }.get(prefix.lower())
        if alias and alias in providers:
            return providers[alias], rest if alias != "openrouter" else model

    if pref != "auto" and pref in providers:
        p = providers[pref]
        mid = model or (p.models[0] if p.models else "default")
        return p, mid

    # auto order: openrouter (broad) -> ollama -> openai -> xai -> anthropic -> groq
    for pid in ("openrouter", "ollama", "openai", "xai", "anthropic", "gemini", "groq", "cerebras"):
        if pid in providers:
            p = providers[pid]
            mid = model or (p.models[0] if p.models else "default")
            return p, mid

    raise RuntimeError(
        "No inference provider available. Set OPENROUTER_API_KEY (recommended), "
        "OPENAI_API_KEY, XAI_API_KEY, ANTHROPIC_API_KEY, GEMINI_API_KEY, or start Ollama."
    )


async def chat_completion(
    messages: list[dict[str, str]],
    model: str | None = None,
    preference: str | None = None,
    temperature: float = 0.2,
    max_tokens: int = 2048,
) -> dict[str, Any]:
    provider, model_id = pick_route(model=model, preference=preference)

    if provider.kind == "cli":
        return await _cli_chat(provider.id, messages)

    if provider.id == "anthropic":
        return await _anthropic_chat(messages, model_id, temperature, max_tokens)

    return await _openai_compat_chat(provider, model_id, messages, temperature, max_tokens)


async def _openai_compat_chat(
    provider: Provider,
    model_id: str,
    messages: list[dict[str, str]],
    temperature: float,
    max_tokens: int,
) -> dict[str, Any]:
    key_map = {
        "openrouter": "OPENROUTER_API_KEY",
        "openai": "OPENAI_API_KEY",
        "xai": "XAI_API_KEY",
        "gemini": "GEMINI_API_KEY",
        "groq": "GROQ_API_KEY",
        "cerebras": "CEREBRAS_API_KEY",
        "ollama": None,
    }
    headers: dict[str, str] = {"Content-Type": "application/json"}
    env_name = key_map.get(provider.id)
    if env_name:
        key = _env(env_name) or (_env("GROK_API_KEY") if provider.id == "xai" else None) or (
            _env("GOOGLE_API_KEY") if provider.id == "gemini" else None
        )
        if not key and provider.id != "ollama":
            raise RuntimeError(f"Missing API key for {provider.id}")
        if key:
            headers["Authorization"] = f"Bearer {key}"
    if provider.id == "openrouter":
        headers["HTTP-Referer"] = os.environ.get("HEIWA_SITE_URL", "https://heiwa.ltd")
        headers["X-Title"] = "Heiwa MCP Gateway"

    body = {
        "model": model_id,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": False,
    }
    url = f"{provider.base_url.rstrip('/')}/chat/completions"
    async with httpx.AsyncClient(timeout=120.0) as client:
        r = await client.post(url, headers=headers, json=body)
        if r.status_code >= 400:
            raise RuntimeError(f"{provider.id} HTTP {r.status_code}: {r.text[:800]}")
        data = r.json()
    content = (
        data.get("choices", [{}])[0].get("message", {}).get("content")
        or data.get("choices", [{}])[0].get("text")
        or ""
    )
    return {
        "provider": provider.id,
        "model": model_id,
        "content": content,
        "usage": data.get("usage"),
        "raw_id": data.get("id"),
    }


async def _anthropic_chat(
    messages: list[dict[str, str]],
    model_id: str,
    temperature: float,
    max_tokens: int,
) -> dict[str, Any]:
    key = _env("ANTHROPIC_API_KEY")
    if not key:
        raise RuntimeError("Missing ANTHROPIC_API_KEY")
    system = "\n".join(m["content"] for m in messages if m.get("role") == "system")
    conv = [m for m in messages if m.get("role") in ("user", "assistant")]
    body: dict[str, Any] = {
        "model": model_id,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "messages": conv,
    }
    if system:
        body["system"] = system
    headers = {
        "x-api-key": key,
        "anthropic-version": "2023-06-01",
        "content-type": "application/json",
    }
    async with httpx.AsyncClient(timeout=120.0) as client:
        r = await client.post(
            "https://api.anthropic.com/v1/messages", headers=headers, json=body
        )
        if r.status_code >= 400:
            raise RuntimeError(f"anthropic HTTP {r.status_code}: {r.text[:800]}")
        data = r.json()
    parts = data.get("content") or []
    text = "".join(p.get("text", "") for p in parts if isinstance(p, dict))
    return {
        "provider": "anthropic",
        "model": model_id,
        "content": text,
        "usage": data.get("usage"),
        "raw_id": data.get("id"),
    }


async def _cli_chat(provider_id: str, messages: list[dict[str, str]]) -> dict[str, Any]:
    prompt = messages[-1]["content"] if messages else ""
    if provider_id in ("claude-cli", "claude-subscription"):
        bin_name = shutil.which("claude")
        if not bin_name:
            raise RuntimeError("claude CLI not installed in PATH")
        proc = subprocess.run(
            [bin_name, "-p", prompt, "--output-format", "text"],
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or "claude CLI failed")
        return {
            "provider": provider_id,
            "model": "claude-cli",
            "content": proc.stdout.strip(),
            "usage": None,
            "raw_id": None,
        }
    if provider_id in ("codex-cli", "codex-subscription"):
        bin_name = shutil.which("codex")
        if not bin_name:
            raise RuntimeError("codex CLI not installed in PATH")
        proc = subprocess.run(
            [bin_name, "exec", prompt],
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or "codex CLI failed")
        return {
            "provider": provider_id,
            "model": "codex-cli",
            "content": proc.stdout.strip(),
            "usage": None,
            "raw_id": None,
        }
    raise RuntimeError(f"CLI provider not implemented: {provider_id}")
