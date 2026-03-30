from __future__ import annotations

import importlib.util
import subprocess
from pathlib import Path


BROKER_PATH = Path("/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/heiwa_devonx_legacy.py")


def _load_broker_module():
    spec = importlib.util.spec_from_file_location("heiwa_devonx_legacy", BROKER_PATH)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _request(**overrides):
    request = {
        "request_id": "req_test",
        "origin_surface": "antigravity",
        "action": "write-file",
        "target_surface": "filesystem",
        "target_scope": "/Users/dmcgregsauce/.gemini/settings.json",
        "requested_mode": "write",
        "arguments": {},
    }
    request.update(overrides)
    return request


def test_run_cmd_handles_timeout_bytes(monkeypatch) -> None:
    broker = _load_broker_module()

    def _raise_timeout(*args, **kwargs):
        raise subprocess.TimeoutExpired(
            cmd=["openclaw", "gateway", "health"],
            timeout=8,
            output=b"stdout-token gho_secret",
            stderr=b"stderr-token sk-secret",
        )

    monkeypatch.setattr(broker.subprocess, "run", _raise_timeout)

    result = broker.run_cmd(["openclaw", "gateway", "health"], timeout=8)

    assert result["returncode"] == 124
    assert isinstance(result["stdout"], str)
    assert isinstance(result["stderr"], str)
    assert "gho_<redacted>" in result["stdout"]
    assert "sk-<redacted>" in result["stderr"]
    assert "Timed out after 8s" in result["stderr"]


def test_allowlisted_write_requires_target_environment(monkeypatch) -> None:
    broker = _load_broker_module()
    monkeypatch.setattr(broker, "lookup_capability_lease", lambda request: {"enabled": False, "status": "disabled_missing_identity"})

    decision, reason, audit = broker.evaluate_dispatch_policy(
        _request(
            action="antigravity.config.apply",
            target_surface="operator",
            target_scope="local",
            reason="Apply broker-managed integration files",
            expected_effect="Antigravity integration files updated",
            rollback_hint="Restore previous config snapshot",
            evidence_before="operator export manifest captured",
        )
    )

    assert decision == "deny"
    assert "target_environment" in reason
    assert audit["approval_metadata_missing"] == ["target_environment"]


def test_sensitive_filesystem_write_denied_even_with_approval_metadata(monkeypatch) -> None:
    broker = _load_broker_module()
    monkeypatch.setattr(broker, "lookup_capability_lease", lambda request: {"enabled": False, "status": "disabled_missing_identity"})

    decision, reason, audit = broker.evaluate_dispatch_policy(
        _request(
            reason="Write app config",
            expected_effect="Config updated",
            rollback_hint="Restore backup",
            target_environment="local",
            evidence_before="Config snapshot captured",
        )
    )

    assert decision == "deny"
    assert "sensitive roots" in reason
    assert audit["classification"] == "phase1_sensitive_filesystem_write_default_deny"


def test_network_post_request_denied(monkeypatch) -> None:
    broker = _load_broker_module()
    monkeypatch.setattr(broker, "lookup_capability_lease", lambda request: {"enabled": False, "status": "disabled_missing_identity"})

    decision, reason, audit = broker.evaluate_dispatch_policy(
        _request(
            action="post-request",
            target_surface="network",
            target_scope="https://example.com",
            reason="POST to remote API",
            expected_effect="Mutation sent",
            rollback_hint="Manual compensating action",
            target_environment="prod",
            evidence_before="Request payload reviewed",
        )
    )

    assert decision == "deny"
    assert "outbound network mutation" in reason
    assert audit["network"]["method"] == "POST"


def test_allowlisted_write_with_complete_metadata_requires_approval(monkeypatch) -> None:
    broker = _load_broker_module()
    monkeypatch.setattr(broker, "lookup_capability_lease", lambda request: {"enabled": False, "status": "disabled_missing_identity"})

    decision, reason, audit = broker.evaluate_dispatch_policy(
        _request(
            action="antigravity.config.apply",
            target_surface="operator",
            target_scope="local",
            reason="Apply broker-managed integration files",
            expected_effect="Antigravity integration files updated",
            rollback_hint="Restore previous config snapshot",
            target_environment="local",
            evidence_before="operator export manifest captured",
        )
    )

    assert decision == "approval_required"
    assert reason == "write operation requires approval"
    assert audit["classification"] == "approval_gated_allowlisted_write"
    assert audit["approval_metadata_missing"] == []


def test_incomplete_lease_identity_keeps_hard_deny_path(monkeypatch) -> None:
    broker = _load_broker_module()
    monkeypatch.setattr(broker, "which", lambda binary: "/opt/homebrew/bin/spacetime")

    decision, reason, audit = broker.evaluate_dispatch_policy(
        _request(
            action="write-file",
            arguments={"lease_id": "lease-123"},
            reason="Write app config",
            expected_effect="Config updated",
            rollback_hint="Restore backup",
            target_environment="local",
            evidence_before="Config snapshot captured",
        )
    )

    assert decision == "deny"
    assert "sensitive roots" in reason
    assert audit["lease_lookup"]["enabled"] is False
    assert audit["lease_lookup"]["status"] == "disabled_missing_identity"
    assert "holder_id|proposal_id|owner_id|principal_id" in audit["lease_lookup"]["missing"]
