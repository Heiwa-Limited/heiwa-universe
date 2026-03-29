from __future__ import annotations

import os
import sys
from pathlib import Path

from fastapi.testclient import TestClient

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "packages/heiwa_sdk"))
sys.path.insert(0, str(ROOT / "packages/heiwa_protocol"))
sys.path.insert(0, str(ROOT / "packages/heiwa_identity"))
sys.path.insert(0, str(ROOT / "apps"))


class FakeDatabase:
    def __init__(self, events: list[dict]):
        self.events = events

    def list_events(
        self,
        after_event_id: str | None = None,
        owner_id: str | None = None,
        limit: int = 200,
    ) -> list[dict]:
        rows = list(self.events)
        if owner_id:
            rows = [row for row in rows if row.get("owner_id") == owner_id]
        if after_event_id:
            rows = [row for row in rows if str(row.get("event_id", "")) > after_event_id]
        return rows[:limit]

    def append_event(self, event_data: dict) -> bool:
        self.events.append(event_data)
        return True


def test_ws_client_replays_after_last_seen_event_id(monkeypatch):
    original_auth_secret = os.environ.get("HEIWA_AUTH_SECRET")
    os.environ["HEIWA_AUTH_SECRET"] = "test-auth-secret"

    try:
        from apps.heiwa_hub import mcp_server
        from apps.heiwa_hub.auth import sign_jwt

        fake_db = FakeDatabase(
            [
                {"event_id": "evt-001", "owner_id": "owner-devon", "event_type": "mission_created"},
                {"event_id": "evt-002", "owner_id": "owner-devon", "event_type": "task_started"},
                {"event_id": "evt-003", "owner_id": "other-owner", "event_type": "task_started"},
            ]
        )
        monkeypatch.setattr(mcp_server, "db", fake_db)

        token = sign_jwt(
            {
                "sub": "owner-devon",
                "owner_id": "owner-devon",
                "principal_id": "discord:123",
            }
        )

        client = TestClient(mcp_server.app)
        with client.websocket_connect(f"/ws/client?token={token}&last_seen_event_id=evt-001") as ws:
            payload = ws.receive_json()
            assert payload["event_id"] == "evt-002"
            assert payload["owner_id"] == "owner-devon"
    finally:
        if original_auth_secret is not None:
            os.environ["HEIWA_AUTH_SECRET"] = original_auth_secret
        else:
            os.environ.pop("HEIWA_AUTH_SECRET", None)
