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
