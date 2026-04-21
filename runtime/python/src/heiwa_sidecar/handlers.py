"""Op handlers dispatched from the stdio server.

Each op is a small async function. The real LangGraph/LlamaIndex/Ragas work
lives behind lazy imports so `health` + `version` stay fast even before the
heavy ML deps resolve in an install.
"""

from __future__ import annotations

import importlib
import platform
import sys
from typing import Any, Awaitable, Callable

from . import __version__
from .protocol import ErrResponse, OkResponse, Request, err, ok

Handler = Callable[[Request], Awaitable[OkResponse | ErrResponse]]


async def op_health(req: Request) -> OkResponse | ErrResponse:
    return ok(req.id, {"status": "ok"})


async def op_version(req: Request) -> OkResponse | ErrResponse:
    return ok(
        req.id,
        {
            "sidecar": __version__,
            "python": sys.version.split()[0],
            "platform": platform.platform(),
        },
    )


async def op_check_deps(req: Request) -> OkResponse | ErrResponse:
    """Probe importability of each ML dep without actually running them."""
    results: dict[str, Any] = {}
    for name in ("langgraph", "llama_index", "ragas"):
        try:
            mod = importlib.import_module(name)
            version = getattr(mod, "__version__", None)
            results[name] = {"importable": True, "version": version}
        except Exception as exc:
            results[name] = {"importable": False, "error": repr(exc)}
    return ok(req.id, results)


async def op_echo(req: Request) -> OkResponse | ErrResponse:
    return ok(req.id, req.args)


async def op_shutdown(req: Request) -> OkResponse | ErrResponse:
    return ok(req.id, {"shutting_down": True})


HANDLERS: dict[str, Handler] = {
    "health": op_health,
    "version": op_version,
    "check_deps": op_check_deps,
    "echo": op_echo,
    "shutdown": op_shutdown,
}


async def dispatch(req: Request) -> OkResponse | ErrResponse:
    handler = HANDLERS.get(req.op)
    if handler is None:
        return err(req.id, code="unknown_op", message=f"no handler for op '{req.op}'")
    try:
        return await handler(req)
    except Exception as exc:
        return err(req.id, code="handler_error", message=repr(exc))
