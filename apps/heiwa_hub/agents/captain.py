"""
Heiwa Captain Agent — Always-On AI Orchestrator

Event-driven Gemini Flash orchestrator on Railway. The Captain is Heiwa's
internal intelligence layer:
- Monitors system health, rate limits, and fleet status
- Proactively communicates via Discord (executive-briefing, central-comms)
- Delegates complex work to Class 3 CLI tools via the rate cascade
- Maintains persistent focus/context in SpacetimeDB
- Coordinates multi-agent workflows

The Captain uses free API inference (Gemini Flash/Pro) for its own reasoning,
NOT subscription CLI tools. CLI tools are reserved for actual task execution.
"""

import asyncio
import logging
import time
from typing import Any

from heiwa_hub.agents.base import BaseAgent
from heiwa_protocol.protocol import Subject
from heiwa_sdk.db import Database

logger = logging.getLogger("Captain")

# How often the Captain runs its proactive loop (seconds)
CAPTAIN_TICK_SEC = 60
# Minimum interval between Discord status broadcasts
STATUS_BROADCAST_INTERVAL_SEC = 300


class CaptainAgent(BaseAgent):
    """Always-on orchestrator that monitors, delegates, and communicates."""

    def __init__(self):
        super().__init__(name="heiwa-captain")
        self.db = Database()
        self._llm = None  # Lazy-loaded to avoid import-time side effects
        self._last_status_broadcast = 0.0
        self._task_results: list[dict[str, Any]] = []
        self._pending_alerts: list[str] = []

    @property
    def llm(self):
        """Lazy-load the LLM engine for Captain's own reasoning."""
        if self._llm is None:
            from heiwa_cognition.llm import LocalLLMEngine
            self._llm = LocalLLMEngine()
        return self._llm

    async def run(self):
        await self.start()

        # Listen to system events
        await self.listen(Subject.TASK_EXEC_RESULT, self._on_task_result)
        await self.listen(Subject.TASK_STATUS, self._on_task_status)
        await self.listen(Subject.NODE_HEARTBEAT, self._on_heartbeat)
        await self.listen(Subject.LOG_ERROR, self._on_error)

        logger.info("Captain online. Monitoring system health and coordinating work.")

        try:
            while self.running:
                await self._captain_tick()
                await asyncio.sleep(CAPTAIN_TICK_SEC)
        except KeyboardInterrupt:
            await self.shutdown()

    # ------------------------------------------------------------------ #
    #  Event handlers                                                      #
    # ------------------------------------------------------------------ #

    async def _on_task_result(self, data: dict[str, Any]):
        """Track completed task results for situational awareness."""
        payload = data.get("data", data)
        self._task_results.append({
            "task_id": payload.get("task_id"),
            "status": payload.get("status"),
            "intent": payload.get("intent_class"),
            "provider": payload.get("provider"),
            "elapsed": payload.get("elapsed_sec"),
            "ts": time.time(),
        })
        # Keep only last 50 results
        if len(self._task_results) > 50:
            self._task_results = self._task_results[-50:]

    async def _on_task_status(self, data: dict[str, Any]):
        """Watch for blocked or failed tasks that need attention."""
        payload = data.get("data", data)
        status = payload.get("status", "")
        if status in ("BLOCKED_AUTH", "BLOCKED_NO_CONTENT", "FAIL", "EXPIRED"):
            task_id = payload.get("task_id", "unknown")
            msg = payload.get("message", status)
            self._pending_alerts.append(f"Task {task_id}: {msg}")

    async def _on_heartbeat(self, data: dict[str, Any]):
        """Track fleet node liveness."""
        # BaseAgent + Spine already handle this; Captain just observes
        pass

    async def _on_error(self, data: dict[str, Any]):
        """Capture error events for proactive reporting."""
        payload = data.get("data", data)
        agent = payload.get("agent", "unknown")
        content = str(payload.get("content", ""))[:200]
        self._pending_alerts.append(f"Error from {agent}: {content}")

    # ------------------------------------------------------------------ #
    #  Proactive loop                                                      #
    # ------------------------------------------------------------------ #

    async def _captain_tick(self):
        """Periodic health check and proactive communication."""
        now = time.time()

        # Gather system state
        system_state = await self._gather_system_state()

        # Check for issues that need proactive communication
        alerts = list(self._pending_alerts)
        self._pending_alerts.clear()

        if alerts:
            await self._handle_alerts(alerts, system_state)

        # Periodic status broadcast
        if now - self._last_status_broadcast > STATUS_BROADCAST_INTERVAL_SEC:
            await self._broadcast_status(system_state)
            self._last_status_broadcast = now

    async def _gather_system_state(self) -> dict[str, Any]:
        """Collect current system state for Captain's awareness."""
        state: dict[str, Any] = {
            "ts": time.time(),
            "recent_tasks": len(self._task_results),
        }

        # Fleet status
        try:
            nodes = self.db.list_nodes()
            state["nodes"] = len(nodes) if isinstance(nodes, list) else 0
            state["nodes_online"] = sum(
                1 for n in (nodes or [])
                if isinstance(n, dict) and n.get("status") == "ONLINE"
            )
        except Exception:
            state["nodes"] = 0
            state["nodes_online"] = 0

        # Recent task success rate
        recent = [r for r in self._task_results if time.time() - r["ts"] < 300]
        if recent:
            passed = sum(1 for r in recent if r["status"] == "PASS")
            state["task_success_rate_5m"] = round(passed / len(recent), 2)
            state["tasks_5m"] = len(recent)
        else:
            state["task_success_rate_5m"] = None
            state["tasks_5m"] = 0

        # LLM availability
        try:
            state["llm_available"] = self.llm.is_available()
        except Exception:
            state["llm_available"] = False

        return state

    async def _handle_alerts(self, alerts: list[str], system_state: dict[str, Any]):
        """Process and report alerts via the bus (Messenger picks up for Discord)."""
        alert_summary = "\n".join(f"- {a}" for a in alerts[:10])
        logger.warning("Captain alerts:\n%s", alert_summary)

        await self.speak(Subject.LOG_INFO, {
            "agent": "captain",
            "content": f"**Captain Alert**\n{alert_summary}",
            "response_channel_id": self._get_comms_channel(),
        })

    async def _broadcast_status(self, system_state: dict[str, Any]):
        """Broadcast periodic system status to Discord."""
        nodes = system_state.get("nodes_online", 0)
        tasks_5m = system_state.get("tasks_5m", 0)
        success_rate = system_state.get("task_success_rate_5m")
        llm_ok = system_state.get("llm_available", False)

        status_parts = [
            f"Nodes: {nodes} online",
            f"Tasks (5m): {tasks_5m}",
        ]
        if success_rate is not None:
            status_parts.append(f"Success: {int(success_rate * 100)}%")
        status_parts.append(f"LLM: {'OK' if llm_ok else 'DOWN'}")

        status_line = " | ".join(status_parts)
        logger.info("Captain status: %s", status_line)

        await self.speak(Subject.SWARM_STATUS_REPORT, {
            "agent": "captain",
            "status": status_line,
            "system_state": system_state,
            "response_channel_id": self._get_comms_channel(),
        })

    def _get_comms_channel(self) -> str | None:
        """Resolve the Captain's preferred Discord channel for comms."""
        try:
            return str(self.db.get_discord_channel("central-comms") or "")
        except Exception:
            return None
