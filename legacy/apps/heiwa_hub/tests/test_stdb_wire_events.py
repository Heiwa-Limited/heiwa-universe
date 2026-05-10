"""Tests for event log STDB bridge and Database facade.

Uses CaptureSpacetimeDB pattern (from test_stdb_option_encoding.py) to intercept
CLI invocations without requiring a live STDB instance.
"""
from __future__ import annotations

import json
import subprocess

import pytest

from heiwa_sdk.spacetimedb import SpacetimeDB
from heiwa_sdk.db import Database


# ── Test harness ─────────────────────────────────────────────────────────────


class CaptureSpacetimeDB:
    def __init__(self) -> None:
        self._inner = SpacetimeDB("heiwa-test-db")
        self.commands: list[list[str]] = []
        self.queries: list[str] = []

        def _capture(cmd: list[str], timeout: int = 10, cwd: str | None = None):
            self.commands.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, stdout="ok", stderr="")

        self._inner._run = _capture  # type: ignore[method-assign]
        # Patch query so SQL assertions work without a real STDB server
        original_query = self._inner.query

        def _capture_query(sql: str):
            self.queries.append(sql)
            return []

        self._inner.query = _capture_query  # type: ignore[method-assign]

    def __getattr__(self, name: str):
        return getattr(self._inner, name)


class FakeSpacetimeDB:
    def __init__(self, *args, **kwargs):
        self.calls: list[tuple[str, object]] = []

    def append_event(self, event_data: dict) -> bool:
        self.calls.append(("append_event", event_data))
        return True

    def list_events(
        self,
        after_event_id: str | None = None,
        owner_id: str | None = None,
        limit: int = 100,
    ) -> list[dict]:
        self.calls.append(("list_events", {"after_event_id": after_event_id, "owner_id": owner_id, "limit": limit}))
        return []


def _reducer_args(cmd: list[str]) -> list[object]:
    """Parse JSON-encoded reducer args (skip the first 6 CLI meta args)."""
    return [json.loads(arg) for arg in cmd[6:]]


# ── append_event ─────────────────────────────────────────────────────────────


def test_append_event_calls_reducer():
    db = CaptureSpacetimeDB()
    db.append_event(
        {
            "event_id": "evt-001",
            "owner_id": "owner-devon",
            "principal_id": "discord:123",
            "session_id": "sess-1",
            "mission_id": "mission-1",
            "battlefield_id": "bf-1",
            "event_type": "tool_call",
            "payload_json": '{"tool":"bash"}',
            "created_at": "2026-03-28T00:00:00+00:00",
        }
    )
    assert len(db.commands) == 1
    cmd = db.commands[0]
    # Reducer name is the 6th element (index 5) in the CLI command
    assert cmd[5] == "append_event"
    args = _reducer_args(cmd)
    assert args[0] == "evt-001"
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:123"} in args
    assert {"some": "sess-1"} in args
    assert {"some": "mission-1"} in args
    assert {"some": "bf-1"} in args
    assert "tool_call" in args
    assert '{"tool":"bash"}' in args


def test_append_event_encodes_none_optionals():
    db = CaptureSpacetimeDB()
    db.append_event(
        {
            "event_id": "evt-002",
            "event_type": "heartbeat",
            "payload_json": "{}",
        }
    )
    args = _reducer_args(db.commands[0])
    none_count = sum(1 for a in args if a == {"none": []})
    # owner_id, principal_id, session_id, mission_id, battlefield_id — all None → 5 nones
    assert none_count >= 5, f"Expected at least 5 none-encoded options, got {none_count}: {args}"


# ── list_events SQL ──────────────────────────────────────────────────────────


def test_list_events_sql_with_owner_filter():
    db = CaptureSpacetimeDB()
    db.list_events(owner_id="owner-devon")
    assert len(db.queries) == 1
    sql = db.queries[0]
    assert "owner_id" in sql
    assert "owner-devon" in sql


def test_list_events_sql_with_after_filter():
    db = CaptureSpacetimeDB()
    db.list_events(after_event_id="evt-050")
    sql = db.queries[0]
    assert "event_id" in sql
    assert "evt-050" in sql
    assert ">" in sql


def test_list_events_sql_combined_filters():
    db = CaptureSpacetimeDB()
    db.list_events(owner_id="owner-devon", after_event_id="evt-100")
    sql = db.queries[0]
    assert "owner_id" in sql
    assert "owner-devon" in sql
    assert "event_id" in sql
    assert "evt-100" in sql


def test_list_events_default_limit():
    db = CaptureSpacetimeDB()
    db.list_events()
    sql = db.queries[0]
    assert "LIMIT" in sql.upper()
    assert "100" in sql


# ── Database facade delegation ────────────────────────────────────────────────


def _make_facade_with_fake_stdb() -> tuple[Database, FakeSpacetimeDB]:
    fake = FakeSpacetimeDB()
    db = Database.__new__(Database)
    db.stdb = fake
    db.state_backend = "spacetimedb"
    return db, fake


def test_facade_append_event_delegates():
    db, fake = _make_facade_with_fake_stdb()
    result = db.append_event({"event_id": "evt-facade-1", "event_type": "test", "payload_json": "{}"})
    assert result is True
    assert any(name == "append_event" for name, _ in fake.calls)


def test_facade_list_events_delegates():
    db, fake = _make_facade_with_fake_stdb()
    result = db.list_events(owner_id="owner-devon", after_event_id="evt-1", limit=10)
    assert result == []
    call_names = [name for name, _ in fake.calls]
    assert "list_events" in call_names


def test_facade_list_events_returns_empty_without_stdb():
    db = Database.__new__(Database)
    db.stdb = None
    db.state_backend = "none"
    result = db.list_events()
    assert result == []


def test_facade_append_event_returns_false_without_stdb():
    db = Database.__new__(Database)
    db.stdb = None
    db.state_backend = "none"
    result = db.append_event({"event_id": "evt-no-stdb", "event_type": "test", "payload_json": "{}"})
    assert result is False
