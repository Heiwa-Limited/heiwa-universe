from __future__ import annotations

import json
import subprocess

from heiwa_sdk.spacetimedb import SpacetimeDB


class CaptureSpacetimeDB:
    def __init__(self) -> None:
        self._inner = SpacetimeDB("heiwa-test-db")
        self.commands: list[list[str]] = []

        def _capture(cmd: list[str], timeout: int = 10, cwd: str | None = None):
            self.commands.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, stdout="ok", stderr="")

        self._inner._run = _capture  # type: ignore[method-assign]

    def __getattr__(self, name: str):
        return getattr(self._inner, name)


def _find_json_arg(cmd: list[str], value: object) -> bool:
    for arg in cmd:
        try:
            if json.loads(arg) == value:
                return True
        except Exception:
            continue
    return False


def _reducer_args(cmd: list[str]) -> list[object]:
    return [json.loads(arg) for arg in cmd[6:]]


def test_add_proposal_encodes_option_fields():
    db = CaptureSpacetimeDB()
    db.add_proposal(
        {
            "proposal_id": "prop-1",
            "created_at": "2026-03-22T00:00:00+00:00",
            "status": "QUEUED",
            "fingerprint": None,
            "payload": {"task": "deploy"},
            "payload_raw": "deploy the hub",
            "mode": "PRODUCTION",
            "execution_targeting": {"requires": ["shell"]},
            "assigned_node_id": None,
            "hub_signature": None,
            "assignment_expires_at": None,
            "attempt_count": 0,
            "proposal_hash": None,
            "approved_at": None,
            "expires_at": None,
            "eligibility_snapshot": None,
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"none": []}), f"None fields should encode as option none: {cmd}"
    assert _find_json_arg(cmd, {"some": "deploy the hub"}), f"payload_raw should encode as option some: {cmd}"
    assert _find_json_arg(cmd, {"some": '{"requires":["shell"]}'}), f"execution_targeting should encode as option some: {cmd}"


def test_add_proposal_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.add_proposal(
        {
            "proposal_id": "prop-compat-1",
            "payload": {"task": "deploy"},
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "prop-compat-1"
    assert args[1] != {"some": "user-alpha"}
    assert args[-3] == {"some": "user-alpha"}


def test_add_proposal_appends_owner_id_and_principal_id():
    db = CaptureSpacetimeDB()
    db.add_proposal(
        {
            "proposal_id": "prop-owner-1",
            "payload": {"task": "deploy"},
            "owner_id": "owner-devon",
            "principal_id": "discord:123",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[-2] == {"some": "owner-devon"}
    assert args[-1] == {"some": "discord:123"}


def test_add_approval_request_encodes_option_fields():
    db = CaptureSpacetimeDB()
    db.add_approval_request(
        {
            "request_id": "APR-1",
            "proposal_id": "prop-1",
            "status": "PENDING",
            "requested_at": "2026-03-22T00:01:00+00:00",
            "expires_at": "2026-03-22T00:06:00+00:00",
            "requested_by": "heiwa-hub",
            "reason": "manual_approval_required",
            "payload": {"raw_text": "deploy the hub"},
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"some": "2026-03-22T00:06:00+00:00"}), f"expires_at should encode as option some: {cmd}"
    assert _find_json_arg(cmd, {"some": "manual_approval_required"}), f"reason should encode as option some: {cmd}"


def test_record_consent_encodes_option_fields():
    db = CaptureSpacetimeDB()
    db.record_consent(
        {
            "consent_id": "CON-1",
            "proposal_id": "prop-1",
            "proposal_hash": "hash-1",
            "actor_type": "human",
            "actor_id": "devon",
            "decision": "APPROVE",
            "comment": "ship it",
            "metadata": {"channel": "web"},
            "approval_request_id": None,
            "requested_by": None,
            "requested_at": "2026-03-22T00:01:00+00:00",
            "request_expires_at": "2026-03-22T01:01:00+00:00",
            "request_reason": "manual_approval_required",
            "request_payload": {"task": "deploy"},
            "approved_at": "2026-03-22T00:02:00+00:00",
            "expires_at": None,
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"some": "ship it"}), f"comment should encode as option some: {cmd}"
    assert _find_json_arg(cmd, {"some": '{"channel":"web"}'}), f"metadata should encode as option some: {cmd}"
    assert _find_json_arg(cmd, {"none": []}), f"None fields should encode as option none: {cmd}"
    assert _find_json_arg(cmd, {"some": '{"task":"deploy"}'}), f"request_payload should encode as option some: {cmd}"


def test_record_approval_decision_encodes_option_fields():
    db = CaptureSpacetimeDB()
    db.record_approval_decision(
        {
            "decision_id": "DEC-1",
            "request_id": "APR-1",
            "proposal_id": "prop-1",
            "actor_type": "human",
            "actor_id": "devon",
            "decision": "APPROVED",
            "reason": "ship it",
            "created_at": "2026-03-22T00:02:00+00:00",
            "metadata": {"channel": "web"},
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"some": "ship it"}), f"reason should encode as option some: {cmd}"


def test_revoke_capability_lease_encodes_none():
    db = CaptureSpacetimeDB()
    db.revoke_capability_lease("LEASE-1", "2026-03-22T00:03:00+00:00", None)
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"none": []}), f"None revoked_by should encode as option none: {cmd}"


def test_issue_capability_lease_encodes_chain_fields_and_routing_lock():
    db = CaptureSpacetimeDB()
    db.issue_capability_lease(
        {
            "lease_id": "LEASE-CHAIN-1",
            "proposal_id": "prop-1",
            "holder_id": "node-1",
            "expires_at": "2026-03-22T00:30:00+00:00",
            "parent_lease_id": "LEASE-PARENT-1",
            "failure_policy": "ABORT",
            "chain_state": "ACTIVE",
            "routing_lock": {
                "model_id": "codex/gpt-5.4",
                "provider": "codex",
                "runtime": "railway",
            },
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"some": "LEASE-PARENT-1"}), f"parent_lease_id should encode as option some: {cmd}"
    assert _find_json_arg(cmd, "ABORT"), f"failure_policy should be encoded: {cmd}"
    assert _find_json_arg(cmd, "ACTIVE"), f"chain_state should be encoded: {cmd}"
    assert _find_json_arg(
        cmd,
        {"some": '{"model_id":"codex/gpt-5.4","provider":"codex","runtime":"railway"}'},
    ), f"routing_lock should encode as option some JSON: {cmd}"


def test_revoke_capability_chain_encodes_none():
    db = CaptureSpacetimeDB()
    db.revoke_capability_chain("LEASE-CHAIN-1", "2026-03-22T00:03:00+00:00", None)
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"none": []}), f"None chain revocation reason should encode as option none: {cmd}"


def test_record_run_encodes_lease_id():
    db = CaptureSpacetimeDB()
    db.record_run(
        {
            "run_id": "run-1",
            "proposal_id": "prop-1",
            "lease_id": "LEASE-1",
            "status": "PASS",
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, "LEASE-1"), f"lease_id should be encoded in record_run: {cmd}"


def test_register_artifact_encodes_run_id():
    db = CaptureSpacetimeDB()
    db.register_artifact(
        {
            "artifact_id": "artifact-1",
            "run_id": "run-1",
            "mission_id": "task-1",
            "artifact_type": "log",
        }
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, {"some": "run-1"}), f"run_id should encode as option some: {cmd}"


def test_insert_execution_memory_encodes_lease_id():
    db = CaptureSpacetimeDB()
    db.insert_execution_memory(
        task_dispatch_id="task-1",
        lease_id="LEASE-1",
        model_used="codex/gpt-5.4",
        outcome="pass",
        duration_ms=123,
    )
    cmd = db.commands[-1]
    assert _find_json_arg(cmd, "LEASE-1"), f"lease_id should be encoded in execution memory insert: {cmd}"


def test_record_route_decision_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.record_route_decision(
        {
            "request_id": "req-1",
            "task_id": "task-1",
            "envelope_version": "2026-03-28",
            "raw_text": "route this",
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "req-1"
    assert args[1] == "task-1"
    assert {"some": "user-alpha"} in args


def test_record_route_decision_encodes_owner_and_principal():
    db = CaptureSpacetimeDB()
    db.record_route_decision(
        {
            "request_id": "req-2",
            "task_id": "task-2",
            "envelope_version": "2026-03-28",
            "raw_text": "route this",
            "owner_id": "owner-devon",
            "principal_id": "discord:456",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:456"} in args


def test_create_mission_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.create_mission(
        {
            "mission_id": "mission-1",
            "prompt": "ship it",
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "mission-1"
    assert {"some": "user-alpha"} in args


def test_create_mission_encodes_owner_id_and_principal_id():
    db = CaptureSpacetimeDB()
    db.create_mission(
        {
            "mission_id": "mission-owner-1",
            "prompt": "status",
            "owner_id": "owner-devon",
            "principal_id": "discord:123",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "mission-owner-1"
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:123"} in args


def test_create_mission_encodes_none_owner_id_and_principal_id():
    db = CaptureSpacetimeDB()
    db.create_mission(
        {
            "mission_id": "mission-no-owner",
            "prompt": "test",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "mission-no-owner"
    # owner_id and principal_id should be encoded as option none when absent
    none_count = sum(1 for a in args if a == {"none": []})
    assert none_count >= 2, f"Expected at least 2 none-encoded options for owner_id/principal_id, got {none_count}"


def test_start_cell_run_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.start_cell_run(
        {
            "cell_run_id": "cell-1",
            "mission_id": "mission-1",
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "cell-1"
    assert args[1] == "mission-1"
    assert {"some": "user-alpha"} in args


def test_start_cell_run_encodes_owner_and_principal():
    db = CaptureSpacetimeDB()
    db.start_cell_run(
        {
            "cell_run_id": "cell-owner-1",
            "mission_id": "mission-1",
            "owner_id": "owner-devon",
            "principal_id": "discord:123",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:123"} in args


def test_write_session_summary_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.write_session_summary(
        {
            "summary_id": "summary-1",
            "session_id": "session-1",
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "summary-1"
    assert args[1] == "session-1"
    assert {"some": "user-alpha"} in args


def test_write_session_summary_encodes_owner_and_principal():
    db = CaptureSpacetimeDB()
    db.write_session_summary(
        {
            "summary_id": "summary-owner-1",
            "session_id": "session-1",
            "owner_id": "owner-devon",
            "principal_id": "captain",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert {"some": "owner-devon"} in args
    assert {"some": "captain"} in args


def test_upsert_provider_account_status_appends_optional_user_id():
    db = CaptureSpacetimeDB()
    db.upsert_provider_account_status(
        {
            "account_id": "acct-1",
            "provider_id": "codex",
            "node_id": "node-1",
            "user_id": "user-alpha",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert args[0] == "acct-1"
    assert args[1] == "codex"
    assert args[2] == "node-1"
    assert {"some": "user-alpha"} in args


def test_upsert_provider_account_status_encodes_owner_and_principal():
    db = CaptureSpacetimeDB()
    db.upsert_provider_account_status(
        {
            "account_id": "acct-owner-1",
            "provider_id": "codex",
            "node_id": "node-1",
            "owner_id": "owner-devon",
            "principal_id": "discord:789",
        }
    )
    args = _reducer_args(db.commands[-1])
    assert {"some": "owner-devon"} in args
    assert {"some": "discord:789"} in args
