from __future__ import annotations

from pathlib import Path
import sys
import types

import pytest

sys.modules.setdefault("aiohttp", types.ModuleType("aiohttp"))

from heiwa_hub.agents.learning import LearningAgent
from heiwa_hub.knowledge_pipeline import build_learning_event
from heiwa_knowledge.registry import KnowledgeRegistry


@pytest.mark.asyncio
async def test_learning_agent_persists_autoresearch_benchmark_candidate(tmp_path: Path) -> None:
    indexed: list[tuple[str, str]] = []

    class _MemoryStub:
        async def index_file(self, file_path: str, content: str, source_type: str = "code_file") -> bool:
            indexed.append((file_path, source_type))
            assert "HeiwaBench" in content
            return True

    registry = KnowledgeRegistry(root_dir=tmp_path, stdb=None)
    agent = LearningAgent(root_dir=tmp_path, registry=registry, memory_service=_MemoryStub())
    payload = {
        "task_id": "task-neo-evo",
        "status": "PASS",
        "intent_class": "self_buff",
        "summary": "HeiwaBench now drives the autoresearch loop and reports suite-based control-plane scores.",
    }
    decision_trace = {
        "task_id": "task-neo-evo",
        "intent_class": "self_buff",
        "agent": "codex",
        "event": "task_completed",
        "rationale": "Self-improvement loops should benchmark the canonical Heiwa surface.",
        "artifacts": {
            "summary": payload["summary"],
            "changed_paths": [
                "apps/heiwa_hub/scripts/run_evolution.py",
                "packages/heiwa_sdk/heiwa_sdk/bench.py",
                "apps/heiwa_hub/agents/learning.py",
            ],
            "verification_outcomes": [{"name": "task_status", "passed": True}],
        },
        "timestamp": "2026-03-19T00:00:00Z",
    }
    event = build_learning_event({"route": {"normalization": {}}}, payload, decision_trace)

    await agent._handle_learning_event({"data": event})

    entries = list((tmp_path / "packages" / "heiwa_knowledge" / "entries").glob("*.md"))
    assert len(entries) == 1
    content = entries[0].read_text(encoding="utf-8")
    assert "HeiwaBench" in content
    assert "autoresearch" in content.lower()
    digest = (tmp_path / "config" / "identities" / "persona" / "instincts.md").read_text(encoding="utf-8")
    assert "HeiwaBench" in digest
    assert indexed == [(entries[0].relative_to(tmp_path).as_posix(), "knowledge_entry")]
