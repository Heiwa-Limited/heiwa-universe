from __future__ import annotations

import os
from unittest.mock import MagicMock

import pytest

os.environ.setdefault("HEIWA_STATE_BACKEND", "spacetimedb")
os.environ.setdefault("STDB_IDENTITY", "heiwa_test_module")

import heiwa_sdk.db as db_module


class FakeSpacetimeDB:
    def __init__(self, *args, **kwargs):
        self.calls: list[tuple[str, object]] = []

    def list_provider_accounts(self, provider_id=None, node_id=None, status=None, limit=50, user_id=None):
        self.calls.append(
            (
                "list_provider_accounts",
                {"provider_id": provider_id, "node_id": node_id, "status": status, "limit": limit, "user_id": user_id},
            )
        )
        return []

    def get_missions(self, status=None, limit=50, user_id=None):
        self.calls.append(("get_missions", {"status": status, "limit": limit, "user_id": user_id}))
        return []

    def get_mission(self, mission_id, user_id=None):
        self.calls.append(("get_mission", {"mission_id": mission_id, "user_id": user_id}))
        return None

    def get_cell_runs(self, mission_id=None, status=None, limit=100, user_id=None):
        self.calls.append(
            ("get_cell_runs", {"mission_id": mission_id, "status": status, "limit": limit, "user_id": user_id})
        )
        return []

    def list_artifacts(self, mission_id=None, limit=100, user_id=None):
        self.calls.append(("list_artifacts", {"mission_id": mission_id, "limit": limit, "user_id": user_id}))
        return []

    def list_session_summaries(self, node_id=None, session_id=None, limit=50, user_id=None):
        self.calls.append(
            ("list_session_summaries", {"node_id": node_id, "session_id": session_id, "limit": limit, "user_id": user_id})
        )
        return []

    def get_runs(self, proposal_id=None, limit=50, user_id=None):
        self.calls.append(("get_runs", {"proposal_id": proposal_id, "limit": limit, "user_id": user_id}))
        return []


def test_database_delegates_user_scoped_reads(monkeypatch):
    monkeypatch.setattr(db_module, "SpacetimeDB", FakeSpacetimeDB)

    db = db_module.Database()

    assert db.list_provider_accounts(user_id="user-alpha") == []
    assert db.get_missions(user_id="user-alpha") == []
    assert db.get_mission("mission-alpha", user_id="user-alpha") is None
    assert db.get_cell_runs(mission_id="mission-alpha", user_id="user-alpha") == []
    assert db.list_artifacts(mission_id="mission-alpha", user_id="user-alpha") == []
    assert db.list_session_summaries(user_id="user-alpha") == []
    assert db.get_runs(user_id="user-alpha") == []

    observed = {name: payload for name, payload in db.stdb.calls}
    assert observed["list_provider_accounts"]["user_id"] == "user-alpha"
    assert observed["get_missions"]["user_id"] == "user-alpha"
    assert observed["get_mission"]["user_id"] == "user-alpha"
    assert observed["get_cell_runs"]["user_id"] == "user-alpha"
    assert observed["list_artifacts"]["user_id"] == "user-alpha"
    assert observed["list_session_summaries"]["user_id"] == "user-alpha"
    assert observed["get_runs"]["user_id"] == "user-alpha"


@pytest.mark.parametrize(
    ("method_name", "kwargs", "expected_clause"),
    [
        ("list_provider_accounts", {"user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("get_missions", {"user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("get_mission", {"mission_id": "mission-alpha", "user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("get_cell_runs", {"mission_id": "mission-alpha", "user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("list_session_summaries", {"user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("list_artifacts", {"mission_id": "mission-alpha", "user_id": "user-alpha"}, "user_id = 'user-alpha'"),
        ("get_runs", {"user_id": "user-alpha"}, "user_id = 'user-alpha'"),
    ],
)
def test_spacetimedb_query_helpers_include_user_scope(method_name, kwargs, expected_clause):
    from heiwa_sdk.spacetimedb import SpacetimeDB

    stdb = SpacetimeDB.__new__(SpacetimeDB)
    stdb.query = MagicMock(return_value=[])

    getattr(stdb, method_name)(**kwargs)

    stdb.query.assert_called_once()
    sql = stdb.query.call_args[0][0]
    assert expected_clause in sql
