from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "packages/heiwa_sdk"))


class FakeSpacetimeDB:
    def __init__(self, *args, **kwargs):
        self.lease = None

    def get_active_capability_lease(self, proposal_id, holder_id):
        return self.lease

    def register_artifact(self, artifact_data):
        return True


def _manager_with_lease(monkeypatch, lease):
    import heiwa_sdk.spacetimedb as stdb_module
    from heiwa_sdk.hooks import ExecutionHookManager

    fake = FakeSpacetimeDB()
    fake.lease = lease
    monkeypatch.setattr(stdb_module, "SpacetimeDB", lambda *args, **kwargs: fake)
    manager = ExecutionHookManager(ROOT)
    return manager


def test_before_tool_call_denies_tool_outside_tool_scope_json(monkeypatch):
    monkeypatch.setenv("HEIWA_ROLLOUT_MODE", "enforce")
    manager = _manager_with_lease(
        monkeypatch,
        {
            "lease_id": "LEASE-1",
            "tool_scope_json": '["heiwa_claude"]',
            "filesystem_scope_json": "{}",
            "network_scope_json": "{}",
            "secret_scope_json": "[]",
        },
    )

    allow, reason, metadata = manager.before_tool_call(
        tool="heiwa_code",
        proposal_id="proposal-1",
        node_id="node-1",
        payload={},
    )

    assert allow is False
    assert "not authorized" in reason
    assert metadata is None


def test_before_tool_call_fails_closed_when_scope_fields_missing(monkeypatch):
    monkeypatch.setenv("HEIWA_ROLLOUT_MODE", "enforce")
    manager = _manager_with_lease(
        monkeypatch,
        {
            "lease_id": "LEASE-2",
            "tool_scope_json": '["heiwa_code"]',
            "network_scope_json": "{}",
            "secret_scope_json": "[]",
        },
    )

    allow, reason, metadata = manager.before_tool_call(
        tool="heiwa_code",
        proposal_id="proposal-2",
        node_id="node-2",
        payload={},
    )

    assert allow is False
    assert "Missing lease scope field" in reason
    assert metadata is None


def test_before_tool_call_denies_mismatched_routing_lock(monkeypatch):
    monkeypatch.setenv("HEIWA_ROLLOUT_MODE", "enforce")
    manager = _manager_with_lease(
        monkeypatch,
        {
            "lease_id": "LEASE-3",
            "tool_scope_json": '["heiwa_code"]',
            "filesystem_scope_json": "{}",
            "network_scope_json": "{}",
            "secret_scope_json": "[]",
            "routing_lock_json": '{"model_id":"codex/gpt-5.4","provider":"codex","runtime":"railway"}',
        },
    )

    allow, reason, metadata = manager.before_tool_call(
        tool="heiwa_code",
        proposal_id="proposal-3",
        node_id="node-3",
        payload={
            "target_model": "codex/gpt-5.3-codex",
            "provider": "codex",
            "target_runtime": "railway",
        },
    )

    assert allow is False
    assert "routing lock" in reason.lower()
    assert metadata is None
