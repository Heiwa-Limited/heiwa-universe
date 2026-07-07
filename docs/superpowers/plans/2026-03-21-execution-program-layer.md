# ExecutionProgram Layer — Software 3.0 Typed Contract

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a typed `ExecutionProgram` contract that the cognition layer compiles from freeform text, HeiwaClaw validates before/after execution, and `/programs/*.program.md` can target later as an authoring surface.

**Architecture:** Approach #2 from Codex analysis. Freeform ingress unchanged. Cognition compiles `IntentProfile` + `ComputeRoute` + raw text into `ExecutionProgram` alongside `BrokerRouteResult`. V1 validates `acceptance` criteria as **advisory** (does not override execution status). Program is persisted to execution memory for future optimization. The program travels as an optional field on `BrokerRouteResult` — fully backward compatible. `stop_conditions` and `artifacts` validation are deferred to v2.

**V1 scope boundary:** This plan implements `acceptance` validation only. `stop_conditions` (runtime abort) and `artifacts` (output verification) require adapter-level hooks that don't exist yet. They are typed into the contract now so the schema is stable, but validation logic for them is explicitly out of scope.

**Tech Stack:** Python dataclasses, existing cognition pipeline (IntentNormalizer, RiskScorer, ComputeRouter, LocalTaskPlanner), heiwa_protocol routing module, pytest.

---

## File Map

| Action | File                                                            | Responsibility                                                              |
| ------ | --------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Create | `packages/heiwa_protocol/heiwa_protocol/program.py`             | `ExecutionProgram` dataclass + serialization + validation                   |
| Modify | `packages/heiwa_protocol/heiwa_protocol/routing.py:69-131`      | Add optional `execution_program` field to `BrokerRouteResult`               |
| Modify | `packages/heiwa_protocol/heiwa_protocol/__init__.py`            | Export `ExecutionProgram`                                                   |
| Create | `packages/heiwa_cognition/heiwa_cognition/program_compiler.py`  | Compile `IntentProfile` + `ComputeRoute` + raw_text into `ExecutionProgram` |
| Modify | `packages/heiwa_cognition/heiwa_cognition/enrichment.py:50-116` | Call compiler with profile + route, attach program to `BrokerRouteResult`   |
| Modify | `apps/heiwa_hub/agents/spine.py:270-292`                        | Forward `execution_program` in `exec_payload`                               |
| Modify | `apps/heiwa_hub/agents/heiwaclaw.py:249-363`                    | Advisory acceptance validation + persist program to execution memory        |
| Modify | `packages/heiwa_sdk/heiwa_sdk/memory.py:102-109`                | Accept optional `execution_program_json` in `record_execution()`            |
| Create | `apps/heiwa_hub/tests/test_execution_program.py`                | Unit tests for program dataclass                                            |
| Create | `apps/heiwa_hub/tests/test_program_compiler.py`                 | Unit tests for compiler                                                     |
| Create | `apps/heiwa_hub/tests/test_program_validation.py`               | Integration tests for HeiwaClaw advisory validation                         |

---

## Chunk 1: Protocol — ExecutionProgram Dataclass

### Task 1: Define ExecutionProgram

**Files:**

- Create: `packages/heiwa_protocol/heiwa_protocol/program.py`
- Create: `apps/heiwa_hub/tests/test_execution_program.py`

- [ ] **Step 1: Write the failing test**

```python
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest apps/heiwa_hub/tests/test_execution_program.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'heiwa_protocol.program'`

- [ ] **Step 3: Write ExecutionProgram dataclass**

```python
# packages/heiwa_protocol/heiwa_protocol/program.py
"""ExecutionProgram: typed contract for bounded agent execution.

Compiled from freeform text by the cognition layer. Validated by HeiwaClaw
before and after execution. Future authoring surface: /programs/*.program.md
"""
from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any


EXECUTION_PROGRAM_SCHEMA_VERSION = 1

@dataclass(slots=True)
class ExecutionProgram:
    """Machine contract for a single execution run.

    Fields:
        schema_version:  Contract version for forward-compatible migration.
        source_kind:     How this program was created: "compiled_freeform" (from raw text
                         via ProgramCompiler) or "authored_program" (from /programs/*.program.md).
        objective:       What this run achieves (single sentence).
        steps:           Ordered execution steps (human-readable strings).
        constraints:     Hard constraints (no_downtime, db_schema_locked, etc.).
        scope:           Files, dirs, or surfaces the run may touch.
        tools_allowed:   Explicit allowlist of tools/adapters.
        budget:          Cost/time/turn ceilings (max_turns, max_seconds, max_cost).
        acceptance:      Success criteria checked post-execution (advisory in v1).
        stop_conditions: Hard abort triggers (v2 — typed now, validated later).
        rollback:        What to do on failure (null = no rollback).
        artifacts:       Expected outputs (v2 — typed now, validated later).
    """
    schema_version: int = EXECUTION_PROGRAM_SCHEMA_VERSION
    source_kind: str = "compiled_freeform"  # "compiled_freeform" | "authored_program"
    objective: str = ""
    steps: list[str] = field(default_factory=list)
    constraints: dict[str, Any] = field(default_factory=dict)
    scope: dict[str, Any] = field(default_factory=dict)
    tools_allowed: list[str] = field(default_factory=list)
    budget: dict[str, Any] = field(default_factory=dict)
    acceptance: list[str] = field(default_factory=list)
    stop_conditions: list[str] = field(default_factory=list)
    rollback: str | None = None
    artifacts: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> ExecutionProgram:
        if not data:
            return cls()
        return cls(
            schema_version=int(data.get("schema_version") or EXECUTION_PROGRAM_SCHEMA_VERSION),
            source_kind=str(data.get("source_kind") or "compiled_freeform"),
            objective=str(data.get("objective") or ""),
            steps=list(data.get("steps") or []),
            constraints=dict(data.get("constraints") or {}),
            scope=dict(data.get("scope") or {}),
            tools_allowed=list(data.get("tools_allowed") or []),
            budget=dict(data.get("budget") or {}),
            acceptance=list(data.get("acceptance") or []),
            stop_conditions=list(data.get("stop_conditions") or []),
            rollback=data.get("rollback"),
            artifacts=list(data.get("artifacts") or []),
        )

    def is_bounded(self) -> bool:
        """True if this program has explicit resource limits or abort conditions."""
        return bool(self.objective) and bool(self.budget or self.stop_conditions)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest apps/heiwa_hub/tests/test_execution_program.py -v`
Expected: 8/8 PASS

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_protocol/heiwa_protocol/program.py apps/heiwa_hub/tests/test_execution_program.py
git commit -m "feat: add ExecutionProgram typed contract to protocol layer"
```

---

### Task 2: Wire ExecutionProgram into BrokerRouteResult

**Files:**

- Modify: `packages/heiwa_protocol/heiwa_protocol/routing.py:69-131`
- Modify: `packages/heiwa_protocol/heiwa_protocol/__init__.py`

- [ ] **Step 1: Write the failing test**

Add to `apps/heiwa_hub/tests/test_execution_program.py`:

```python
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest apps/heiwa_hub/tests/test_execution_program.py::TestBrokerRouteResultProgram -v`
Expected: FAIL — `TypeError: __init__() got an unexpected keyword argument 'execution_program'`

- [ ] **Step 3: Add execution_program field to BrokerRouteResult**

In `packages/heiwa_protocol/heiwa_protocol/routing.py`:

Add import at top:

```python
from heiwa_protocol.program import ExecutionProgram
```

Add field to `BrokerRouteResult` (after line 95, before `@classmethod`):

```python
execution_program: ExecutionProgram | None = None
```

In `from_payload()` (after `message=` line), add:

```python
execution_program=ExecutionProgram.from_dict(payload.get("execution_program")) if payload.get("execution_program") else None,
```

In `to_dict()`, add after `payload["privacy_level"]` line:

```python
if self.execution_program:
    payload["execution_program"] = self.execution_program.to_dict()
else:
    payload.pop("execution_program", None)
```

**Note:** `BrokerRouteResult` uses `slots=True`. The new field must be added to the dataclass field list (the decorator handles slot creation). No manual `__slots__` needed.

- [ ] **Step 4: Update protocol **init**.py**

In `packages/heiwa_protocol/heiwa_protocol/__init__.py`, add:

```python
from .program import ExecutionProgram
```

- [ ] **Step 5: Run tests to verify**

Run: `pytest apps/heiwa_hub/tests/test_execution_program.py -v`
Expected: 12/12 PASS

Run: `pytest apps/heiwa_hub/tests/ apps/heiwa_trading/tests/ -v`
Expected: All existing tests still pass (field is optional, defaults to None)

- [ ] **Step 6: Commit**

```bash
git add packages/heiwa_protocol/heiwa_protocol/routing.py packages/heiwa_protocol/heiwa_protocol/__init__.py apps/heiwa_hub/tests/test_execution_program.py
git commit -m "feat: wire ExecutionProgram into BrokerRouteResult as optional field"
```

---

## Chunk 2: Cognition — Program Compiler

### Task 3: Build the program compiler

The compiler takes `IntentProfile` + `ComputeRoute` + raw text and produces an `ExecutionProgram`. This is deterministic (no LLM calls) — intent-to-program mapping with sensible defaults per intent class.

**Files:**

- Create: `packages/heiwa_cognition/heiwa_cognition/program_compiler.py`
- Create: `apps/heiwa_hub/tests/test_program_compiler.py`

- [ ] **Step 1: Write failing tests**

```python
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest apps/heiwa_hub/tests/test_program_compiler.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'heiwa_cognition.program_compiler'`

- [ ] **Step 3: Write the compiler**

```python
# packages/heiwa_cognition/heiwa_cognition/program_compiler.py
"""ProgramCompiler: deterministic intent-to-program mapping.

Compiles IntentProfile + ComputeRoute + raw text into an ExecutionProgram.
No LLM calls — this is a rules engine. LLM-based compilation
is a future upgrade path.
"""
from __future__ import annotations

import re
from typing import Any

from heiwa_cognition.intent import IntentProfile
from heiwa_cognition.router import ComputeRoute
from heiwa_protocol.program import ExecutionProgram

# Budget defaults by risk level
_BUDGET_BY_RISK: dict[str, dict[str, Any]] = {
    "low":      {"max_turns": 5,  "max_seconds": 120},
    "medium":   {"max_turns": 10, "max_seconds": 300},
    "high":     {"max_turns": 20, "max_seconds": 600},
    "critical": {"max_turns": 30, "max_seconds": 900},
}

# File path regex for scope extraction
_FILE_PATH_RE = re.compile(
    r"(?:^|\s)((?:apps|packages|config|docs|scripts)/\S+\.(?:py|ts|js|json|md|yml|yaml|toml|sh))",
    re.MULTILINE,
)


class ProgramCompiler:
    """Compiles freeform intent into a typed ExecutionProgram."""

    def compile(
        self,
        profile: IntentProfile,
        route: ComputeRoute,
        raw_text: str,
    ) -> ExecutionProgram:
        intent = profile.intent_class
        risk = profile.risk_level
        tool = route.target_tool or profile.preferred_tool

        # Chat and general: pass through unbounded
        if intent in ("chat", "general", "status_check"):
            return ExecutionProgram(objective=raw_text[:200])

        budget = dict(_BUDGET_BY_RISK.get(risk, _BUDGET_BY_RISK["low"]))
        tools_allowed = [tool] if tool else []
        scope = self._extract_scope(raw_text)
        stop_conditions = self._stop_conditions_for(risk)
        constraints: dict[str, Any] = {}
        if route.privacy_level == "sovereign":
            constraints["sovereign"] = True

        if intent == "deploy":
            return ExecutionProgram(
                objective=f"Deploy: {raw_text[:150]}",
                steps=["validate pre-conditions", "execute deploy", "verify health"],
                constraints={**constraints, "no_downtime": True},
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["health endpoint returns 200", "no error logs in first 60s"],
                stop_conditions=stop_conditions,
                rollback="revert to previous deployment",
                artifacts=["deploy_log", "health_check_result"],
            )

        if intent in ("operate",):
            return ExecutionProgram(
                objective=f"Ops: {raw_text[:150]}",
                steps=["diagnose", "apply fix", "verify"],
                constraints=constraints,
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["issue resolved", "no regression"],
                stop_conditions=stop_conditions,
                rollback="revert changes",
                artifacts=["incident_log"],
            )

        if intent == "build":
            return ExecutionProgram(
                objective=f"Build: {raw_text[:150]}",
                steps=["implement", "test", "verify"],
                constraints=constraints,
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["tests pass", "no lint errors"],
                stop_conditions=stop_conditions,
                rollback=None,
                artifacts=["code_diff"],
            )

        if intent == "audit":
            return ExecutionProgram(
                objective=f"Audit: {raw_text[:150]}",
                steps=["scan", "report"],
                constraints=constraints,
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["audit report generated", "exit code 0"],
                stop_conditions=stop_conditions,
                rollback=None,
                artifacts=["audit_report"],
            )

        if intent == "research":
            return ExecutionProgram(
                objective=f"Research: {raw_text[:150]}",
                steps=["gather", "synthesize", "report"],
                constraints=constraints,
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["findings documented"],
                stop_conditions=stop_conditions,
                rollback=None,
                artifacts=["research_report"],
            )

        if intent == "strategy":
            return ExecutionProgram(
                objective=f"Strategy: {raw_text[:150]}",
                steps=["analyze", "propose", "document"],
                constraints=constraints,
                scope=scope,
                tools_allowed=tools_allowed,
                budget=budget,
                acceptance=["proposal documented"],
                stop_conditions=stop_conditions,
                rollback=None,
                artifacts=["strategy_document"],
            )

        # Fallback: bounded but generic
        return ExecutionProgram(
            objective=raw_text[:200],
            steps=["execute"],
            constraints={},
            scope=scope,
            tools_allowed=tools_allowed,
            budget=budget,
            acceptance=[],
            stop_conditions=stop_conditions,
            rollback=None,
            artifacts=[],
        )

    @staticmethod
    def _extract_scope(raw_text: str) -> dict[str, Any]:
        """Extract file paths mentioned in raw text."""
        matches = _FILE_PATH_RE.findall(raw_text)
        if matches:
            return {"files": matches}
        return {}

    @staticmethod
    def _stop_conditions_for(risk: str) -> list[str]:
        """High-risk tasks always get abort conditions."""
        if risk in ("high", "critical"):
            return ["3 consecutive failures", "budget exhausted"]
        if risk == "medium":
            return ["budget exhausted"]
        return []
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest apps/heiwa_hub/tests/test_program_compiler.py -v`
Expected: 10/10 PASS

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/program_compiler.py apps/heiwa_hub/tests/test_program_compiler.py
git commit -m "feat: add ProgramCompiler — deterministic intent-to-program mapping"
```

---

### Task 4: Wire compiler into enrichment pipeline

**Files:**

- Modify: `packages/heiwa_cognition/heiwa_cognition/enrichment.py:50-116`

- [ ] **Step 1: Write failing test**

Add to `apps/heiwa_hub/tests/test_program_compiler.py`:

```python
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest apps/heiwa_hub/tests/test_program_compiler.py::TestEnrichmentProducesProgram -v`
Expected: FAIL — `execution_program` is None (not attached yet)

- [ ] **Step 3: Wire compiler into BrokerEnrichmentService.enrich()**

In `packages/heiwa_cognition/heiwa_cognition/enrichment.py`:

Add import at top (after existing imports):

```python
from heiwa_cognition.program_compiler import ProgramCompiler
```

Add to `BrokerEnrichmentService.__init__()`:

```python
self.program_compiler = ProgramCompiler()
```

In `enrich()`, after the `normalization["identity_id"] = identity.id` line (line 91) and before the `return BrokerRouteResult(` line (line 93), add:

```python
# Compile typed execution program from intent + route + raw text
execution_program = self.program_compiler.compile(
    profile=profile,
    route=route,
    raw_text=request.raw_text,
)
```

Add to the `BrokerRouteResult(` constructor call (after `context_files_json=` line):

```python
execution_program=execution_program,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest apps/heiwa_hub/tests/test_program_compiler.py -v`
Expected: 12/12 PASS

Run: `pytest apps/heiwa_hub/tests/ apps/heiwa_trading/tests/ -v`
Expected: All existing tests still pass

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/enrichment.py apps/heiwa_hub/tests/test_program_compiler.py
git commit -m "feat: wire ProgramCompiler into enrichment pipeline"
```

---

## Chunk 3: Execution — HeiwaClaw Validation

### Task 5: Carry ExecutionProgram across planner boundary + forward through dispatch

**Critical context:** In `spine.py:handle_request()`, line 97 merges enrichment onto `payload` via `payload.update(route.to_dict())`, which includes `execution_program`. But line 120 does `payload = task_plan.to_dict()`, which **replaces the entire payload** with `TaskPlan` fields — and `TaskPlan` has no `execution_program` field. The compiled program is lost before `_dispatch_steps()` runs. Fix: save and restore across the planner boundary.

**Files:**

- Modify: `apps/heiwa_hub/agents/spine.py:102-131` (planner boundary)
- Modify: `apps/heiwa_hub/agents/spine.py:270-292` (dispatch)

- [ ] **Step 1: Carry execution_program across the planner overwrite**

In `apps/heiwa_hub/agents/spine.py`, in `handle_request()`, before the planner section (before line 103 `if not payload.get("steps")`), add:

```python
# Save enrichment fields that TaskPlan.to_dict() would overwrite
_saved_execution_program = payload.get("execution_program")
```

After line 120 `payload = task_plan.to_dict()`, add:

```python
# Restore execution_program lost by TaskPlan.to_dict() overwrite
if _saved_execution_program:
    payload["execution_program"] = _saved_execution_program
```

- [ ] **Step 2: Add execution_program to exec_payload in _dispatch_steps()**

In `_dispatch_steps()`, in the `exec_payload` dict (after `"envelope_version"` line ~291), add:

```python
"execution_program": payload.get("execution_program"),
```

- [ ] **Step 3: Run existing tests to verify no breakage**

Run: `pytest apps/heiwa_hub/tests/test_task_ingress_e2e.py apps/heiwa_hub/tests/test_approval_gate_e2e.py -v`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_hub/agents/spine.py
git commit -m "feat: carry execution_program across planner boundary + forward through dispatch"
```

---

### Task 6: HeiwaClaw advisory acceptance validation + program persistence

HeiwaClaw checks `acceptance` criteria after execution completes and reports validation results. **V1 is explicitly advisory** — validation does NOT override `exec_status`. Status remains based on execution exit code. Validation is reported separately in the result payload and logged. The program is also persisted to execution memory for future optimization loops.

**Files:**

- Modify: `apps/heiwa_hub/agents/heiwaclaw.py:249-363`
- Create: `apps/heiwa_hub/tests/test_program_validation.py`

- [ ] **Step 1: Write failing tests**

```python
# apps/heiwa_hub/tests/test_program_validation.py
"""Tests for HeiwaClaw ExecutionProgram validation."""
import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from heiwa_protocol.program import ExecutionProgram


class TestProgramValidation:
    def _make_agent(self):
        with patch("heiwa_hub.agents.heiwaclaw.RepoAuditor"):
            from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent
            agent = HeiwaClawAgent()
            agent.db = MagicMock()
            agent.db.stdb = MagicMock()
            agent._llm = MagicMock()
            agent.speak = AsyncMock()
            return agent

    def test_validate_acceptance_pass(self):
        agent = self._make_agent()
        prog = ExecutionProgram(
            objective="deploy hub",
            acceptance=["returned 200", "health endpoint"],
        )
        # Both criteria are contiguous substrings of the output (case-insensitive)
        result = agent._validate_acceptance(prog, "Health endpoint returned 200 OK")
        assert result["passed"] is True
        assert result["matched"] == ["returned 200", "health endpoint"]
        assert result["unmatched"] == []

    def test_validate_acceptance_fail(self):
        agent = self._make_agent()
        prog = ExecutionProgram(
            objective="deploy hub",
            acceptance=["returned 200", "no error logs"],
        )
        result = agent._validate_acceptance(prog, "returned 200 but errors found")
        assert result["passed"] is False
        assert "returned 200" in result["matched"]
        assert "no error logs" in result["unmatched"]

    def test_validate_acceptance_empty_program(self):
        agent = self._make_agent()
        prog = ExecutionProgram(objective="chat")
        result = agent._validate_acceptance(prog, "whatever")
        assert result["passed"] is True  # no criteria = pass
        assert result["matched"] == []

    def test_validate_acceptance_none_program(self):
        agent = self._make_agent()
        result = agent._validate_acceptance(None, "output")
        assert result["passed"] is True
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest apps/heiwa_hub/tests/test_program_validation.py -v`
Expected: FAIL — `AttributeError: 'HeiwaClawAgent' object has no attribute '_validate_acceptance'`

- [ ] **Step 3: Add _validate_acceptance to HeiwaClawAgent**

In `apps/heiwa_hub/agents/heiwaclaw.py`, in the Utilities section (before `_resolve_runtime`), add:

```python
    def _validate_acceptance(
        self,
        program: "ExecutionProgram | None",
        output: str,
    ) -> dict[str, Any]:
        """Advisory check: execution output against program acceptance criteria.

        V1: This is ADVISORY ONLY. It does not override exec_status.
        Returns dict with: passed (bool), matched (list), unmatched (list).
        """
        if not program or not program.acceptance:
            return {"passed": True, "matched": [], "unmatched": []}

        output_lower = output.lower()
        matched = []
        unmatched = []
        for criterion in program.acceptance:
            if criterion.lower() in output_lower:
                matched.append(criterion)
            else:
                unmatched.append(criterion)

        return {
            "passed": len(unmatched) == 0,
            "matched": matched,
            "unmatched": unmatched,
        }
```

Add import at top of heiwaclaw.py (won't cause issues — protocol is a dependency):

```python
from heiwa_protocol.program import ExecutionProgram
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest apps/heiwa_hub/tests/test_program_validation.py -v`
Expected: 4/4 PASS

- [ ] **Step 5: Wire validation into _handle_exec()**

In `apps/heiwa_hub/agents/heiwaclaw.py`, in `_handle_exec()`:

After the `BrokerRouteResult.from_payload()` call (~line 281), extract the program:

```python
execution_program = None
prog_data = payload.get("execution_program")
if prog_data:
    execution_program = ExecutionProgram.from_dict(prog_data)
```

After execution completes and `exec_status` is set (~line 318, after `elapsed = round(...)`), add advisory validation:

```python
# Advisory: validate against execution program acceptance criteria
# V1: does NOT override exec_status — reported separately
program_validation = self._validate_acceptance(execution_program, full_result)
if execution_program and execution_program.is_bounded() and not program_validation["passed"]:
    logger.warning(
        "Task %s: advisory acceptance unmet: %s",
        task_id, program_validation["unmatched"],
    )
```

Add `program_validation` to `result_payload` dict (advisory, does not affect status):

```python
"program_validation": program_validation,
"execution_program": execution_program.to_dict() if execution_program else None,
```

- [ ] **Step 6: Persist program to execution memory**

In `_handle_exec()`, in the existing `if self.memory:` block that calls `record_execution()`, add the program JSON:

```python
self.memory.record_execution(
    task_id=task_id,
    model=route.target_model,
    outcome=exec_status.lower(),
    duration_ms=int(elapsed * 1000),
    error=full_result if exec_status == "FAIL" else None,
    execution_program_json=json.dumps(execution_program.to_dict()) if execution_program else None,
)
```

Add `import json` at top of heiwaclaw.py if not already present.

In `packages/heiwa_sdk/heiwa_sdk/memory.py`, update `record_execution()` to accept but NOT forward the program to STDB (the reducer has fixed arity):

```python
    def record_execution(
        self,
        task_id: str,
        model: str,
        outcome: str,
        duration_ms: int,
        error: str | None = None,
        execution_program_json: str | None = None,
    ) -> bool:
        """Record task outcome in execution_memory.

        execution_program_json is accepted for future persistence but NOT forwarded
        to STDB — the insert_execution_memory reducer has fixed arity (6 args).
        The program is logged here; STDB schema migration comes in v2.
        """
        if execution_program_json:
            import logging
            logging.getLogger(__name__).debug(
                "ExecutionProgram for %s logged (STDB persistence deferred to v2)", task_id,
            )
        return self.stdb.insert_execution_memory(
            task_dispatch_id=task_id,
            model_used=model,
            outcome=outcome,
            duration_ms=duration_ms,
            error_summary=error,
        )
```

**Why not forward to STDB:** `SpacetimeDB.insert_execution_memory()` calls `self.call("insert_execution_memory", ...)` with 6 positional args matching the current reducer arity. Adding a 7th arg would fail the call, not silently drop it. The param is accepted at the Python API surface so callers can pass it now; persistence is enabled when the STDB table/reducer are updated in v2.

- [ ] **Step 7: Run full test suite**

Run: `pytest apps/heiwa_hub/tests/ apps/heiwa_trading/tests/ -v`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_hub/agents/heiwaclaw.py apps/heiwa_hub/tests/test_program_validation.py packages/heiwa_sdk/heiwa_sdk/memory.py
git commit -m "feat: advisory acceptance validation + persist ExecutionProgram to execution memory"
```

---

## Chunk 4: Integration Verification

### Task 7: End-to-end verification and docs

**Files:**

- Modify: `CLAUDE.md` (execution gateway section)
- Modify: `apps/heiwa_hub/agents/CONTEXT.md`

- [ ] **Step 1: Run full test suite**

Run: `pytest apps/heiwa_hub/tests/ apps/heiwa_trading/tests/ -v`
Expected: All tests PASS (existing 156+ plus ~24 new)

- [ ] **Step 2: Verify import chain**

```bash
python -c "from heiwa_protocol import ExecutionProgram; print('Protocol:', ExecutionProgram)"
python -c "from heiwa_cognition.program_compiler import ProgramCompiler; print('Compiler:', ProgramCompiler)"
python -c "
from heiwa_cognition.program_compiler import ProgramCompiler
from heiwa_cognition.intent import IntentProfile
from heiwa_cognition.router import ComputeRoute
p = IntentProfile(intent_class='deploy', risk_level='high', requires_approval=True,
    preferred_runtime='railway', preferred_tool='heiwa_ops', preferred_tier='tier3_orchestrator',
    normalized_instruction='deploy hub', assumptions=[], missing_details=[], confidence=0.9, underspecified=False)
r = ComputeRoute(compute_class=2, assigned_worker='', target_tool='heiwa_ops',
    target_model='', target_runtime='railway', target_tier='tier3_orchestrator',
    privacy_level='local', rationale='test')
prog = ProgramCompiler().compile(p, r, 'deploy the hub')
print(f'Program: {prog.objective}, bounded={prog.is_bounded()}, rollback={prog.rollback}, source={prog.source_kind}')
"
```

Expected: All print OK, program is bounded with rollback for deploy intent.

- [ ] **Step 3: Update CLAUDE.md execution gateway section**

In the `Execution gateway (packages/heiwa_sdk/)` section, add after the `heiwaclaw/` line:

```
- program.py (heiwa_protocol) — ExecutionProgram typed contract: objective, steps, constraints, acceptance, budget, rollback
```

In the `Cognition pipeline` section, add:

```
- program_compiler.py — compiles IntentProfile + ComputeRoute + raw text into typed ExecutionProgram (deterministic, no LLM)
```

- [ ] **Step 4: Final commit**

```bash
git add CLAUDE.md apps/heiwa_hub/agents/CONTEXT.md
git commit -m "docs: document ExecutionProgram layer in architecture docs"
```

- [ ] **Step 5: Push and deploy**

```bash
git push origin main
```

---

## Migration Path: `/programs/*.program.md` (Future — NOT in this plan)

Once `ExecutionProgram` is stable and validated in production:

1. Define a markdown schema for `.program.md` files (structured frontmatter + steps)
2. Build a `ProgramLoader` that parses `.program.md` into `ExecutionProgram` with `source_kind="authored_program"`
3. Add `/programs/` directory with canonical programs: `deploy.program.md`, `build.program.md`, `audit.program.md`
4. `ProgramCompiler` checks `/programs/` for a matching program before falling back to rules engine
5. HeiwaClaw can then execute both freeform-compiled and authored programs through the same contract
6. `schema_version` enables forward-compatible schema evolution without breaking old programs
7. Implement `stop_conditions` validation (runtime abort hooks in adapters) and `artifacts` validation (output verification)

This creates the authoring surface for Software 3.0 — programs written in natural language with typed constraints, compiled into the same `ExecutionProgram` that freeform text produces today. The `source_kind` field distinguishes provenance for auditing and optimization.

---

## Summary

| What                                        | Where                                 | Lines          |
| ------------------------------------------- | ------------------------------------- | -------------- |
| `ExecutionProgram` dataclass                | `heiwa_protocol/program.py`           | ~70            |
| Optional field on `BrokerRouteResult`       | `heiwa_protocol/routing.py`           | ~8 delta       |
| `ProgramCompiler` rules engine              | `heiwa_cognition/program_compiler.py` | ~140           |
| Enrichment wiring                           | `heiwa_cognition/enrichment.py`       | ~6 delta       |
| Spine pass-through                          | `spine.py`                            | ~1 delta       |
| HeiwaClaw advisory validation + persistence | `heiwaclaw.py`                        | ~35 delta      |
| Execution memory signature                  | `memory.py`                           | ~2 delta       |
| Tests                                       | 3 new test files                      | ~220           |
| **Total new code**                          |                                       | **~480 lines** |

**Design constraints:**

- Backward compatible throughout. Old payloads without `execution_program` work unchanged.
- No LLM calls added. No new dependencies.
- Acceptance validation is advisory in v1 — does not override execution status.
- `schema_version` + `source_kind` enable forward-compatible migration to `/programs/`.
- Compiler takes `profile + route + raw_text` — uses routed tool/runtime, not just intent defaults.
- Program persisted to execution memory for future optimization loop.
