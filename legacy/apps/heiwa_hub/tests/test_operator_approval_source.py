from __future__ import annotations

import json

from heiwa_hub.approval_views import list_approvals_from_stdb


class FakeSTDB:
    def query(self, sql: str) -> list[dict]:
        if "FROM approval_requests" in sql:
            return [
                {
                    "request_id": "approval-task-1",
                    "proposal_id": "task-1",
                    "status": "PENDING",
                    "requested_at": "2026-03-22T10:00:00+00:00",
                    "expires_at": "2026-03-22T10:10:00+00:00",
                    "requested_by": "devon",
                    "reason": "Critical deploy must hold",
                    "payload_json": json.dumps(
                        {
                            "risk_level": "critical",
                            "source_surface": "web",
                            "requested_by": "devon",
                            "raw_text": "deploy the hub to production",
                        }
                    ),
                }
            ]
        if "FROM approval_decisions" in sql:
            return []
        raise AssertionError(f"unexpected SQL: {sql}")


def test_stdb_approval_source_returns_structured_approval():
    approvals = list_approvals_from_stdb(FakeSTDB())
    assert len(approvals) == 1

    approval = approvals[0]
    assert approval.get("task_id") == "task-1"
    assert approval.get("approval_id") == "approval-task-1"
    assert approval.get("risk_level") == "critical"
    assert approval.get("source_surface") == "web"
    assert "deploy the hub" in str(approval.get("raw_text_excerpt") or "")
