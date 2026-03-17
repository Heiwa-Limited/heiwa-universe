"""
Heiwa Agent — Machine-Perspective Collaborator

The Heiwa Agent is Heiwa's always-on intelligence layer that communicates
exclusively via Discord DM with the operator. It observes the system from
a machine perspective: watching tasks flow, agents execute, models route,
nodes heartbeat — and narrates what it sees so the operator gets a REPL-like
collaboration loop.

Structured agent output (task results, telemetry, logs) still posts to the
Discord server via Messenger. The Heiwa Agent never posts to server channels.

The Heiwa Agent uses free API inference (Gemini Flash/Pro) for its own reasoning,
NOT subscription CLI tools. CLI tools are reserved for actual task execution.
"""

import asyncio
import logging
import time
from typing import Any

from heiwa_hub.agents.base import BaseAgent
from heiwa_protocol.protocol import Subject
from heiwa_sdk.db import Database
from heiwa_sdk.audit import RepoAuditor
from heiwa_sdk.memory import MemoryService
from heiwa_sdk.agent_memory import AgentMemory
from pathlib import Path

logger = logging.getLogger("HeiwaAgent")

CAPTAIN_TICK_SEC = 60
STATUS_BROADCAST_INTERVAL_SEC = 300
AUDIT_INTERVAL_SEC = 600
MODEL_TUNING_INTERVAL_SEC = 300
PRUNE_INTERVAL_SEC = 3600


class HeiwaAgent(BaseAgent):
    """Always-on orchestrator that DMs the operator with machine-perspective observations."""

    def __init__(self):
        super().__init__(name="heiwa-agent")
        self.db = Database()
        self.root = Path(__file__).resolve().parents[3]
        self.auditor = RepoAuditor(self.root)
        self.memory = MemoryService(stdb=self.db.stdb) if self.db.stdb else None
        self._llm = None
        self._last_status_broadcast = 0.0
        self._last_audit = 0.0
        self._last_tuning = 0.0
        self._last_prune = 0.0
        self._task_results: list[dict[str, Any]] = []
        self._pending_alerts: list[str] = []
        self._boot_time = time.time()
        self._tasks_seen = 0
        self._errors_seen = 0
        self.agent_memory = AgentMemory(stdb=self.db.stdb) if self.db.stdb else None
        self._boot_context: dict | None = None

    @property
    def llm(self):
        if self._llm is None:
            from heiwa_cognition.llm import LocalLLMEngine
            self._llm = LocalLLMEngine()
        return self._llm

    async def run(self):
        await self.start()

        await self.listen(Subject.TASK_EXEC_RESULT, self._on_task_result)
        await self.listen(Subject.TASK_STATUS, self._on_task_status)
        await self.listen(Subject.TASK_INGRESS, self._on_task_ingress)
        await self.listen(Subject.NODE_HEARTBEAT, self._on_heartbeat)
        await self.listen(Subject.LOG_ERROR, self._on_error)

        logger.info("Heiwa Agent online. Monitoring system health and coordinating work.")

        # Wait for Messenger/Discord to connect before sending boot DM
        await asyncio.sleep(15)
        
        # Boot hydration: load last session's memory
        self._boot_context = self._hydrate_boot_context()
        boot_ctx_summary = ""
        if self._boot_context.get("summaries"):
            boot_ctx_summary = (
                "\n\nFrom last session:\n> "
                + self._boot_context["summaries"][0].get("content", "")[:200]
            )

        await self._dm(self._build_boot_message() + boot_ctx_summary)

        try:
            while self.running:
                await self._agent_tick()
                await asyncio.sleep(CAPTAIN_TICK_SEC)
        except KeyboardInterrupt:
            await self.shutdown()

    # ------------------------------------------------------------------ #
    #  DM channel — all Captain comms go through here                     #
    # ------------------------------------------------------------------ #

    async def _dm(self, content: str):
        """Send a message to the operator via DM (Messenger handles delivery)."""
        self._store_agent_response(content)
        await self.speak(Subject.HEIWA_AGENT_DM, {
            "agent": "heiwa-agent",
            "content": content,
        })

    # ------------------------------------------------------------------ #
    #  Event handlers — observe and narrate                               #
    # ------------------------------------------------------------------ #

    async def _on_task_ingress(self, data: dict[str, Any]):
        """A new task just entered the system. Narrate what we see."""
        payload = data.get("data", data)
        task_id = payload.get("task_id", "?")
        raw = str(payload.get("raw_text", ""))[:120]
        intent = payload.get("intent_class", "unknown")
        source = payload.get("source", "unknown")
        self._tasks_seen += 1

        # STORE: persist the incoming task as an operator message
        self._store_operator_message(
            f"[task:{task_id}] {raw}", source=source
        )

        await self._dm(
            f"New task landed from **{source}**.\n"
            f"`{task_id}` classified as **{intent}**\n"
            f"> {raw}\n"
            f"Routing it now — I'll tell you what happens."
        )

        # FOCUS: track active topic
        self._update_focus(intent, {"task_id": task_id, "raw": raw[:80]})

    async def _on_task_result(self, data: dict[str, Any]):
        """A task completed. Tell the operator what happened."""
        payload = data.get("data", data)
        task_id = payload.get("task_id", "?")
        status = payload.get("status", "UNKNOWN")
        intent = payload.get("intent_class", "")
        provider = payload.get("provider") or payload.get("target_tool", "unknown")
        elapsed = payload.get("elapsed_sec")
        summary = str(payload.get("summary", ""))[:300]

        self._task_results.append({
            "task_id": task_id,
            "status": status,
            "intent": intent,
            "provider": provider,
            "elapsed": elapsed,
            "ts": time.time(),
        })
        if len(self._task_results) > 50:
            self._task_results = self._task_results[-50:]

        icon = "pass" if status == "PASS" else "fail"
        elapsed_str = f" in {elapsed:.1f}s" if elapsed else ""

        msg = f"**{icon.upper()}** `{task_id}`{elapsed_str} via **{provider}**"
        if summary:
            msg += f"\n> {summary}"

        # If it failed, add context on what to do
        if status != "PASS":
            msg += "\n\nThis one didn't land. Want me to retry with a different route, or should we look at it together?"

        await self._dm(msg)

    async def _on_task_status(self, data: dict[str, Any]):
        """Watch for blocked or failed tasks — alert the operator immediately."""
        payload = data.get("data", data)
        status = payload.get("status", "")
        if status in ("BLOCKED_AUTH", "BLOCKED_NO_CONTENT", "FAIL", "EXPIRED"):
            task_id = payload.get("task_id", "unknown")
            msg = payload.get("message", status)
            self._pending_alerts.append(f"`{task_id}`: {msg}")

    async def _on_heartbeat(self, data: dict[str, Any]):
        pass

    async def _on_error(self, data: dict[str, Any]):
        """System error — tell the operator."""
        payload = data.get("data", data)
        agent = payload.get("agent", "unknown")
        content = str(payload.get("content", ""))[:200]
        self._errors_seen += 1
        self._pending_alerts.append(f"**{agent}** threw: {content}")

    # ------------------------------------------------------------------ #
    #  Memory Loop Methods                                               #
    # ------------------------------------------------------------------ #

    def _store_operator_message(self, content: str, source: str = "discord_dm"):
        """STORE step: persist operator input to captain_messages."""
        if self.agent_memory:
            self.agent_memory.store_message(role="operator", content=content, source=source)

    def _store_agent_response(self, content: str):
        """STORE step: persist agent output to captain_messages."""
        if self.agent_memory:
            self.agent_memory.store_message(role="agent", content=content, source="system")

    async def _maybe_compress(self):
        """Check if rolling compression is needed and run it."""
        if not self.agent_memory:
            return
        ctx = self.agent_memory.load_context_window()
        messages = ctx.get("messages", [])
        if self.agent_memory.needs_compression(messages):
            await self._run_rolling_compression(messages)

    async def _run_rolling_compression(self, messages: list[dict]):
        """Compress oldest uncompressed messages into a rolling summary."""
        if not messages:
            return
        mid = len(messages) // 2
        to_compress = messages[:mid]
        if not to_compress:
            return

        text_block = "\n".join(
            f"[{m.get('role', '?')}] {m.get('content', '')}" for m in to_compress
        )
        try:
            summary = await self._llm_summarize(text_block)
        except Exception as e:
            logger.error("Rolling compression failed: %s", e)
            return

        range_start = to_compress[0].get("timestamp", 0)
        range_end = to_compress[-1].get("timestamp", 0)

        self.agent_memory.store_summary(
            summary_type="rolling",
            content=summary,
            range_start=range_start,
            range_end=range_end,
            messages_compressed=len(to_compress),
        )
        self.agent_memory.mark_compressed(before_timestamp=range_end + 1)
        logger.info("Rolling compression: %d messages → summary", len(to_compress))

    async def _llm_summarize(self, text: str) -> str:
        """REASON helper: use LLM to generate a summary."""
        prompt = (
            "Summarize this conversation concisely, preserving key decisions, "
            "action items, and technical details:\n\n" + text
        )
        return await self.llm.generate(prompt)

    def _hydrate_boot_context(self) -> dict:
        """BOOT: load last session's uncompressed messages + recent summaries."""
        if not self.agent_memory:
            return {"messages": [], "focuses": [], "summaries": []}
        return self.agent_memory.load_context_window()

    def _should_cascade(self, text: str) -> bool:
        """REASON: detect if message needs stronger model (cascade) vs Flash."""
        if not self.agent_memory:
            return False
        return self.agent_memory.detect_complexity(text)

    def _update_focus(self, topic: str, context: dict[str, Any], priority: int = 3):
        """FOCUS step: create or update active focus tracking entry."""
        if not self.agent_memory:
            return
        # Check if there's already an active focus on this topic
        active = self.agent_memory.stdb.get_active_focuses()
        existing = next((f for f in active if f.get("topic") == topic), None)
        if existing:
            self.agent_memory.upsert_focus(
                topic=topic,
                context=context,
                priority=priority,
                focus_id=existing["focus_id"],
            )
        else:
            self.agent_memory.upsert_focus(
                topic=topic,
                context=context,
                priority=priority,
            )

    # ------------------------------------------------------------------ #
    #  Proactive loop                                                      #
    # ------------------------------------------------------------------ #

    async def _agent_tick(self):
        now = time.time()

        system_state = await self._gather_system_state()

        # Memory compression check
        await self._maybe_compress()

        # Handle queued alerts
        alerts = list(self._pending_alerts)
        self._pending_alerts.clear()
        if alerts:
            await self._handle_alerts(alerts, system_state)

        # Periodic repo audit
        if now - self._last_audit > AUDIT_INTERVAL_SEC:
            asyncio.create_task(self._run_periodic_audit())
            self._last_audit = now

        # Periodic model tuning
        if now - self._last_tuning > MODEL_TUNING_INTERVAL_SEC:
            asyncio.create_task(self._tune_model_tiers())
            self._last_tuning = now

        # Periodic maintenance
        if now - self._last_prune > PRUNE_INTERVAL_SEC:
            asyncio.create_task(self._run_maintenance())
            self._last_prune = now

        # Periodic status — conversational, not a wall of metrics
        if now - self._last_status_broadcast > STATUS_BROADCAST_INTERVAL_SEC:
            await self._broadcast_status(system_state)
            self._last_status_broadcast = now

    async def _gather_system_state(self) -> dict[str, Any]:
        state: dict[str, Any] = {
            "ts": time.time(),
            "uptime_sec": time.time() - self._boot_time,
            "recent_tasks": len(self._task_results),
        }

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

        recent = [r for r in self._task_results if time.time() - r["ts"] < 300]
        if recent:
            passed = sum(1 for r in recent if r["status"] == "PASS")
            state["task_success_rate_5m"] = round(passed / len(recent), 2)
            state["tasks_5m"] = len(recent)
        else:
            state["task_success_rate_5m"] = None
            state["tasks_5m"] = 0

        try:
            state["llm_available"] = self.llm.is_available()
        except Exception:
            state["llm_available"] = False

        return state

    # ------------------------------------------------------------------ #
    #  DM-based communication (replaces server channel posts)             #
    # ------------------------------------------------------------------ #

    async def _handle_alerts(self, alerts: list[str], system_state: dict[str, Any]):
        logger.warning("Heiwa Agent alerts: %s", alerts)
        lines = "\n".join(f"- {a}" for a in alerts[:10])
        await self._dm(f"Heads up — caught some issues:\n{lines}\n\nLet me know if you want me to dig into any of these.")

    async def _broadcast_status(self, system_state: dict[str, Any]):
        """Conversational status update to the operator."""
        nodes = system_state.get("nodes_online", 0)
        tasks_5m = system_state.get("tasks_5m", 0)
        success_rate = system_state.get("task_success_rate_5m")
        llm_ok = system_state.get("llm_available", False)
        uptime = system_state.get("uptime_sec", 0)

        uptime_min = int(uptime / 60)

        parts = []

        # Fleet
        if nodes == 0:
            parts.append("No nodes registered yet — fleet is cold.")
        else:
            parts.append(f"{nodes} node{'s' if nodes != 1 else ''} online.")

        # Tasks
        if tasks_5m == 0:
            parts.append("Quiet last 5 minutes — no tasks in flight.")
        else:
            rate_str = f" ({int(success_rate * 100)}% pass rate)" if success_rate is not None else ""
            parts.append(f"{tasks_5m} task{'s' if tasks_5m != 1 else ''} in the last 5m{rate_str}.")

        # LLM
        if not llm_ok:
            parts.append("Local LLM is down — tasks will route to cloud providers.")

        # Uptime
        parts.append(f"I've been up {uptime_min}m, seen {self._tasks_seen} tasks total, {self._errors_seen} errors.")

        status_msg = " ".join(parts)
        logger.info("Heiwa Agent status: %s", status_msg)
        await self._dm(status_msg)

    async def _run_periodic_audit(self):
        result = await self.auditor.run_audit()

        level = "INFO" if result.passed else "WARNING"
        logger.log(getattr(logging, level), "Repo Audit: %s", result.summary)

        if result.passed:
            await self._dm(f"Ran a repo audit — everything checks out. {result.summary}")
        else:
            await self._dm(
                f"Repo audit found issues: {result.summary}\n"
                f"Errors: {', '.join(result.errors[:5])}\n\n"
                f"I can create a repair directive to fix these. Want me to?"
            )
            # Still create the directive proactively
            await self._create_repair_directive(result)

    async def _create_repair_directive(self, result: Any):
        if not self.db.stdb:
            return
        payload = {
            "summary": result.summary,
            "errors": result.errors,
            "instruction": f"Fix repository issues: {result.summary}. Errors: {', '.join(result.errors)}"
        }
        success = self.db.stdb.insert_captain_directive(
            directive_type="repair_repo",
            payload=payload,
            priority=2
        )
        if success:
            logger.info("Created repair directive in STDB.")

    async def _tune_model_tiers(self):
        if not self.db.stdb or not self.memory:
            return

        logger.info("Starting model tier tuning...")
        try:
            tiers = self.db.stdb.get_model_tiers(enabled_only=False)
            updated_count = 0
            tuned_models = []

            for tier in tiers:
                model_id = tier["model_id"]
                stats = self.memory.get_execution_stats(model_id)

                if stats["count"] >= 3:
                    success = self.db.stdb.update_model_tier_stats(
                        model_id=model_id,
                        success_rate=stats["success_rate"],
                        avg_latency_ms=stats["avg_latency_ms"],
                        latency_p95_ms=stats["p95_latency_ms"]
                    )
                    if success:
                        updated_count += 1
                        tuned_models.append(f"`{model_id}` ({int(stats['success_rate'] * 100)}% / {stats['avg_latency_ms']}ms avg)")

            if updated_count > 0:
                logger.info("Model tuning complete. Updated %d model(s).", updated_count)
                await self._dm(
                    f"Tuned {updated_count} model tier{'s' if updated_count != 1 else ''} based on execution history:\n"
                    + "\n".join(f"- {m}" for m in tuned_models)
                )
        except Exception as e:
            logger.error("Failed to tune model tiers: %s", e)

    async def _run_maintenance(self):
        if not self.db.stdb:
            return
        logger.info("Starting system maintenance...")
        try:
            self.db.stdb.prune_knowledge_embeddings(ttl_hours=168)
            self.db.stdb.prune_execution_memory(keep_last=1000)
            logger.info("System maintenance complete.")
        except Exception as e:
            logger.error("Maintenance task failed: %s", e)

    # ------------------------------------------------------------------ #
    #  Boot message                                                        #
    # ------------------------------------------------------------------ #

    def _build_boot_message(self) -> str:
        """First message to the operator when the system comes online."""
        parts = ["Hey — I'm online. Here's what I see:"]

        # Check STDB
        if self.db.stdb:
            try:
                tiers = self.db.stdb.get_model_tiers(enabled_only=True)
                parts.append(f"- **{len(tiers)} models** loaded in the tier matrix")
            except Exception:
                parts.append("- Model tier matrix: couldn't read")
        else:
            parts.append("- No STDB connection — running degraded")

        # Check LLM
        try:
            if self.llm.is_available():
                parts.append("- Local LLM: up")
            else:
                parts.append("- Local LLM: down (tasks will route to cloud)")
        except Exception:
            parts.append("- Local LLM: can't reach Ollama")

        # Check fleet
        try:
            nodes = self.db.list_nodes()
            n = len(nodes) if isinstance(nodes, list) else 0
            parts.append(f"- Fleet: {n} node{'s' if n != 1 else ''}")
        except Exception:
            parts.append("- Fleet: no nodes yet")

        parts.append("\nDM me anytime — I'm watching everything and I'll keep you posted on what Heiwa's doing.")

        return "\n".join(parts)
