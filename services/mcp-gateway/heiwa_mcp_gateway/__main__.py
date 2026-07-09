"""Entry points:

  # MCP stdio (Grok Build, Claude Code, Codex MCP)
  heiwa-mcp
  python -m heiwa_mcp_gateway

  # MCP streamable HTTP + OpenAI-compatible proxy (Grok custom connector / API)
  heiwa-mcp --http --host 127.0.0.1 --port 8742
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import time
import uuid
from typing import Any

from .server import mcp
from .providers import chat_completion, list_providers


def main() -> None:
    parser = argparse.ArgumentParser(description="Heiwa MCP gateway")
    parser.add_argument(
        "--http",
        action="store_true",
        help="Serve streamable HTTP MCP + OpenAI-compatible /v1 endpoints",
    )
    parser.add_argument("--host", default=os.environ.get("HEIWA_MCP_HOST", "127.0.0.1"))
    parser.add_argument(
        "--port", type=int, default=int(os.environ.get("HEIWA_MCP_PORT", "8742"))
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("HEIWA_MCP_TOKEN", ""),
        help="Optional bearer token for HTTP mode",
    )
    args = parser.parse_args()

    if args.http:
        _run_http(args.host, args.port, args.token)
    else:
        # stdio MCP
        mcp.run(transport="stdio")


def _run_http(host: str, port: int, token: str) -> None:
    """Combine FastMCP streamable HTTP with OpenAI-compatible proxy."""
    import uvicorn
    from starlette.applications import Starlette
    from starlette.middleware import Middleware
    from starlette.middleware.base import BaseHTTPMiddleware
    from starlette.requests import Request
    from starlette.responses import JSONResponse, PlainTextResponse
    from starlette.routing import Mount, Route

    class AuthMiddleware(BaseHTTPMiddleware):
        async def dispatch(self, request: Request, call_next):
            if not token:
                return await call_next(request)
            # Health is open; everything else needs bearer
            if request.url.path in ("/health", "/", "/docs"):
                return await call_next(request)
            auth = request.headers.get("authorization", "")
            if auth != f"Bearer {token}":
                return JSONResponse({"error": "unauthorized"}, status_code=401)
            return await call_next(request)

    async def health(_: Request) -> JSONResponse:
        enabled = [p.id for p in list_providers() if p.enabled]
        return JSONResponse(
            {"ok": True, "service": "heiwa-mcp-gateway", "providers": enabled}
        )

    async def root(_: Request) -> JSONResponse:
        return JSONResponse(
            {
                "name": "heiwa-mcp-gateway",
                "mcp": "/mcp",
                "openai_compatible": {
                    "chat": "/v1/chat/completions",
                    "models": "/v1/models",
                },
                "health": "/health",
            }
        )

    async def models(_: Request) -> JSONResponse:
        data = []
        for p in list_providers():
            if not p.enabled:
                continue
            for m in p.models:
                data.append(
                    {
                        "id": f"{p.id}/{m}" if not m.startswith(p.id) else m,
                        "object": "model",
                        "owned_by": p.id,
                    }
                )
        return JSONResponse({"object": "list", "data": data})

    async def chat_completions(request: Request) -> JSONResponse:
        try:
            body = await request.json()
        except Exception:
            return JSONResponse({"error": "invalid json"}, status_code=400)
        messages = body.get("messages") or []
        model = body.get("model")
        temperature = float(body.get("temperature", 0.2))
        max_tokens = int(body.get("max_tokens") or body.get("max_completion_tokens") or 2048)
        # Map OpenAI-style model strings
        preference = None
        mid = model
        if isinstance(model, str) and "/" in model:
            pref, rest = model.split("/", 1)
            if pref in {
                "openrouter",
                "ollama",
                "openai",
                "xai",
                "anthropic",
                "gemini",
                "groq",
                "cerebras",
            }:
                preference = pref
                mid = rest if pref != "openrouter" else model
        try:
            result = await chat_completion(
                messages=[{"role": m["role"], "content": m.get("content", "")} for m in messages],
                model=mid,
                preference=preference,
                temperature=temperature,
                max_tokens=max_tokens,
            )
        except Exception as exc:
            return JSONResponse({"error": {"message": str(exc), "type": "heiwa_router_error"}}, status_code=502)

        created = int(time.time())
        cid = result.get("raw_id") or f"heiwa-{uuid.uuid4().hex[:12]}"
        return JSONResponse(
            {
                "id": cid,
                "object": "chat.completion",
                "created": created,
                "model": f"{result['provider']}/{result['model']}",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": result["content"]},
                        "finish_reason": "stop",
                    }
                ],
                "usage": result.get("usage")
                or {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
                "heiwa": {"provider": result["provider"], "model": result["model"]},
            }
        )

    # FastMCP streamable HTTP app
    mcp_app = mcp.streamable_http_app()

    routes = [
        Route("/", root),
        Route("/health", health),
        Route("/v1/models", models),
        Route("/v1/chat/completions", chat_completions, methods=["POST"]),
        Mount("/", app=mcp_app),
    ]
    middleware = [Middleware(AuthMiddleware)] if token else []
    app = Starlette(routes=routes, middleware=middleware)

    print(f"Heiwa MCP gateway HTTP on http://{host}:{port}")
    print(f"  MCP:     http://{host}:{port}/mcp")
    print(f"  OpenAI:  http://{host}:{port}/v1/chat/completions")
    print(f"  Health:  http://{host}:{port}/health")
    if token:
        print("  Auth:    Bearer token required")
    uvicorn.run(app, host=host, port=port, log_level="info")


if __name__ == "__main__":
    main()
