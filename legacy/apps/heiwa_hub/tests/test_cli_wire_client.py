from __future__ import annotations

import asyncio
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "packages/heiwa_cli"))
sys.path.insert(0, str(ROOT / "packages/heiwa_sdk"))
sys.path.insert(0, str(ROOT / "packages/heiwa_protocol"))
sys.path.insert(0, str(ROOT / "packages/heiwa_identity"))
sys.path.insert(0, str(ROOT / "apps"))


@pytest.mark.asyncio
async def test_wire_client_waits_for_terminal_task_event():
    from heiwa_cli.wire import WireClient

    client = WireClient(
        base_url="https://hub.example",
        token="test-token",
        battlefield_id="bf-heiwa",
        session_id="cli-session-123",
    )
    client._event_queue.put_nowait(
        {
            "event_type": "task_accepted",
            "mission_id": "task-1",
            "payload_json": {"task_id": "task-1", "status": "ACCEPTED"},
        }
    )
    client._event_queue.put_nowait(
        {
            "event_type": "task_result",
            "mission_id": "task-1",
            "payload_json": {"task_id": "task-1", "status": "PASS", "summary": "done"},
        }
    )

    exit_code, summary = await client.wait_for_task("task-1", timeout=0.1)
    assert exit_code == 0
    assert summary == "done"


@pytest.mark.asyncio
async def test_dispatch_once_uses_wire_client_in_distributed_mode(monkeypatch):
    import heiwa_cli.oneshot as oneshot

    class DummyCtx:
        def __init__(self) -> None:
            self.auth_token = "test-token"
            self.trace = False
            self.mode = "distributed"
            self.node_id = "cli-node"
            self.session_id = "cli-session-123"
            self.turns: list[tuple[str, str]] = []

        def hub_url_candidates(self) -> list[str]:
            return ["https://hub.example"]

        def battlefield_registration(self) -> dict:
            return {
                "battlefield_id": "bf-heiwa",
                "name": "heiwa",
                "repo_url": "https://github.com/devonmcgreg/heiwa",
                "root_path": str(ROOT),
                "node_id": "cli-node",
                "status": "active",
            }

        def remember_turn(self, role: str, content: str, **_extra) -> None:
            self.turns.append((role, content))

    class FakeWireClient:
        last_instance: "FakeWireClient | None" = None

        def __init__(self, *, base_url: str, token: str, battlefield_id: str, session_id: str):
            self.base_url = base_url
            self.token = token
            self.battlefield_id = battlefield_id
            self.session_id = session_id
            self.connected = False
            self.registered_payload: dict | None = None
            self.submitted_payload: dict | None = None
            self.closed = False
            FakeWireClient.last_instance = self

        async def connect(self) -> None:
            self.connected = True

        async def register_battlefield(self, payload: dict) -> dict:
            self.registered_payload = payload
            return payload

        async def submit_task(self, *, raw_text: str, sender_id: str, source_surface: str, task_id: str) -> dict:
            self.submitted_payload = {
                "raw_text": raw_text,
                "sender_id": sender_id,
                "source_surface": source_surface,
                "task_id": task_id,
            }
            return {"task_id": task_id, "route": {"target_tool": "heiwa_code"}}

        async def wait_for_task(self, task_id: str, timeout: float = 120.0, trace: bool = False) -> tuple[int, str]:
            assert task_id == self.submitted_payload["task_id"]
            assert trace is False
            return 0, "wire complete"

        async def close(self) -> None:
            self.closed = True

    monkeypatch.setattr("heiwa_sdk.operator_surface.maybe_fast_path_turn", lambda *_args, **_kwargs: None)
    monkeypatch.setattr(oneshot, "WireClient", FakeWireClient, raising=False)

    async def fail_direct_execute(*_args, **_kwargs):
        raise AssertionError("dispatch_once should not fall back to direct execution")

    monkeypatch.setattr(oneshot, "_direct_execute", fail_direct_execute)

    ctx = DummyCtx()
    exit_code = await oneshot.dispatch_once(ctx, "status")

    assert exit_code == 0
    instance = FakeWireClient.last_instance
    assert instance is not None
    assert instance.connected is True
    assert instance.registered_payload["battlefield_id"] == "bf-heiwa"
    assert instance.submitted_payload["raw_text"] == "status"
    assert instance.closed is True
    assert ("assistant", "wire complete") in ctx.turns
