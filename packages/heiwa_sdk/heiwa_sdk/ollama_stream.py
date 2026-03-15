"""Async streaming client for local Ollama inference."""

from __future__ import annotations

import json
import logging
import os
from dataclasses import dataclass, field
from typing import Callable

logger = logging.getLogger("SDK.OllamaStream")


@dataclass(slots=True)
class OllamaUsage:
    """Token usage stats from the Ollama response."""
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_duration_ms: int = 0
    eval_rate: float = 0.0  # tokens/sec

    def summary_line(self) -> str:
        parts = []
        if self.prompt_tokens:
            parts.append(f"prompt:{self.prompt_tokens}")
        if self.completion_tokens:
            parts.append(f"completion:{self.completion_tokens}")
        if self.eval_rate > 0:
            parts.append(f"{self.eval_rate:.1f} tok/s")
        if self.total_duration_ms > 0:
            parts.append(f"{self.total_duration_ms}ms")
        return " | ".join(parts) if parts else ""


@dataclass(slots=True)
class StreamResult:
    """Result of a streaming Ollama call."""
    exit_code: int
    output: str
    usage: OllamaUsage = field(default_factory=OllamaUsage)


def _ollama_base_url() -> str:
    return (
        os.environ.get("HEIWA_OLLAMA_URL")
        or os.environ.get("OLLAMA_URL")
        or os.environ.get("OLLAMA_BASE_URL")
        or "http://127.0.0.1:11434"
    )


def _default_model() -> str:
    return os.environ.get("HEIWA_OLLAMA_MODEL") or "qwen3.5:4b"


async def stream_ollama(
    prompt: str,
    *,
    model: str | None = None,
    system: str | None = None,
    base_url: str | None = None,
    temperature: float = 0.2,
    num_ctx: int = 8192,
    timeout: float = 120.0,
    on_token: Callable[[str], None] | None = None,
) -> tuple[int, str]:
    """Stream tokens from Ollama, calling on_token for each chunk.

    Returns (exit_code, full_response).
    For usage stats, use stream_ollama_with_usage() instead.
    """
    result = await stream_ollama_with_usage(
        prompt, model=model, system=system, base_url=base_url,
        temperature=temperature, num_ctx=num_ctx, timeout=timeout,
        on_token=on_token,
    )
    return result.exit_code, result.output


async def stream_ollama_with_usage(
    prompt: str,
    *,
    model: str | None = None,
    system: str | None = None,
    base_url: str | None = None,
    temperature: float = 0.2,
    num_ctx: int = 8192,
    timeout: float = 120.0,
    on_token: Callable[[str], None] | None = None,
) -> StreamResult:
    """Stream tokens from Ollama with full usage stats.

    Returns StreamResult with exit_code, output, and usage.
    """
    url = f"{(base_url or _ollama_base_url()).rstrip('/')}/api/generate"
    model = model or _default_model()
    payload = {
        "model": model,
        "prompt": prompt,
        "stream": True,
        "options": {
            "temperature": temperature,
            "num_ctx": num_ctx,
        },
    }
    if system:
        payload["system"] = system

    try:
        import httpx
    except ImportError:
        logger.debug("httpx not available, falling back to non-streaming requests")
        code, text = await _fallback_no_stream(url, payload, timeout)
        return StreamResult(exit_code=code, output=text)

    chunks: list[str] = []
    usage = OllamaUsage()
    try:
        async with httpx.AsyncClient(timeout=httpx.Timeout(timeout, connect=10.0)) as client:
            async with client.stream("POST", url, json=payload) as resp:
                if resp.status_code != 200:
                    body = await resp.aread()
                    logger.error("Ollama returned HTTP %d: %s", resp.status_code, body.decode(errors="ignore")[:200])
                    return StreamResult(exit_code=1, output=f"Ollama HTTP {resp.status_code}")
                async for line in resp.aiter_lines():
                    if not line.strip():
                        continue
                    try:
                        data = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    token = data.get("response", "")
                    if token and on_token:
                        on_token(token)
                    if token:
                        chunks.append(token)
                    if data.get("done"):
                        # Parse usage from the final chunk
                        usage.prompt_tokens = int(data.get("prompt_eval_count", 0))
                        usage.completion_tokens = int(data.get("eval_count", 0))
                        total_ns = int(data.get("total_duration", 0))
                        usage.total_duration_ms = total_ns // 1_000_000 if total_ns else 0
                        eval_ns = int(data.get("eval_duration", 0))
                        if eval_ns > 0 and usage.completion_tokens > 0:
                            usage.eval_rate = usage.completion_tokens / (eval_ns / 1e9)
                        break
    except httpx.ConnectError:
        return StreamResult(exit_code=1, output="Ollama is not running at " + (base_url or _ollama_base_url()))
    except httpx.TimeoutException:
        partial = "".join(chunks)
        if partial:
            return StreamResult(exit_code=0, output=partial, usage=usage)
        return StreamResult(exit_code=1, output="TIMEOUT")
    except Exception as exc:
        logger.debug("Ollama stream error: %s", exc)
        return StreamResult(exit_code=1, output=str(exc))

    return StreamResult(exit_code=0, output="".join(chunks), usage=usage)


async def _fallback_no_stream(url: str, payload: dict, timeout: float) -> tuple[int, str]:
    """Non-streaming fallback using requests (sync, run in thread)."""
    import asyncio

    def _call():
        import requests as req
        payload["stream"] = False
        try:
            resp = req.post(url, json=payload, timeout=timeout)
            resp.raise_for_status()
            return 0, resp.json().get("response", "")
        except req.Timeout:
            return 1, "TIMEOUT"
        except req.RequestException as e:
            return 1, str(e)

    return await asyncio.to_thread(_call)


async def ollama_available(base_url: str | None = None) -> bool:
    """Quick health check — is Ollama responding?"""
    url = f"{(base_url or _ollama_base_url()).rstrip('/')}/api/tags"
    try:
        import httpx
        async with httpx.AsyncClient(timeout=3.0) as client:
            resp = await client.get(url)
            return resp.status_code == 200
    except Exception:
        return False


async def ollama_models(base_url: str | None = None) -> list[str]:
    """List models available in Ollama."""
    url = f"{(base_url or _ollama_base_url()).rstrip('/')}/api/tags"
    try:
        import httpx
        async with httpx.AsyncClient(timeout=3.0) as client:
            resp = await client.get(url)
            if resp.status_code == 200:
                return [m.get("name", "") for m in resp.json().get("models", [])]
    except Exception:
        pass
    return []
