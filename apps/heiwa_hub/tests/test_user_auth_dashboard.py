from __future__ import annotations

import hashlib
import hmac
import os
from dataclasses import dataclass

import pytest
from fastapi.testclient import TestClient

os.environ["HEIWA_STATE_BACKEND"] = "stateless"
os.environ["HEIWA_AUTH_TOKEN"] = "test-operator-token"
os.environ["HEIWA_AUTH_SECRET"] = "test-auth-secret"

from apps.heiwa_hub import mcp_server
from apps.heiwa_hub import auth as auth_module
from apps.heiwa_hub.auth import sign_jwt
from heiwa_sdk import HubStateService


@dataclass
class FakeSpacetimeDB:
    users: dict[str, dict]

    def _escape_sql_literal(self, value: str) -> str:
        return str(value).replace("'", "''")

    def query(self, sql: str):
        if "FROM users" not in sql:
            return []
        for user_id, row in self.users.items():
            if user_id in sql:
                return [dict(row)]
        return []


class FakeDatabase:
    state_backend = "spacetimedb"

    def __init__(self):
        self.stdb = FakeSpacetimeDB(
            users={
                "user-alpha": {
                    "user_id": "user-alpha",
                    "display_name": "Alpha",
                    "tier": "free",
                },
                "user-bravo": {
                    "user_id": "user-bravo",
                    "display_name": "Bravo",
                    "tier": "pro",
                },
            }
        )
        self.provider_accounts = [
            {"account_id": "acct-alpha", "provider_id": "google-gemini-cli", "user_id": "user-alpha", "status": "connected"},
            {"account_id": "acct-bravo", "provider_id": "codex", "user_id": "user-bravo", "status": "connected"},
        ]
        self.missions = [
            {"mission_id": "mission-alpha", "user_id": "user-alpha", "status": "running", "updated_at": "2026-03-25T18:00:00Z"},
            {"mission_id": "mission-bravo", "user_id": "user-bravo", "status": "running", "updated_at": "2026-03-25T17:00:00Z"},
            {"mission_id": "mission-alpha-complete", "user_id": "user-alpha", "status": "completed", "updated_at": "2026-03-25T16:00:00Z"},
            {"mission_id": "mission-bravo-failed", "user_id": "user-bravo", "status": "failed", "updated_at": "2026-03-25T15:00:00Z"},
        ]
        self.mission_steps = [
            {"mission_id": "mission-alpha", "step_id": "step-1", "position": 1, "updated_at": "2026-03-25T18:00:00Z"},
            {"mission_id": "mission-bravo", "step_id": "step-2", "position": 1, "updated_at": "2026-03-25T17:00:00Z"},
        ]
        self.cell_runs = [
            {"cell_run_id": "cell-alpha", "mission_id": "mission-alpha", "user_id": "user-alpha", "started_at": "2026-03-25T18:00:00Z"},
            {"cell_run_id": "cell-bravo", "mission_id": "mission-bravo", "user_id": "user-bravo", "started_at": "2026-03-25T17:00:00Z"},
        ]
        self.artifacts = [
            {"artifact_id": "artifact-alpha", "mission_id": "mission-alpha", "user_id": "user-alpha", "created_at": "2026-03-25T18:00:00Z"},
            {"artifact_id": "artifact-bravo", "mission_id": "mission-bravo", "user_id": "user-bravo", "created_at": "2026-03-25T17:00:00Z"},
        ]
        self.session_summaries = [
            {"summary_id": "summary-alpha", "session_id": "sess-alpha", "user_id": "user-alpha", "created_at": "2026-03-25T18:00:00Z"},
            {"summary_id": "summary-bravo", "session_id": "sess-bravo", "user_id": "user-bravo", "created_at": "2026-03-25T17:00:00Z"},
        ]
        self.runs = [
            {"run_id": "run-alpha", "proposal_id": "proposal-alpha", "user_id": "user-alpha", "ended_at": "2026-03-25T18:00:00Z"},
            {"run_id": "run-bravo", "proposal_id": "proposal-bravo", "user_id": "user-bravo", "ended_at": "2026-03-25T17:00:00Z"},
        ]

    def list_provider_accounts(self, provider_id=None, node_id=None, status=None, limit=100, user_id=None):
        rows = list(self.provider_accounts)
        if provider_id:
            rows = [row for row in rows if row["provider_id"] == provider_id]
        if status:
            rows = [row for row in rows if row["status"] == status]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]

    def get_missions(self, status=None, limit=50, user_id=None):
        rows = list(self.missions)
        if status:
            rows = [row for row in rows if row["status"] == status]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]

    def get_mission(self, mission_id, user_id=None):
        for row in self.missions:
            if row["mission_id"] != mission_id:
                continue
            if user_id and row["user_id"] != user_id:
                continue
            return dict(row)
        return None

    def get_mission_steps(self, mission_id, limit=100):
        rows = [row for row in self.mission_steps if row["mission_id"] == mission_id]
        return rows[:limit]

    def get_cell_runs(self, mission_id=None, status=None, limit=100, user_id=None):
        rows = list(self.cell_runs)
        if mission_id:
            rows = [row for row in rows if row["mission_id"] == mission_id]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]

    def list_artifacts(self, mission_id=None, limit=100, user_id=None):
        rows = list(self.artifacts)
        if mission_id:
            rows = [row for row in rows if row["mission_id"] == mission_id]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]

    def list_session_summaries(self, node_id=None, session_id=None, limit=50, user_id=None):
        rows = list(self.session_summaries)
        if session_id:
            rows = [row for row in rows if row["session_id"] == session_id]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]

    def get_runs(self, proposal_id=None, limit=50, user_id=None):
        rows = list(self.runs)
        if proposal_id:
            rows = [row for row in rows if row["proposal_id"] == proposal_id]
        if user_id:
            rows = [row for row in rows if row["user_id"] == user_id]
        return rows[:limit]


@pytest.fixture()
def auth_client(monkeypatch):
    fake_db = FakeDatabase()
    monkeypatch.setattr(mcp_server, "db", fake_db)
    monkeypatch.setattr(mcp_server, "state", HubStateService(fake_db))
    client = TestClient(mcp_server.app)
    return client


def _user_headers(user_id: str, discord_user_id: str) -> dict[str, str]:
    token = sign_jwt({"sub": user_id, "discord_user_id": discord_user_id, "username": user_id})
    return {"Authorization": f"Bearer {token}"}


def _state_token(nonce: str) -> str:
    sig = hmac.new(auth_module._auth_secret(), nonce.encode(), hashlib.sha256).hexdigest()[:16]
    return f"{nonce}.{sig}"


def test_dashboard_page_exists(auth_client: TestClient):
    response = auth_client.get("/dashboard.html")

    assert response.status_code == 200
    assert "Dashboard" in response.text


def test_auth_me_returns_user_profile_from_jwt(auth_client: TestClient):
    response = auth_client.get("/auth/me", headers=_user_headers("user-alpha", "discord-alpha"))

    assert response.status_code == 200
    payload = response.json()
    assert payload["user"]["user_id"] == "user-alpha"
    assert payload["claims"]["discord_user_id"] == "discord-alpha"


@pytest.mark.parametrize("path", ["/auth/providers", "/missions", "/history"])
def test_user_views_require_jwt(path: str, auth_client: TestClient):
    response = auth_client.get(path)

    assert response.status_code == 401


def test_user_views_are_scoped_to_authenticated_user(auth_client: TestClient):
    headers = _user_headers("user-alpha", "discord-alpha")

    providers = auth_client.get("/auth/providers", headers=headers)
    missions = auth_client.get("/missions", headers=headers)
    mission_detail = auth_client.get("/missions/mission-alpha", headers=headers)
    foreign_mission = auth_client.get("/missions/mission-bravo", headers=headers)
    history = auth_client.get("/history", headers=headers)

    assert providers.status_code == 200
    assert [row["user_id"] for row in providers.json()["providers"]] == ["user-alpha"]

    assert missions.status_code == 200
    assert {row["user_id"] for row in missions.json()["missions"]} == {"user-alpha"}

    assert mission_detail.status_code == 200
    assert mission_detail.json()["user_id"] == "user-alpha"
    assert {row["user_id"] for row in mission_detail.json()["cell_runs"]} == {"user-alpha"}
    assert {row["user_id"] for row in mission_detail.json()["artifacts"]} == {"user-alpha"}

    assert foreign_mission.status_code == 404

    assert history.status_code == 200
    payload = history.json()
    assert {row["user_id"] for row in payload["session_summaries"]} == {"user-alpha"}
    assert {row["user_id"] for row in payload["completed_missions"]} == {"user-alpha"}
    assert payload["failed_missions"] == []
    assert {row["user_id"] for row in payload["artifacts"]} == {"user-alpha"}
    assert {row["user_id"] for row in payload["recent_runs"]} == {"user-alpha"}


def test_validate_state_prunes_expired_entries(monkeypatch):
    expired = _state_token("expired")
    valid = _state_token("valid")
    now = 1_000_000.0
    monkeypatch.setattr(auth_module.time, "time", lambda: now)
    monkeypatch.setitem(auth_module._pending_states, expired, now - auth_module._STATE_TTL - 1)
    monkeypatch.setitem(auth_module._pending_states, valid, now - 1)

    assert auth_module._validate_state(valid) is True
    assert expired not in auth_module._pending_states
