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
        self.missions: list[dict] = []
        self.mission_steps: list[dict] = []
        self.cell_runs: list[dict] = []
        self.battlefields: dict[str, dict] = {}

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

    def create_mission(self, mission_data: dict) -> bool:
        self.missions.append(mission_data)
        return True

    def append_mission_step(self, step_data: dict) -> bool:
        self.mission_steps.append(step_data)
        return True

    def start_cell_run(self, run_data: dict) -> bool:
        self.cell_runs.append(run_data)
        return True

    def upsert_battlefield(self, battlefield_data: dict) -> bool:
        self.battlefields[battlefield_data["battlefield_id"]] = dict(battlefield_data)
        return True

    def get_battlefield(self, battlefield_id: str, owner_id: str | None = None) -> dict | None:
        row = self.battlefields.get(battlefield_id)
        if row is None:
            return None
        if owner_id and row.get("owner_id") != owner_id:
            return None
        return row


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


def test_ws_client_streams_live_owner_scoped_events(monkeypatch):
    original_auth_secret = os.environ.get("HEIWA_AUTH_SECRET")
    os.environ["HEIWA_AUTH_SECRET"] = "test-auth-secret"

    try:
        from apps.heiwa_hub import mcp_server
        from apps.heiwa_hub.auth import sign_jwt

        fake_db = FakeDatabase([])
        monkeypatch.setattr(mcp_server, "db", fake_db)
        monkeypatch.setattr(mcp_server, "CLIENT_STREAM_HEARTBEAT_SEC", 0.05, raising=False)

        token = sign_jwt(
            {
                "sub": "owner-devon",
                "owner_id": "owner-devon",
                "principal_id": "discord:123",
            }
        )

        client = TestClient(mcp_server.app)
        with client.websocket_connect(f"/ws/client?token={token}") as ws:
            mcp_server._append_wire_event(
                event_type="task_progress",
                payload={"task_id": "task-other", "message": "ignore"},
                owner_id="other-owner",
            )
            mcp_server._append_wire_event(
                event_type="task_progress",
                payload={"task_id": "task-owned", "message": "deliver"},
                owner_id="owner-devon",
                principal_id="discord:123",
            )
            payload = ws.receive_json()
            assert payload["event_type"] == "task_progress"
            assert payload["owner_id"] == "owner-devon"
            assert payload["payload_json"]["task_id"] == "task-owned"
    finally:
        if original_auth_secret is not None:
            os.environ["HEIWA_AUTH_SECRET"] = original_auth_secret
        else:
            os.environ.pop("HEIWA_AUTH_SECRET", None)


def test_ws_client_receives_live_task_accepted_event_from_task_ingress(monkeypatch):
    original_auth_token = os.environ.get("HEIWA_AUTH_TOKEN")
    os.environ["HEIWA_AUTH_TOKEN"] = "test-wire-token"

    try:
        from apps.heiwa_hub import mcp_server
        from heiwa_protocol.routing import BrokerRouteResult, BROKER_ENVELOPE_VERSION
        from heiwa_sdk.heiwaclaw.gateway import OpenClawDispatch

        async def fake_enrich(_request):
            return BrokerRouteResult(
                request_id="req-001",
                task_id="cli-task-accepted",
                envelope_version=BROKER_ENVELOPE_VERSION,
                raw_text="status",
                source_surface="cli",
                intent_class="inspect",
                risk_level="low",
                privacy_level="local",
                compute_class=1,
                assigned_worker="railway-hub",
                target_tool="heiwa_code",
                target_model="gpt-5.4",
                target_runtime="railway",
                target_tier="tier1_remote",
                requires_approval=False,
                rationale="test route",
            )

        fake_db = FakeDatabase([])
        monkeypatch.setattr(mcp_server, "db", fake_db)
        monkeypatch.setattr(mcp_server, "CLIENT_STREAM_HEARTBEAT_SEC", 0.05, raising=False)
        monkeypatch.setattr(mcp_server.enrichment, "enrich", fake_enrich)
        monkeypatch.setattr(
            mcp_server.claw_gateway,
            "resolve",
            lambda _route: OpenClawDispatch(
                gateway_tool="heiwa_code",
                adapter_tool="heiwa_code",
                provider="openai",
                target_model="gpt-5.4",
                target_runtime="railway",
                session_id="heiwa-cli-task-accepted",
                transport="subprocess",
                rate_group="openai",
                auth_kind="oauth",
                websocket_preferred=True,
                direct_execution=False,
                rationale="test dispatch",
            ),
        )
        monkeypatch.setattr(
            mcp_server.cells,
            "recommend",
            lambda _prompt: {"cell": {"cell_id": "codex-builder", "roster": [{"role": "builder"}]}},
        )

        client = TestClient(mcp_server.app)
        with client.websocket_connect("/ws/client?token=test-wire-token") as ws:
            response = client.post(
                "/tasks",
                headers={"Authorization": "Bearer test-wire-token"},
                json={
                    "raw_text": "status",
                    "sender_id": "cli-node",
                    "source_surface": "cli",
                    "task_id": "cli-task-accepted",
                    "session_id": "cli-session-123",
                    "battlefield_id": "bf-heiwa",
                },
            )
            assert response.status_code == 200
            payload = ws.receive_json()
            assert payload["event_type"] == "task_accepted"
            assert payload["owner_id"] == "local-operator"
            assert payload["mission_id"] == "cli-task-accepted"
            assert payload["session_id"] == "cli-session-123"
            assert payload["battlefield_id"] == "bf-heiwa"
            assert payload["payload_json"]["task_id"] == "cli-task-accepted"
    finally:
        if original_auth_token is not None:
            os.environ["HEIWA_AUTH_TOKEN"] = original_auth_token
        else:
            os.environ.pop("HEIWA_AUTH_TOKEN", None)


def test_register_battlefield_is_owner_scoped(monkeypatch):
    original_auth_token = os.environ.get("HEIWA_AUTH_TOKEN")
    os.environ["HEIWA_AUTH_TOKEN"] = "test-wire-token"

    try:
        from apps.heiwa_hub import mcp_server

        fake_db = FakeDatabase([])
        monkeypatch.setattr(mcp_server, "db", fake_db)

        client = TestClient(mcp_server.app)
        response = client.post(
            "/battlefields",
            headers={"Authorization": "Bearer test-wire-token"},
            json={
                "battlefield_id": "bf-heiwa",
                "name": "heiwa",
                "repo_url": "https://github.com/devonmcgreg/heiwa",
                "root_path": str(ROOT),
                "node_id": "cli-node",
                "status": "active",
            },
        )

        assert response.status_code == 200
        payload = response.json()
        assert payload["battlefield_id"] == "bf-heiwa"
        assert payload["owner_id"] == "local-operator"
        assert fake_db.battlefields["bf-heiwa"]["principal_id"] == "local-operator"
    finally:
        if original_auth_token is not None:
            os.environ["HEIWA_AUTH_TOKEN"] = original_auth_token
        else:
            os.environ.pop("HEIWA_AUTH_TOKEN", None)
