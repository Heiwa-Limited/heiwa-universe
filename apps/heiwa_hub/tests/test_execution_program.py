# apps/heiwa_hub/tests/test_execution_program.py
"""Tests for ExecutionProgram dataclass."""
import pytest
from heiwa_protocol.program import ExecutionProgram


class TestExecutionProgram:
    def test_minimal_construction(self):
        prog = ExecutionProgram(objective="deploy the hub")
        assert prog.objective == "deploy the hub"
        assert prog.schema_version == 1
        assert prog.source_kind == "compiled_freeform"
        assert prog.steps == []
        assert prog.constraints == {}
        assert prog.scope == {}
        assert prog.tools_allowed == []
        assert prog.budget == {}
        assert prog.acceptance == []
        assert prog.stop_conditions == []
        assert prog.rollback is None
        assert prog.artifacts == []

    def test_full_construction(self):
        prog = ExecutionProgram(
            objective="deploy status page",
            steps=["build", "test", "deploy"],
            constraints={"no_downtime": True},
            scope={"files": ["apps/heiwa_web/"]},
            tools_allowed=["heiwa_ops", "heiwa_claw"],
            budget={"max_turns": 5, "max_seconds": 300},
            acceptance=["health endpoint 200", "tests pass"],
            stop_conditions=["cost > $1", "3 consecutive failures"],
            rollback="revert to previous deploy",
            artifacts=["deploy_log", "health_check_result"],
            source_kind="authored_program",
        )
        assert len(prog.steps) == 3
        assert prog.budget["max_turns"] == 5
        assert prog.rollback == "revert to previous deploy"
        assert prog.source_kind == "authored_program"

    def test_round_trip_serialization(self):
        prog = ExecutionProgram(
            objective="run audit",
            steps=["lint", "test"],
            acceptance=["exit code 0"],
        )
        d = prog.to_dict()
        assert d["objective"] == "run audit"
        assert d["steps"] == ["lint", "test"]

        restored = ExecutionProgram.from_dict(d)
        assert restored.objective == prog.objective
        assert restored.steps == prog.steps
        assert restored.acceptance == prog.acceptance

    def test_from_dict_with_missing_fields(self):
        """Backward compat: partial dicts produce valid programs."""
        prog = ExecutionProgram.from_dict({"objective": "hello"})
        assert prog.objective == "hello"
        assert prog.steps == []
        assert prog.rollback is None

    def test_from_dict_with_none(self):
        prog = ExecutionProgram.from_dict(None)
        assert prog.objective == ""

    def test_is_bounded_true(self):
        prog = ExecutionProgram(
            objective="build feature",
            budget={"max_turns": 10},
            stop_conditions=["test failure"],
        )
        assert prog.is_bounded() is True

    def test_is_bounded_false_when_no_budget_or_stops(self):
        prog = ExecutionProgram(objective="open ended task")
        assert prog.is_bounded() is False

    def test_empty_objective_is_unbounded(self):
        prog = ExecutionProgram(objective="")
        assert prog.is_bounded() is False


class TestBrokerRouteResultProgram:
    """ExecutionProgram as optional field on BrokerRouteResult."""

    def test_route_result_default_no_program(self):
        from heiwa_protocol.routing import BrokerRouteResult
        route = BrokerRouteResult(
            request_id="r1", task_id="t1", envelope_version="2026-03-13",
            raw_text="hello", source_surface="cli", intent_class="chat",
            risk_level="low", privacy_level="local", compute_class=1,
            assigned_worker="", target_tool="heiwa_claw", target_model="",
            target_runtime="railway", target_tier="tier1_local",
            requires_approval=False, rationale="test",
        )
        assert route.execution_program is None

    def test_route_result_with_program(self):
        from heiwa_protocol.routing import BrokerRouteResult
        from heiwa_protocol.program import ExecutionProgram

        prog = ExecutionProgram(objective="deploy", acceptance=["health 200"])
        route = BrokerRouteResult(
            request_id="r1", task_id="t1", envelope_version="2026-03-13",
            raw_text="deploy hub", source_surface="cli", intent_class="deploy",
            risk_level="high", privacy_level="local", compute_class=2,
            assigned_worker="", target_tool="heiwa_ops", target_model="",
            target_runtime="railway", target_tier="tier3_orchestrator",
            requires_approval=True, rationale="test",
            execution_program=prog,
        )
        assert route.execution_program is not None
        assert route.execution_program.objective == "deploy"

    def test_round_trip_via_payload(self):
        from heiwa_protocol.routing import BrokerRouteResult
        from heiwa_protocol.program import ExecutionProgram

        prog = ExecutionProgram(objective="build feature", steps=["code", "test"])
        route = BrokerRouteResult(
            request_id="r1", task_id="t1", envelope_version="2026-03-13",
            raw_text="build", source_surface="cli", intent_class="build",
            risk_level="medium", privacy_level="local", compute_class=2,
            assigned_worker="", target_tool="heiwa_claw", target_model="",
            target_runtime="macbook", target_tier="tier5_heavy_code",
            requires_approval=False, rationale="test",
            execution_program=prog,
        )
        payload = route.to_dict()
        assert isinstance(payload["execution_program"], dict)
        assert payload["execution_program"]["objective"] == "build feature"

        restored = BrokerRouteResult.from_payload(payload)
        assert restored.execution_program is not None
        assert restored.execution_program.steps == ["code", "test"]

    def test_round_trip_without_program(self):
        """Old payloads without execution_program still parse."""
        from heiwa_protocol.routing import BrokerRouteResult
        payload = {"request_id": "r1", "task_id": "t1", "raw_text": "hi"}
        route = BrokerRouteResult.from_payload(payload)
        assert route.execution_program is None
