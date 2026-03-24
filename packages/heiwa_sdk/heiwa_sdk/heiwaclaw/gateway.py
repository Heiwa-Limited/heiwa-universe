"""OpenClaw: Spawning/dispatch mechanism for HeiwaClaw.

Resolves routing decisions to concrete adapters and executes tasks
via Claude Code, Codex, Gemini, Antigravity, and Ollama.
"""
from __future__ import annotations

import logging
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, Type

from heiwa_protocol.routing import BrokerRouteResult
from heiwa_sdk.provider_registry import ProviderRegistry
from heiwa_sdk.heiwaclaw.adapters.base import BaseClawAdapter

logger = logging.getLogger("SDK.OpenClaw")

@dataclass(slots=True)
class OpenClawDispatch:
    gateway_tool: str
    adapter_tool: str
    provider: str
    target_model: str
    target_runtime: str
    session_id: str
    transport: str
    rate_group: str
    auth_kind: str
    websocket_preferred: bool
    direct_execution: bool
    rationale: str
    adapter_env: dict[str, str] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


# Backward compatibility alias
HeiwaClawDispatch = OpenClawDispatch


class OpenClaw:
    """Spawning mechanism for all model/provider routing.

    This is how HeiwaClaw (the living agent) reaches out to
    Claude Code, Codex, Gemini, Antigravity, and Ollama.
    """

    def __init__(self, root_dir: Path):
        self.root = root_dir
        self.providers = ProviderRegistry(root_dir)
        self._adapters: Dict[str, BaseClawAdapter] = {}
        self._load_adapters()
        from heiwa_sdk.hooks import ExecutionHookManager
        self.hooks = ExecutionHookManager(root_dir)

    def _load_adapters(self):
        """Initialize specialized adapters."""
        from heiwa_sdk.heiwaclaw.adapters.reflex import ReflexAdapter
        from heiwa_sdk.heiwaclaw.adapters.gemini import GeminiAdapter
        from heiwa_sdk.heiwaclaw.adapters.claude import ClaudeAdapter
        from heiwa_sdk.heiwaclaw.adapters.codex import CodexAdapter
        from heiwa_sdk.heiwaclaw.adapters.acp import ACPAdapter

        self._adapters = {
            "heiwa_reflex": ReflexAdapter(self.root),
            "heiwa_gemini": GeminiAdapter(self.root),
            "heiwa_claude": ClaudeAdapter(self.root),
            "heiwa_code": CodexAdapter(self.root),
            "heiwa_acp": ACPAdapter(self.root),
        }

    def _provider_for(self, route: BrokerRouteResult) -> str:
        provider = self.providers.provider_for_worker(route.assigned_worker)
        if provider:
            return provider
        provider = self.providers.provider_for_model(route.target_model)
        return provider or "local"

    def resolve(self, payload: BrokerRouteResult | dict[str, Any]) -> OpenClawDispatch:
        route = payload if isinstance(payload, BrokerRouteResult) else BrokerRouteResult.from_payload(payload)

        if route.target_tool == "heiwa_ops":
            return OpenClawDispatch(
                gateway_tool=route.target_tool,
                adapter_tool="heiwa_ops",
                provider="local",
                target_model=route.target_model,
                target_runtime=route.target_runtime,
                session_id=f"heiwa-ops-{route.task_id}",
                transport="direct_exec",
                rate_group="deterministic_ops",
                auth_kind="none",
                websocket_preferred=False,
                direct_execution=True,
                rationale=route.rationale,
            )

        if route.target_tool == "heiwa_acp":
            return OpenClawDispatch(
                gateway_tool=route.target_tool,
                adapter_tool="heiwa_acp",
                provider="remote",
                target_model=route.target_model,
                target_runtime=route.target_runtime,
                session_id=f"heiwa-acp-{route.task_id}",
                transport="acp_bridge",
                rate_group="acp_remote",
                auth_kind="none",
                websocket_preferred=True,
                direct_execution=False,
                rationale=route.rationale,
            )

        provider = self._provider_for(route)
        provider_config = self.providers.resolve(provider)

        return OpenClawDispatch(
            gateway_tool=provider_config.gateway_tool,
            adapter_tool=provider_config.adapter_tool,
            provider=provider,
            target_model=route.target_model,
            target_runtime=route.target_runtime,
            session_id=f"heiwa-{route.task_id}",
            transport=provider_config.transport,
            rate_group=provider_config.rate_group,
            auth_kind=provider_config.auth_kind,
            websocket_preferred=provider_config.websocket_preferred,
            direct_execution=provider_config.direct_execution,
            rationale=route.rationale,
            adapter_env=provider_config.env,
        )

    async def execute(
        self,
        payload: BrokerRouteResult | dict[str, Any],
        instruction: str,
        system_prompt: str | None = None,
    ) -> tuple[int, str]:
        route = payload if isinstance(payload, BrokerRouteResult) else BrokerRouteResult.from_payload(payload)
        dispatch = self.resolve(route)

        # 1. pre_tool_call hook validation
        allow, reason, hook_metadata = self.hooks.before_tool_call(
            tool=dispatch.adapter_tool,
            proposal_id=str(route.task_id),
            node_id=str(route.assigned_worker or "unknown"),
            payload=dispatch.to_dict(),
        )
        if not allow:
            logger.error("🚫 Execution denied by pre-hook: %s", reason)
            return 1, f"Execution denied by pre-hook: {reason}"

        env = {
            "HEIWA_GATEWAY_TRANSPORT": dispatch.transport,
            "HEIWA_PROVIDER": dispatch.provider,
            "HEIWA_RATE_GROUP": dispatch.rate_group,
            "HEIWA_AUTH_KIND": dispatch.auth_kind,
            "OPENCLAW_EXEC_MODE": "gateway" if dispatch.websocket_preferred else "local",
            "OPENCLAW_SESSION_ID": dispatch.session_id,
        }
        if system_prompt:
            env["HEIWA_SYSTEM_PROMPT"] = system_prompt
        env.update(dispatch.adapter_env)

        adapter = self._adapters.get(dispatch.adapter_tool)
        if not adapter:
            if dispatch.adapter_tool == "heiwa_ops":
                return await self._execute_direct_ops(instruction)
            return 1, f"Unknown adapter tool: {dispatch.adapter_tool}"

        exit_code, output = await adapter.execute(
            route=route,
            instruction=instruction,
            env=env,
            model=dispatch.target_model or None
        )

        # 2. post_tool_call hook execution
        self.hooks.after_tool_call(
            tool=dispatch.adapter_tool,
            proposal_id=str(route.task_id),
            exit_code=exit_code,
            output=output,
            audit_metadata=hook_metadata,
        )

        self._record_rate_usage(dispatch, exit_code, output)

        return exit_code, output

    async def _execute_direct_ops(self, instruction: str) -> tuple[int, str]:
        return 0, "Direct ops executed."

    def _record_rate_usage(self, dispatch: OpenClawDispatch, exit_code: int, output: str):
        from ..rate_ledger import get_rate_ledger
        ledger = get_rate_ledger()
        ledger.record(dispatch.rate_group)


# Backward compatibility alias
HeiwaClawGateway = OpenClaw
