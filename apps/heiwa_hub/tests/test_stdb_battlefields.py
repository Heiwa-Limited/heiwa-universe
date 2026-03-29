"""Tests for battlefield STDB bridge and Database facade.

Uses CaptureSpacetimeDB pattern to intercept CLI invocations without a live STDB.
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

        def _capture_query(sql: str):
            self.queries.append(sql)
            return []

        self._inner.query = _capture_query  # type: ignore[method-assign]

    def __getattr__(self, name: str):
        return getattr(self._inner, name)


class FakeSpacetimeDB:
    def __init__(self, *args, **kwargs):
        self.calls: list[tuple[str, object]] = []

    def upsert_battlefield(self, bf_data: dict) -> bool:
        self.calls.append(("upsert_battlefield", bf_data))
        return True

    def list_battlefields(self, owner_id: str | None = None, limit: int = 50) -> list[dict]:
        self.calls.append(("list_battlefields", {"owner_id": owner_id, "limit": limit}))
        return []

    def get_battlefield(self, battlefield_id: str, owner_id: str | None = None) -> dict | None:
        self.calls.append(("get_battlefield", {"battlefield_id": battlefield_id, "owner_id": owner_id}))
        return None


def _reducer_args(cmd: list[str]) -> list[object]:
    return [json.loads(arg) for arg in cmd[6:]]


# ── upsert_battlefield ───────────────────────────────────────────────────────


def test_upsert_battlefield_calls_reducer():
    db = CaptureSpacetimeDB()
    db.upsert_battlefield(
        {
            "battlefield_id": "bf-001",
            "owner_id": "owner-devon",
            "principal_id": "discord:123",
            "name": "heiwa-hub",
            "repo_url": "https://github.com/devonmcgreg/heiwa",
            "root_path": "/workspace/heiwa",
            "node_id": "node-local-1",
            "status": "active",
            "created_at": "2026-03-28T00:00:00+00:00",
            "last_active_at": "2026-03-28T00:01:00+00:00",
        }
    )
    assert len(db.commands) == 1
    cmd = db.commands[0]
    assert cmd[5] == "upsert_battlefield"
    args = _reducer_args(cmd)
    assert args[0] == "bf-001"
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:123"} in args
    assert "heiwa-hub" in args
    assert {"some": "https://github.com/devonmcgreg/heiwa"} in args
    assert {"some": "/workspace/heiwa"} in args
    assert {"some": "node-local-1"} in args
    assert "active" in args


def test_upsert_battlefield_encodes_none_optionals():
    db = CaptureSpacetimeDB()
    db.upsert_battlefield(
        {
            "battlefield_id": "bf-002",
            "name": "bare-battlefield",
            "status": "active",
        }
    )
    args = _reducer_args(db.commands[0])
    none_count = sum(1 for a in args if a == {"none": []})
    # owner_id, principal_id, repo_url, root_path, node_id — all None → 5 nones
    assert none_count >= 5, f"Expected at least 5 none-encoded options, got {none_count}: {args}"


# ── list_battlefields SQL ────────────────────────────────────────────────────


def test_list_battlefields_sql_with_owner():
    db = CaptureSpacetimeDB()
    db.list_battlefields(owner_id="owner-devon")
    assert len(db.queries) == 1
    sql = db.queries[0]
    assert "owner_id" in sql
    assert "owner-devon" in sql


def test_list_battlefields_default_limit():
    db = CaptureSpacetimeDB()
    db.list_battlefields()
    sql = db.queries[0]
    assert "LIMIT" in sql.upper()
    assert "50" in sql


# ── get_battlefield SQL ──────────────────────────────────────────────────────


def test_get_battlefield_with_owner():
    db = CaptureSpacetimeDB()
    db.get_battlefield("bf-001", owner_id="owner-devon")
    assert len(db.queries) == 1
    sql = db.queries[0]
    assert "bf-001" in sql
    assert "owner_id" in sql
    assert "owner-devon" in sql


def test_get_battlefield_without_owner():
    db = CaptureSpacetimeDB()
    db.get_battlefield("bf-001")
    sql = db.queries[0]
    assert "bf-001" in sql
    # No owner_id filter when not provided
    assert "owner_id" not in sql


# ── archive_battlefield ──────────────────────────────────────────────────────


def test_archive_battlefield_calls_reducer():
    db = CaptureSpacetimeDB()
    db.archive_battlefield("bf-001")
    assert len(db.commands) == 1
    cmd = db.commands[0]
    assert cmd[5] == "archive_battlefield"
    args = _reducer_args(cmd)
    assert args[0] == "bf-001"


# ── Database facade delegation ────────────────────────────────────────────────


def _make_facade_with_fake_stdb() -> tuple[Database, FakeSpacetimeDB]:
    fake = FakeSpacetimeDB()
    db = Database.__new__(Database)
    db.stdb = fake
    db.state_backend = "spacetimedb"
    return db, fake


def test_facade_upsert_battlefield_delegates():
    db, fake = _make_facade_with_fake_stdb()
    result = db.upsert_battlefield({"battlefield_id": "bf-facade-1", "name": "test", "status": "active"})
    assert result is True
    assert any(name == "upsert_battlefield" for name, _ in fake.calls)


def test_facade_list_battlefields_delegates():
    db, fake = _make_facade_with_fake_stdb()
    result = db.list_battlefields(owner_id="owner-devon")
    assert result == []
    call_names = [name for name, _ in fake.calls]
    assert "list_battlefields" in call_names


def test_facade_get_battlefield_delegates():
    db, fake = _make_facade_with_fake_stdb()
    result = db.get_battlefield("bf-001", owner_id="owner-devon")
    assert result is None
    call_names = [name for name, _ in fake.calls]
    assert "get_battlefield" in call_names


def test_facade_upsert_battlefield_returns_false_without_stdb():
    db = Database.__new__(Database)
    db.stdb = None
    db.state_backend = "none"
    result = db.upsert_battlefield({"battlefield_id": "bf-no-stdb", "name": "test", "status": "active"})
    assert result is False


def test_facade_list_battlefields_returns_empty_without_stdb():
    db = Database.__new__(Database)
    db.stdb = None
    db.state_backend = "none"
    result = db.list_battlefields()
    assert result == []


def test_facade_get_battlefield_returns_none_without_stdb():
    db = Database.__new__(Database)
    db.stdb = None
    db.state_backend = "none"
    result = db.get_battlefield("bf-001")
    assert result is None
