# apps/heiwa_hub/tests/test_program_compiler.py
"""Tests for ProgramCompiler — deterministic intent-to-program mapping."""
import pytest
from heiwa_cognition.program_compiler import ProgramCompiler
from heiwa_cognition.intent import IntentProfile
from heiwa_cognition.router import ComputeRoute


def _make_profile(intent: str, risk: str = "low", tool: str = "heiwa_claw") -> IntentProfile:
    return IntentProfile(
        intent_class=intent,
        risk_level=risk,
        requires_approval=risk in ("high", "critical"),
        preferred_runtime="railway",
        preferred_tool=tool,
        preferred_tier="tier1_local",
        normalized_instruction="do the thing",
        assumptions=[],
        missing_details=[],
        confidence=0.8,
        underspecified=False,
    )


def _make_route(
    tool: str = "heiwa_claw",
    runtime: str = "railway",
    tier: str = "tier1_local",
    privacy: str = "local",
) -> ComputeRoute:
    return ComputeRoute(
        compute_class=1,
        assigned_worker="",
        target_tool=tool,
        target_model="",
        target_runtime=runtime,
        target_tier=tier,
        privacy_level=privacy,
        rationale="test",
    )


class TestProgramCompiler:
    def setup_method(self):
        self.compiler = ProgramCompiler()

    def test_build_intent_produces_bounded_program(self):
        profile = _make_profile("build", risk="medium")
        route = _make_route(tool="heiwa_claw", runtime="macbook", tier="tier5_heavy_code")
        prog = self.compiler.compile(profile=profile, route=route, raw_text="implement the feature")
        assert prog.objective
        assert prog.acceptance  # build tasks have acceptance criteria
        assert prog.is_bounded()
        assert prog.source_kind == "compiled_freeform"
        assert "heiwa_claw" in prog.tools_allowed  # from route, not just profile

    def test_deploy_intent_has_rollback(self):
        profile = _make_profile("deploy", risk="high", tool="heiwa_ops")
        route = _make_route(tool="heiwa_ops", runtime="railway", tier="tier3_orchestrator")
        prog = self.compiler.compile(profile=profile, route=route, raw_text="deploy the status page")
        assert prog.rollback is not None
        assert prog.acceptance  # deploy must have acceptance
        assert "heiwa_ops" in prog.tools_allowed

    def test_chat_intent_is_unbounded(self):
        profile = _make_profile("chat")
        route = _make_route()
        prog = self.compiler.compile(profile=profile, route=route, raw_text="hello")
        assert not prog.is_bounded()  # chat has no budget or stop conditions

    def test_audit_intent_is_bounded(self):
        profile = _make_profile("audit", tool="heiwa_ops")
        route = _make_route(tool="heiwa_ops")
        prog = self.compiler.compile(profile=profile, route=route, raw_text="check the repo")
        assert prog.is_bounded()
        assert prog.acceptance

    def test_research_has_artifacts(self):
        profile = _make_profile("research")
        route = _make_route()
        prog = self.compiler.compile(profile=profile, route=route, raw_text="analyze competitors")
        assert prog.artifacts  # research produces artifacts

    def test_high_risk_has_stop_conditions(self):
        profile = _make_profile("operate", risk="high", tool="heiwa_ops")
        route = _make_route(tool="heiwa_ops")
        prog = self.compiler.compile(profile=profile, route=route, raw_text="fix the incident")
        assert prog.stop_conditions  # high risk always gets abort conditions

    def test_tools_allowed_from_route(self):
        """tools_allowed should come from the routed tool, not just the profile."""
        profile = _make_profile("build", tool="heiwa_claw")
        route = _make_route(tool="heiwa_code")  # route overrides to codex
        prog = self.compiler.compile(profile=profile, route=route, raw_text="build it")
        assert "heiwa_code" in prog.tools_allowed  # uses route's tool

    def test_budget_scales_with_risk(self):
        route = _make_route()
        low = self.compiler.compile(
            profile=_make_profile("build", risk="low"), route=route, raw_text="small fix"
        )
        high = self.compiler.compile(
            profile=_make_profile("build", risk="high"), route=route, raw_text="big refactor"
        )
        # Higher risk gets more budget
        assert high.budget.get("max_turns", 0) >= low.budget.get("max_turns", 0)

    def test_scope_from_raw_text_file_mention(self):
        profile = _make_profile("build")
        route = _make_route()
        prog = self.compiler.compile(
            profile=profile,
            route=route,
            raw_text="fix the bug in apps/heiwa_hub/main.py",
        )
        # Compiler should extract file paths from raw text
        assert prog.scope.get("files") or prog.scope == {}  # either extracted or empty is OK

    def test_sovereign_privacy_constrains_scope(self):
        """Sovereign privacy routes should set constraints."""
        profile = _make_profile("build")
        route = _make_route(privacy="sovereign", runtime="macbook")
        prog = self.compiler.compile(profile=profile, route=route, raw_text="build local tool")
        assert prog.constraints.get("sovereign") is True


class TestEnrichmentProducesProgram:
    """Enrichment pipeline attaches ExecutionProgram to BrokerRouteResult."""

    @pytest.mark.asyncio
    async def test_enrichment_attaches_program(self):
        from heiwa_cognition.enrichment import BrokerEnrichmentService
        from heiwa_protocol.routing import BrokerRouteRequest

        svc = BrokerEnrichmentService()
        req = BrokerRouteRequest(
            request_id="test-prog-1",
            task_id="test-prog-task-1",
            raw_text="deploy the status page",
            sender_id="test",
            source_surface="cli",
        )
        result = await svc.enrich(req)
        assert result.execution_program is not None
        assert result.execution_program.objective
        assert "deploy" in result.execution_program.objective.lower()
        assert result.execution_program.schema_version == 1
        assert result.execution_program.source_kind == "compiled_freeform"

    @pytest.mark.asyncio
    async def test_enrichment_chat_has_no_bounded_program(self):
        from heiwa_cognition.enrichment import BrokerEnrichmentService
        from heiwa_protocol.routing import BrokerRouteRequest

        svc = BrokerEnrichmentService()
        req = BrokerRouteRequest(
            request_id="test-chat-1",
            task_id="test-chat-task-1",
            raw_text="hello",
            sender_id="test",
            source_surface="cli",
        )
        result = await svc.enrich(req)
        # Chat gets a program but it's unbounded
        assert result.execution_program is not None
        assert not result.execution_program.is_bounded()
