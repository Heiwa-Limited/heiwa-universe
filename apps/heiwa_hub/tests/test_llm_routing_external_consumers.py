from __future__ import annotations

from pathlib import Path
from unittest.mock import AsyncMock

import pytest

import heiwa_cognition.llm as llm_module
from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent
from heiwa_hub.chat import ChatEngine
from heiwa_sdk.audit import RepoAuditor
from heiwa_sdk.heiwaclaw.adapters.reflex import ReflexAdapter
from heiwa_protocol.routing import BrokerRouteResult


class _ExplodingLocalLLMEngine:
    def __init__(self, *args, **kwargs):
        raise AssertionError("legacy LocalLLMEngine path should not be used")


def _route(**overrides: object) -> BrokerRouteResult:
    payload = {
        "request_id": "req-test",
        "task_id": "task-test",
        "raw_text": "reply with exactly OK",
        "source_surface": "cli",
        "intent_class": "chat",
        "risk_level": "low",
        "privacy_level": "local",
        "compute_class": 1,
        "assigned_worker": "node_a_orchestrator",
        "target_tool": "heiwa_claw",
        "target_model": "ollama/llama-4-scout:q4_k_m",
        "target_runtime": "railway",
        "target_tier": "tier1_local",
        "requires_approval": False,
        "rationale": "test route",
    }
    payload.update(overrides)
    return BrokerRouteResult.from_payload(payload)


@pytest.mark.asyncio
async def test_repo_auditor_uses_llm_facade(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("RAILWAY_ENVIRONMENT", raising=False)
    monkeypatch.delenv("RAILWAY_PROJECT_ID", raising=False)
    monkeypatch.setattr(llm_module, "LocalLLMEngine", _ExplodingLocalLLMEngine, raising=False)
    monkeypatch.setattr(llm_module, "llm_generate_async", AsyncMock(return_value="audit ok"), raising=False)

    auditor = RepoAuditor(root=Path.cwd())
    result = await auditor._run_ollama_audit()

    assert result == "audit ok"


def test_reflex_adapter_uses_llm_facade(monkeypatch: pytest.MonkeyPatch) -> None:
    from heiwa_hub.cognition import llm_local as llm_local_module

    monkeypatch.setattr(llm_local_module, "LocalLLMEngine", _ExplodingLocalLLMEngine, raising=False)
    monkeypatch.setattr(llm_local_module, "llm_generate", lambda **kwargs: "reflex ok", raising=False)

    adapter = ReflexAdapter(root_dir=Path.cwd())
    result = adapter._execute_via_runtime_engine(_route(), "do the thing", "system prompt")

    assert result == "reflex ok"


@pytest.mark.asyncio
async def test_heiwaclaw_summary_uses_llm_facade(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(llm_module, "LocalLLMEngine", _ExplodingLocalLLMEngine, raising=False)
    monkeypatch.setattr(llm_module, "llm_generate_async", AsyncMock(return_value="summary ok"), raising=False)

    result = await HeiwaClawAgent._llm_summarize(object(), "important text")

    assert result == "summary ok"


@pytest.mark.asyncio
async def test_chat_engine_uses_llm_facade(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(llm_module, "LocalLLMEngine", _ExplodingLocalLLMEngine, raising=False)
    monkeypatch.setattr(llm_module, "llm_generate", lambda *args, **kwargs: "chat ok", raising=False)
    monkeypatch.setattr(ChatEngine, "_try_boost_node", AsyncMock(return_value=None))

    engine = ChatEngine()
    reply = await engine._route_llm("hello")

    assert reply == "chat ok"
