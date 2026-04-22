"""Wire protocol between Rust runtime and Python sidecar.

Framing: one JSON object per line on stdin / stdout (JSONL). The Rust side
writes Requests to our stdin and reads Responses from our stdout. Stderr is
reserved for free-form logs.

A Request carries `id`, `op`, and `args`. A Response echoes `id` and is one
of `ok` (with `result`) or `err` (with `code` + `message`).
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, Field


class Request(BaseModel):
    id: str
    op: str
    args: dict[str, Any] = Field(default_factory=dict)


class OkResponse(BaseModel):
    id: str
    status: Literal["ok"] = "ok"
    result: Any = None


class ErrResponse(BaseModel):
    id: str
    status: Literal["err"] = "err"
    code: str
    message: str


Response = OkResponse | ErrResponse


def ok(request_id: str, result: Any = None) -> OkResponse:
    return OkResponse(id=request_id, result=result)


def err(request_id: str, code: str, message: str) -> ErrResponse:
    return ErrResponse(id=request_id, code=code, message=message)
