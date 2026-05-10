from __future__ import annotations

import json
from typing import Any


def _parse_json_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return dict(value)
    if isinstance(value, str) and value.strip():
        try:
            parsed = json.loads(value)
            if isinstance(parsed, dict):
                return parsed
        except Exception:
            return {}
    return {}


def list_approvals_from_stdb(stdb: Any, status: str | None = None, limit: int = 100) -> list[dict[str, Any]]:
    query = "SELECT * FROM approval_requests"
    if status:
        escaped_status = str(status).replace("'", "''")
        query += f" WHERE status = '{escaped_status}'"
    query += f" LIMIT {int(limit)}"
    requests = stdb.query(query)
    decisions = stdb.query("SELECT * FROM approval_decisions LIMIT 500")

    latest_by_request: dict[str, dict[str, Any]] = {}
    for row in sorted(decisions, key=lambda item: item.get("created_at", "")):
        request_id = str(row.get("request_id") or "")
        if request_id:
            latest_by_request[request_id] = row

    items: list[dict[str, Any]] = []
    for row in requests:
        payload = _parse_json_object(row.get("payload_json"))
        decision = latest_by_request.get(str(row.get("request_id") or ""))
        raw_text = str(payload.get("raw_text") or payload.get("prompt") or "")
        items.append(
            {
                "task_id": row.get("proposal_id"),
                "status": row.get("status"),
                "created_at": row.get("requested_at"),
                "expires_at": row.get("expires_at"),
                "decision_by": decision.get("actor_id") if decision else None,
                "decision_at": decision.get("created_at") if decision else None,
                "reason": decision.get("reason") if decision else row.get("reason"),
                "approval_id": row.get("request_id"),
                "risk_level": payload.get("risk_level"),
                "source_surface": payload.get("source_surface"),
                "requested_by": payload.get("requested_by") or row.get("requested_by"),
                "raw_text_excerpt": raw_text[:200],
            }
        )
    return items
