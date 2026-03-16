"""Generic CLI-based Adapter for STDIO tools."""
from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from heiwa_protocol.routing import BrokerRouteResult
from heiwa_sdk.heiwaclaw.adapters.base import BaseClawAdapter
from heiwa_sdk.tool_mesh import ToolMesh

logger = logging.getLogger("SDK.Claw.CLI")


class CLIAdapter(BaseClawAdapter):
    """Handles execution for CLI-based tools (Gemini, Claude, Codex)."""

    def __init__(self, root_dir: Path, adapter_tool: str):
        self.root = root_dir
        self.adapter_tool = adapter_tool
        self.tool_mesh = ToolMesh(root_dir)

    async def execute(
        self,
        route: BrokerRouteResult,
        instruction: str,
        env: dict[str, str],
        model: str | None = None,
    ) -> tuple[int, str]:
        # CLI tools typically don't have the "runtime engine fallback" 
        # as they ARE the primary engines for their respective tiers.
        return await self.tool_mesh.execute(
            self.adapter_tool,
            instruction,
            model=model,
            extra_env=env,
        )
