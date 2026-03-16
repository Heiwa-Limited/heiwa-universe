"""Smoke test: verify the CLI import chain resolves cleanly."""

import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
for pkg in ("heiwa_cli", "heiwa_cognition", "heiwa_sdk", "heiwa_protocol", "heiwa_identity", "heiwa_ui"):
    p = str(ROOT / "packages" / pkg)
    if p not in sys.path:
        sys.path.insert(0, p)
if str(ROOT / "apps") not in sys.path:
    sys.path.insert(0, str(ROOT / "apps"))


def test_cli_context():
    from heiwa_cli.context import CLIContext
    ctx = CLIContext(root=ROOT)
    assert ctx.root == ROOT
    assert ctx.node_id
    assert ctx.session_id


def test_cli_commands():
    from heiwa_cli.commands import get_commands, command_names
    cmds = command_names()
    assert len(cmds) >= 10
    required = {"/help", "/status", "/cells", "/models", "/cost", "/doctor", "/bench",
                "/approve", "/reject", "/trace", "/compact", "/clear"}
    assert required.issubset(set(cmds)), f"Missing: {required - set(cmds)}"


@pytest.mark.asyncio
async def test_enrichment_pipeline():
    from heiwa_cognition.enrichment import BrokerEnrichmentService
    from heiwa_protocol.routing import BrokerRouteRequest
    svc = BrokerEnrichmentService()
    req = BrokerRouteRequest(
        request_id="test-1",
        task_id="test-task-1",
        raw_text="deploy the status page",
        sender_id="test-node",
        source_surface="cli",
    )
    result = await svc.enrich(req)
    assert result.intent_class
    assert result.risk_level
    assert result.target_tool


def test_identity_selector():
    from heiwa_cognition.identity import IdentitySelector
    sel = IdentitySelector(ROOT / "config" / "identities" / "profiles.json")
    identities = sel.all_identities()
    assert len(identities) >= 3
    for identity in identities:
        assert identity.id
        assert len(identity.cells) >= 1


def test_gateway_resolve():
    from heiwa_sdk.heiwaclaw import HeiwaClawGateway
    from heiwa_protocol.routing import BrokerRouteResult
    gw = HeiwaClawGateway(ROOT)
    route = BrokerRouteResult(
        request_id="test-1",
        task_id="test-task-1",
        raw_text="hello world",
        source_surface="cli",
        intent_class="chat",
        risk_level="low",
        compute_class=1,
        assigned_worker="node_a_orchestrator",
        target_tool="heiwa_claw",
        target_model="ollama/llama-4-scout:q4_k_m",
        target_runtime="macbook",
        target_tier="tier1_local",
        envelope_version="1.0",
        privacy_level="standard",
        requires_approval=False,
        rationale="test",
    )
    dispatch = gw.resolve(route)
    assert dispatch.provider
    assert dispatch.rate_group
    assert dispatch.adapter_tool


def test_rate_ledger():
    from heiwa_sdk.rate_ledger import RateGroupLedger
    ledger = RateGroupLedger(ROOT / "config" / "swarm" / "ai_router.json")
    assert ledger.has_capacity("local_ollama")
    assert ledger.has_capacity("claude_code")
    assert ledger.remaining("local_ollama") is None  # unlimited
    assert ledger.remaining("claude_code") == 40


def test_operator_surface():
    from heiwa_sdk.operator_surface import maybe_fast_path_turn, operator_display_name
    fast = maybe_fast_path_turn("hello", "macbook@heiwa-agile")
    assert fast is not None
    assert "Devon" in fast.response
    assert maybe_fast_path_turn("deploy the hub", "test") is None
    assert operator_display_name("devon@heiwa") == "Devon"


def test_approval_inline_imports():
    from heiwa_cli.approval_inline import InlineApprovalResult, request_inline_approval
    result = InlineApprovalResult(status="approved")
    assert result.status == "approved"


def test_auth_imports():
    from heiwa_cli.auth import ProviderAuthManager, SUPPORTED_PROVIDERS
    assert len(SUPPORTED_PROVIDERS) >= 4


if __name__ == "__main__":
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    passed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as exc:
            print(f"FAIL {test.__name__}: {exc}")
    print(f"CLI import chain test: {passed}/{len(tests)} passed {'PASSED' if passed == len(tests) else 'FAILED'}")
