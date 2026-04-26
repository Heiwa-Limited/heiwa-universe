from __future__ import annotations

import os

os.environ.setdefault("HEIWA_STATE_BACKEND", "compatibility_sqlite")
os.environ.setdefault("DATABASE_PATH", "/tmp/heiwa-operator-approval-fallback.db")
os.environ.setdefault("HEIWA_AUTH_TOKEN", "test-heiwa-token")
os.environ.setdefault("HEIWA_EXECUTOR_RUNTIME", "railway")
os.environ.setdefault("HEIWA_MASTER_KEY", "test-heiwa-master-key-32-bytes!!")


class FakeSTDB:
    def __init__(self) -> None:
        self.queries: list[str] = []

    def query(self, sql: str) -> list[dict]:
        self.queries.append(sql)
        return []


def test_compatibility_mode_uses_memory_not_stdb():
    from apps.heiwa_hub import mcp_server
    from heiwa_hub.cognition.approval import get_approval_registry

    approvals = get_approval_registry()
    approvals._states.clear()  # type: ignore[attr-defined]
    approvals._payloads.clear()  # type: ignore[attr-defined]
    approvals.add(
        "task-memory-1",
        {
            "approval_id": "approval-task-memory-1",
            "risk_level": "critical",
            "source_surface": "web",
            "requested_by": "devon",
            "raw_text": "deploy the hub to production",
        },
    )

    compatibility_stdb = FakeSTDB()
    mcp_server.db.state_backend = "compatibility_sqlite"
    mcp_server.db.stdb = compatibility_stdb

    items = mcp_server._list_approvals_snapshot()
    assert len(items) == 1
    assert items[0].get("task_id") == "task-memory-1"
    assert items[0].get("approval_id") == "approval-task-memory-1"
    assert items[0].get("risk_level") == "critical"
    assert not compatibility_stdb.queries, "compatibility mode should not query STDB"


def test_spacetimedb_mode_falls_back_to_memory_when_stdb_empty():
    from apps.heiwa_hub import mcp_server

    empty_stdb = FakeSTDB()
    mcp_server.db.state_backend = "spacetimedb"
    mcp_server.db.stdb = empty_stdb

    items = mcp_server._list_approvals_snapshot()
    assert len(items) == 1, "should fall back to in-memory when STDB returns empty"
    assert empty_stdb.queries, "spacetimedb mode should query STDB before falling back"
