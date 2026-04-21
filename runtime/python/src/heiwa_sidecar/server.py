"""Stdio JSONL server loop.

Reads Request frames from a text stream (default stdin), dispatches them,
and writes Response frames to another text stream (default stdout). On
`shutdown` the loop exits cleanly after replying.
"""

from __future__ import annotations

import asyncio
import json
import sys
from typing import Awaitable, Callable, TextIO

from pydantic import ValidationError

from .handlers import dispatch
from .protocol import ErrResponse, OkResponse, Request, err

Dispatcher = Callable[[Request], Awaitable[OkResponse | ErrResponse]]


async def serve(
    input_stream: TextIO | None = None,
    output_stream: TextIO | None = None,
    dispatcher: Dispatcher = dispatch,
) -> int:
    """Run the stdio loop until EOF or a `shutdown` op. Returns the exit code."""
    inp = input_stream if input_stream is not None else sys.stdin
    out = output_stream if output_stream is not None else sys.stdout

    for line in _line_iter(inp):
        line = line.strip()
        if not line:
            continue

        try:
            raw = json.loads(line)
            req = Request.model_validate(raw)
        except (json.JSONDecodeError, ValidationError) as exc:
            response = err("", code="parse_error", message=str(exc))
        else:
            response = await dispatcher(req)

        out.write(response.model_dump_json() + "\n")
        out.flush()

        if isinstance(response, OkResponse) and response.result == {"shutting_down": True}:
            return 0

    return 0


def _line_iter(stream: TextIO):
    while True:
        line = stream.readline()
        if not line:
            return
        yield line


def run() -> int:
    return asyncio.run(serve())
