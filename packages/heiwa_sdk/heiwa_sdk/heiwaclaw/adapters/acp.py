"""Agentic Control Protocol (ACP) Adapter for cross-node coordination."""
from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from heiwa_protocol.routing import BrokerRouteResult
from heiwa_sdk.heiwaclaw.adapters.base import BaseClawAdapter

logger = logging.getLogger("SDK.Claw.ACP")


class ACPAdapter(BaseClawAdapter):
    """Handles communication with remote Heiwa instances via ACP."""

    def __init__(self, root_dir: Path):
        self.root = root_dir

    async def execute(
        self,
        route: BrokerRouteResult,
        instruction: str,
        env: dict[str, str],
        model: str | None = None,
    ) -> tuple[int, str]:
        # ACP uses the ToolMesh or a specialized WebSocket client to send
        # the task to another Heiwa node.
        # For Phase 5, we implement the foundational handoff via the Hub's worker mesh.
        logger.info("🔌 [ACP] Routing task %s to remote node: %s", route.task_id, route.assigned_worker)

        import websockets
        import json
        from heiwa_sdk.config import settings

        hub_url = settings.HUB_BASE_URL.replace("https://", "wss://").replace("http://", "ws://")
        ws_url = f"{hub_url}/ws/worker"

        try:
            # We timeout quickly to avoid blocking the gateway if the hub is unreachable
            async with websockets.connect(ws_url, open_timeout=5) as websocket:
                # 1. Register as an ACP proxy
                auth_token = settings.HEIWA_AUTH_TOKEN
                registration = {
                    "type": "register",
                    "worker_id": f"acp-proxy-{settings.RAILWAY_ENVIRONMENT_NAME}",
                    "auth_token": auth_token,
                    "capabilities": {"role": "proxy", "acp": True}
                }
                await websocket.send(json.dumps(registration))

                # 2. Wait for confirmation
                resp_raw = await websocket.recv()
                resp = json.loads(resp_raw)
                if resp.get("type") == "error":
                    return 1, f"ACP handoff failed at registration: {resp.get('detail')}"

                # 3. Hand off the task via the result loop (Hub bus integration)
                # This informs the Hub that the task is being handled remotely.
                task_payload = {
                    "type": "result",
                    "data": {
                        "task_id": route.task_id,
                        "status": "ACP_HANDOFF",
                        "summary": f"Handoff to {route.assigned_worker} initiated via ACP bridge."
                    }
                }
                await websocket.send(json.dumps(task_payload))
                return 0, f"ACP handoff to {route.assigned_worker} succeeded."
        except Exception as e:
            logger.error("ACP handoff error: %s", e)
            return 1, f"ACP handoff failed: {str(e)}"
