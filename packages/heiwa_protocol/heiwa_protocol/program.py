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
