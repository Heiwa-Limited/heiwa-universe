"""Local Reflex Adapter (Ollama, vLLM, LiteLLM)."""
from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from heiwa_protocol.routing import BrokerRouteResult
from heiwa_sdk.heiwaclaw.adapters.base import BaseClawAdapter
from heiwa_sdk.tool_mesh import ToolMesh

logger = logging.getLogger("SDK.Claw.Reflex")

_RUNTIME_ENGINE_INTENTS = {"chat", "general", "research", "strategy"}
_RUNTIME_ENGINE_UNAVAILABLE_MARKERS = (
    "is unavailable",
    "command not found",
    "no such file",
    "not installed",
)


class ReflexAdapter(BaseClawAdapter):
    """Handles local HTTP/Reflex execution with optional LLM engine fallback."""

    def __init__(self, root_dir: Path):
        self.root = root_dir
        self.tool_mesh = ToolMesh(root_dir)

    async def execute(
        self,
        route: BrokerRouteResult,
        instruction: str,
        env: dict[str, str],
        model: str | None = None,
    ) -> tuple[int, str]:
        # Preference: direct runtime engine if configured
        if self._should_prefer_runtime_engine(route, "heiwa_reflex"):
            runtime_result = self._execute_via_runtime_engine(route, instruction, env.get("HEIWA_SYSTEM_PROMPT"))
            if runtime_result:
                return 0, runtime_result

        # Execute via ToolMesh (standard path)
        exit_code, output = await self.tool_mesh.execute(
            "heiwa_reflex",
            instruction,
            model=model,
            extra_env=env,
        )

        # Retry with runtime engine on specific failures
        if self._should_retry_with_runtime_engine(route, exit_code, output):
            runtime_result = self._execute_via_runtime_engine(route, instruction, env.get("HEIWA_SYSTEM_PROMPT"))
            if runtime_result:
                return 0, runtime_result

        return exit_code, output

    def _should_prefer_runtime_engine(self, route: BrokerRouteResult, adapter_tool: str) -> bool:
        if not self._runtime_engine_allowed(route):
            return False
        return adapter_tool == "heiwa_reflex"

    def _should_retry_with_runtime_engine(self, route: BrokerRouteResult, exit_code: int, output: str) -> bool:
        if exit_code == 0 or not self._runtime_engine_allowed(route):
            return False
        lowered = str(output or "").lower()
        return any(marker in lowered for marker in _RUNTIME_ENGINE_UNAVAILABLE_MARKERS)

    def _runtime_engine_allowed(self, route: BrokerRouteResult) -> bool:
        runtime = str(route.target_runtime or "").strip().lower()
        intent = str(route.intent_class or "").strip().lower()
        return runtime in {"railway", "cloud"} and intent in _RUNTIME_ENGINE_INTENTS

    def _execute_via_runtime_engine(
        self,
        route: BrokerRouteResult,
        instruction: str,
        system_prompt: str | None = None,
    ) -> str:
        try:
            from heiwa_hub.cognition.llm_local import LocalLLMEngine
        except Exception as exc:
            logger.info("Runtime engine unavailable: %s", exc)
            return ""

        engine = LocalLLMEngine()
        result = engine.generate(
            prompt=instruction,
            system=system_prompt or None,
            complexity=self._runtime_engine_complexity(route),
            runtime=route.target_runtime or "railway",
        )
        return result.strip() if result else ""

    @staticmethod
    def _runtime_engine_complexity(route: BrokerRouteResult) -> str:
        tier = str(route.target_tier or "").strip().lower()
        if tier in {"tier6_premium_context", "tier7_supreme_court"}:
            return "high"
        if tier in {"tier2_fast_context", "tier3_orchestrator", "tier4_pooled_orchestrator", "tier5_heavy_code"}:
            return "medium"
        return "low"
