"""
HeiwaClaw — The Living Agent

HeiwaClaw is Heiwa's 24/7 always-on consciousness running on Railway.
It holds state, memory, and context. It observes the system from a machine
perspective — watching tasks flow, agents execute, models route, nodes
heartbeat — and narrates what it sees to the operator via Discord DM.

HeiwaClaw spawns work to the Class 3 team (Claude Code, Codex, Gemini,
Antigravity, Ollama) via OpenClaw, the spawning/dispatch mechanism.

This agent unifies what was previously split across HeiwaAgent (observation)
and ExecutorAgent (execution) into a single living entity.
"""

import asyncio
import json
import logging
import os
import sys
import time
from pathlib import Path
from typing import Any

from heiwa_hub.agents.base import BaseAgent
from heiwa_protocol.program import ExecutionProgram
from heiwa_protocol.protocol import Subject
from heiwa_protocol.routing import BROKER_ENVELOPE_VERSION, BrokerRouteResult
from heiwa_sdk import OpenClaw
from heiwa_sdk.db import Database
from heiwa_sdk.audit import RepoAuditor
from heiwa_sdk.memory import MemoryService
from heiwa_sdk.agent_memory import AgentMemory

logger = logging.getLogger("HeiwaClaw")

TICK_SEC = 60
STATUS_BROADCAST_INTERVAL_SEC = 1800    # 30 min
AUDIT_INTERVAL_SEC = 3600               # 1 hour
MODEL_TUNING_INTERVAL_SEC = 1800        # 30 min
PRUNE_INTERVAL_SEC = 3600


class HeiwaClawAgent(BaseAgent):
    """Unified living agent: observes, decides, executes, communicates.

    Merges observation/DM/memory (former HeiwaAgent) with task dispatch
    via OpenClaw (former ExecutorAgent) into a single always-on entity.
    """

    def __init__(self):
        super().__init__(name="heiwaclaw")
        self.db = Database()
        self.root = Path(__file__).resolve().parents[3]
        self.executor_runtime = self._resolve_runtime()

        # OpenClaw: how we spawn work to Claude, Codex, Gemini, Antigravity, Ollama
        self.gateway = OpenClaw(self.root)

        # Observation + memory
        self.auditor = RepoAuditor(self.root)
        self.memory = MemoryService(stdb=self.db.stdb) if self.db.stdb else None
        self.agent_memory = AgentMemory(stdb=self.db.stdb) if self.db.stdb else None
        self._llm = None

        # Observation state
        self._last_status_broadcast = 0.0
        self._last_audit = time.time()  # Delay first audit
        self._last_tuning = 0.0
        self._last_prune = 0.0
        self._task_results: list[dict[str, Any]] = []
        self._pending_alerts: list[str] = []
        self._boot_time = time.time()
        self._tasks_seen = 0
        self._errors_seen = 0
        self._boot_context: dict | None = None

    @property
    def llm(self):
        if self._llm is None:
            from heiwa_cognition.llm import LocalLLMEngine
            self._llm = LocalLLMEngine()
        return self._llm

    async def run(self):
        await self.start()

        # -- Observation subjects (from HeiwaAgent) --
        await self.listen(Subject.TASK_EXEC_RESULT, self._on_task_result)
        await self.listen(Subject.TASK_STATUS, self._on_task_status)
        await self.listen(Subject.TASK_INGRESS, self._on_task_ingress)
        await self.listen(Subject.HEIWA_AGENT_INGRESS, self._on_direct_dm)
        await self.listen(Subject.NODE_HEARTBEAT, self._on_heartbeat)
        await self.listen(Subject.LOG_ERROR, self._on_error)

        # -- Execution subjects (from ExecutorAgent) --
        await self.listen(Subject.TASK_EXEC, self._handle_exec)
        await self.listen(Subject.TASK_EXEC_REQUEST_CODE, self._handle_exec)
        await self.listen(Subject.TASK_EXEC_REQUEST_RESEARCH, self._handle_exec)
        await self.listen(Subject.CAPTAIN_DIRECTIVE, self._handle_directive)

        logger.info(
            "HeiwaClaw online (%s). Observing system, ready to spawn work via OpenClaw.",
            self.executor_runtime,
        )

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
                await asyncio.sleep(TICK_SEC)
        except KeyboardInterrupt:
            await self.shutdown()

    # ================================================================== #
    #  DM channel — all operator comms go through here                   #
    # ================================================================== #

    async def _dm(self, content: str):
        """Send a message to the operator via DM."""
        self._store_agent_response(content)
        await self.speak(Subject.HEIWA_AGENT_DM, {
            "agent": "heiwaclaw",
            "content": content,
        })

    # ================================================================== #
    #  Observation handlers                                               #
    # ================================================================== #

    async def _on_direct_dm(self, data: dict[str, Any]):
        """Direct operator interaction via DM — routes through ChatEngine."""
        payload = data.get("data", data)
        content = payload.get("content", "")
        author = payload.get("author", "operator")

        logger.info("DM from %s: %s", author, content[:80])

        loop = asyncio.get_event_loop()
        loop.run_in_executor(None, self._store_operator_message, content, "discord_dm")

        try:
            from heiwa_hub.chat import get_chat_engine
            engine = get_chat_engine()
            session_id = f"discord-dm-{author}"
            reply = await engine.respond(
                session_id=session_id,
                content=content,
                author=author,
            )
            await self._dm(reply)
        except Exception as e:
            logger.error("Chat engine DM response failed: %s", e)
            await self._dm("I caught your message, but I'm having trouble thinking clearly right now.")

    async def _on_task_ingress(self, data: dict[str, Any]):
        """A new task entered the system. Narrate what we see."""
        payload = data.get("data", data)
        task_id = payload.get("task_id", "?")
        raw = str(payload.get("raw_text", ""))[:120]
        intent = payload.get("intent_class", "unknown")
        source = payload.get("source", "unknown")
        is_dm = payload.get("is_dm", False)
        self._tasks_seen += 1

        self._store_operator_message(f"[task:{task_id}] {raw}", source=source)

        if not is_dm and intent != "chat":
            await self._dm(
                f"New task landed from **{source}**.\n"
                f"`{task_id}` classified as **{intent}**\n"
                f"> {raw}\n"
                f"Routing it now — I'll tell you what happens."
            )

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

        if intent == "chat":
            return

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

        if status != "PASS":
            msg += "\n\nThis one didn't land. Want me to retry with a different route, or should we look at it together?"

        await self._dm(msg)

    async def _on_task_status(self, data: dict[str, Any]):
        """Watch for blocked or failed tasks — alert the operator."""
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

    # ================================================================== #
    #  Execution handlers (OpenClaw dispatch)                             #
    # ================================================================== #

    async def _handle_exec(self, data: dict[str, Any]) -> None:
        """Execute a task via OpenClaw — spawn work to the Class 3 team."""
        payload = data.get("data", data)
        task_id = payload.get("task_id", "unknown")
        instruction = payload.get("instruction", "")
        intent_class = payload.get("intent_class", "general")
        target_runtime = payload.get("target_runtime", "railway").lower()
        target_model = payload.get("target_model", "")
        target_tool = str(payload.get("target_tool", "heiwa_claw")).strip().lower() or "heiwa_claw"

        if target_runtime not in {self.executor_runtime, "both", "any"}:
            logger.info("Skipping task %s: target=%s, local=%s", task_id, target_runtime, self.executor_runtime)
            return

        route = BrokerRouteResult.from_payload({
            "request_id": payload.get("request_id", f"exec-{task_id}"),
            "task_id": task_id,
            "envelope_version": payload.get("envelope_version", BROKER_ENVELOPE_VERSION),
            "raw_text": payload.get("raw_text", instruction),
            "source_surface": payload.get("source_surface", "heiwaclaw"),
            "intent_class": intent_class,
            "risk_level": payload.get("risk_level", "low"),
            "privacy_level": payload.get("privacy_level"),
            "compute_class": payload.get("compute_class", 1),
            "assigned_worker": payload.get("assigned_worker", ""),
            "target_tool": target_tool,
            "target_model": target_model,
            "target_runtime": target_runtime,
            "target_tier": payload.get("target_tier", "tier1_local"),
            "requires_approval": payload.get("requires_approval", False),
            "rationale": payload.get("rationale", ""),
            "normalization": payload.get("normalization", {}),
        })
        dispatch = self.gateway.resolve(route)

        execution_program = None
        prog_data = payload.get("execution_program")
        if prog_data:
            execution_program = ExecutionProgram.from_dict(prog_data)

        logger.info("Processing task: %s | Intent: %s", task_id, intent_class)
        await self.speak(Subject.TASK_STATUS, {
            "accepted": True,
            "reason": None,
            "task_id": task_id,
            "step_id": payload.get("step_id", "heiwaclaw"),
            "status": "RUNNING",
            "message": "HeiwaClaw started task execution via OpenClaw.",
            "runtime": self.executor_runtime,
            "response_channel_id": payload.get("response_channel_id"),
            "response_thread_id": payload.get("response_thread_id"),
        })

        start = time.time()
        full_result = ""
        exec_status = "FAIL"

        try:
            if dispatch.direct_execution and target_tool == "heiwa_ops" and intent_class == "audit":
                logger.info("Routing %s through bounded Class 1 audit.", task_id)
                exec_status, full_result = await self._run_bounded_audit(instruction)
            else:
                logger.info(
                    "Routing %s through OpenClaw (%s -> %s).",
                    task_id, dispatch.provider, dispatch.adapter_tool,
                )
                code, output = await self.gateway.execute(route, instruction)
                exec_status = "PASS" if code == 0 else "FAIL"
                full_result = output.strip()
        except Exception as e:
            logger.error("Task %s FAILED: %s", task_id, e)
            full_result = f"Error: {e}"
            exec_status = "FAIL"

        elapsed = round(time.time() - start, 2)
        logger.info("Task %s finished in %ss.", task_id, elapsed)

        # Advisory: validate against execution program acceptance criteria
        # V1: does NOT override exec_status — reported separately
        program_validation = self._validate_acceptance(execution_program, full_result)
        if execution_program and execution_program.is_bounded() and not program_validation["passed"]:
            logger.warning(
                "Task %s: advisory acceptance unmet: %s",
                task_id, program_validation["unmatched"],
            )

        result_payload = {
            "task_id": task_id,
            "status": exec_status,
            "summary": full_result,
            "runtime": self.executor_runtime,
            "elapsed_sec": elapsed,
            "target_tool": dispatch.gateway_tool,
            "adapter_tool": dispatch.adapter_tool,
            "provider": dispatch.provider,
            "target_model": route.target_model,
            "intent_class": intent_class,
            "requested_by": payload.get("requested_by"),
            "response_channel_id": payload.get("response_channel_id"),
            "response_thread_id": payload.get("response_thread_id"),
            "program_validation": program_validation,
            "execution_program": execution_program.to_dict() if execution_program else None,
        }

        await self.speak(Subject.TASK_EXEC_RESULT, result_payload)

        # Record in execution memory
        if self.memory:
            try:
                self.memory.record_execution(
                    task_id=task_id,
                    model=route.target_model,
                    outcome=exec_status.lower(),
                    duration_ms=int(elapsed * 1000),
                    error=full_result if exec_status == "FAIL" else None,
                    execution_program_json=json.dumps(execution_program.to_dict()) if execution_program else None,
                )
            except Exception as e:
                logger.error("Failed to record execution memory: %s", e)

        await self.speak(Subject.TASK_STATUS, {
            "accepted": exec_status == "PASS",
            "reason": None if exec_status == "PASS" else "Execution produced a failure result.",
            "task_id": task_id,
            "step_id": payload.get("step_id", "heiwaclaw"),
            "status": "DELIVERED",
            "message": f"Execution result published ({exec_status}).",
            "runtime": self.executor_runtime,
            "target_tool": dispatch.gateway_tool,
            "response_channel_id": payload.get("response_channel_id"),
            "response_thread_id": payload.get("response_thread_id"),
        })

    async def _handle_directive(self, data: dict[str, Any]) -> None:
        """Handle proactive directives (e.g., repair_repo)."""
        payload = data.get("data", data)
        dtype = payload.get("directive_type")

        if dtype == "repair_repo":
            instruction = payload.get("instruction", "Fix repository issues.")
            logger.info("Executing repair directive: %s", instruction)
            repair_task = {
                "task_id": f"repair-{int(time.time())}",
                "instruction": instruction,
                "intent_class": "build",
                "risk_level": "high",
                "source_surface": "heiwaclaw",
                "target_runtime": "macbook",
            }
            await self._handle_exec({"data": repair_task})

    # ================================================================== #
    #  Memory loop                                                        #
    # ================================================================== #

    def _store_operator_message(self, content: str, source: str = "discord_dm"):
        if self.agent_memory:
            self.agent_memory.store_message(role="operator", content=content, source=source)

    def _store_agent_response(self, content: str):
        if self.agent_memory:
            self.agent_memory.store_message(role="agent", content=content, source="system")

    async def _maybe_compress(self):
        if not self.agent_memory:
            return
        ctx = self.agent_memory.load_context_window()
        messages = ctx.get("messages", [])
        if self.agent_memory.needs_compression(messages):
            await self._run_rolling_compression(messages)

    async def _run_rolling_compression(self, messages: list[dict]):
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
        logger.info("Rolling compression: %d messages -> summary", len(to_compress))

    async def _llm_summarize(self, text: str) -> str:
        prompt = (
            "Summarize this conversation concisely, preserving key decisions, "
            "action items, and technical details:\n\n" + text
        )
        return await self.llm.generate_async(prompt=prompt, complexity="low")

    def _hydrate_boot_context(self) -> dict:
        if not self.agent_memory:
            return {"messages": [], "focuses": [], "summaries": []}
        return self.agent_memory.load_context_window()

    def _should_cascade(self, text: str) -> bool:
        if not self.agent_memory:
            return False
        return self.agent_memory.detect_complexity(text)

    def _update_focus(self, topic: str, context: dict[str, Any], priority: int = 3):
        if not self.agent_memory:
            return
        active = self.agent_memory.stdb.get_active_focuses()
        existing = next((f for f in active if f.get("topic") == topic), None)
        if existing:
            self.agent_memory.upsert_focus(
                topic=topic, context=context, priority=priority,
                focus_id=existing["focus_id"],
            )
        else:
            self.agent_memory.upsert_focus(
                topic=topic, context=context, priority=priority,
            )

    # ================================================================== #
    #  Proactive tick loop                                                #
    # ================================================================== #

    async def _agent_tick(self):
        now = time.time()
        system_state = await self._gather_system_state()

        try:
            await self._maybe_compress()
        except Exception as e:
            logger.warning("Memory compression failed: %s", e)

        alerts = list(self._pending_alerts)
        self._pending_alerts.clear()
        if alerts:
            await self._handle_alerts(alerts, system_state)

        if now - self._last_audit > AUDIT_INTERVAL_SEC:
            if self._has_background_capacity():
                asyncio.create_task(self._run_periodic_audit())
            else:
                logger.debug("Skipping periodic audit — LLM rate capacity low")
            self._last_audit = now

        if now - self._last_tuning > MODEL_TUNING_INTERVAL_SEC:
            asyncio.create_task(self._tune_model_tiers())
            self._last_tuning = now

        if now - self._last_prune > PRUNE_INTERVAL_SEC:
            asyncio.create_task(self._run_maintenance())
            self._last_prune = now

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

    # ================================================================== #
    #  Communication                                                      #
    # ================================================================== #

    async def _handle_alerts(self, alerts: list[str], system_state: dict[str, Any]):
        logger.warning("HeiwaClaw alerts: %s", alerts)
        lines = "\n".join(f"- {a}" for a in alerts[:10])
        await self._dm(f"Heads up — caught some issues:\n{lines}\n\nLet me know if you want me to dig into any of these.")

    async def _broadcast_status(self, system_state: dict[str, Any]):
        tasks_5m = system_state.get("tasks_5m", 0)
        success_rate = system_state.get("task_success_rate_5m")
        llm_ok = system_state.get("llm_available", False)

        parts: list[str] = []

        if tasks_5m > 0:
            rate_str = f" ({int(success_rate * 100)}% pass rate)" if success_rate is not None else ""
            parts.append(f"{tasks_5m} task{'s' if tasks_5m != 1 else ''} recently{rate_str}.")

        if not llm_ok:
            parts.append("LLM providers are down — routing to cloud fallbacks.")

        if self._errors_seen > 0:
            parts.append(f"{self._errors_seen} errors caught since boot.")

        if not parts:
            logger.debug("Status check: all quiet, skipping DM")
            return

        status_msg = " ".join(parts)
        logger.info("HeiwaClaw status: %s", status_msg)
        await self._dm(status_msg)

    # ================================================================== #
    #  Periodic tasks                                                     #
    # ================================================================== #

    async def _run_periodic_audit(self):
        result = await self.auditor.run_audit()
        level = "INFO" if result.passed else "WARNING"
        logger.log(getattr(logging, level), "Repo Audit: %s", result.summary)

        if result.passed:
            logger.info("Repo audit passed: %s", result.summary)
        else:
            await self._dm(
                f"Repo audit found issues: {result.summary}\n"
                f"Errors: {', '.join(result.errors[:5])}\n\n"
                f"I can create a repair directive to fix these. Want me to?"
            )
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
                logger.info(
                    "Model tuning complete. Updated %d model(s): %s",
                    updated_count, ", ".join(tuned_models),
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

    # ================================================================== #
    #  Utilities                                                          #
    # ================================================================== #

    def _validate_acceptance(
        self,
        program: "ExecutionProgram | None",
        output: str,
    ) -> dict[str, Any]:
        """Advisory check: execution output against program acceptance criteria.

        V1: This is ADVISORY ONLY. It does not override exec_status.
        Returns dict with: passed (bool), matched (list), unmatched (list).
        """
        if not program or not program.acceptance:
            return {"passed": True, "matched": [], "unmatched": []}

        output_lower = output.lower()
        matched = []
        unmatched = []
        for criterion in program.acceptance:
            if criterion.lower() in output_lower:
                matched.append(criterion)
            else:
                unmatched.append(criterion)

        return {
            "passed": len(unmatched) == 0,
            "matched": matched,
            "unmatched": unmatched,
        }

    @staticmethod
    def _has_background_capacity() -> bool:
        try:
            from heiwa_sdk.rate_ledger import get_rate_ledger
            ledger = get_rate_ledger()
            return ledger.can_run_background("google_gemini_api")
        except Exception:
            return True

    def _resolve_runtime(self) -> str:
        explicit = str(os.getenv("HEIWA_EXECUTOR_RUNTIME", "")).strip().lower()
        if explicit:
            return explicit
        if os.getenv("RAILWAY_ENVIRONMENT") or os.getenv("RAILWAY_PROJECT_ID"):
            return "railway"
        return "macbook"

    async def _run_bounded_audit(self, instruction: str) -> tuple[str, str]:
        script = self.root / "apps/heiwa_cli/scripts/lint_config.py"
        if not script.exists():
            return "FAIL", f"Missing audit script: {script}"
        proc = await asyncio.create_subprocess_exec(
            sys.executable, str(script),
            cwd=str(self.root),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()
        combined = (stdout + stderr).decode(errors="ignore").strip()
        if not combined and proc.returncode == 0:
            combined = "PASS: lint_config"
        status = "PASS" if proc.returncode == 0 else "FAIL"
        probe_marker = self._extract_smoke_probe(instruction)
        summary = f"[CLASS1_AUDIT] lint_config.py\n[EXIT_CODE] {proc.returncode}\n{combined}".strip()
        if probe_marker:
            summary = f"{summary}\nHEIWA_SMOKE_PROBE_OK:{probe_marker}"
        return status, summary

    @staticmethod
    def _extract_smoke_probe(instruction: str) -> str:
        marker = "HEIWA_SMOKE_PROBE:"
        if marker not in instruction:
            return ""
        suffix = instruction.split(marker, 1)[1].strip()
        return suffix.split()[0] if suffix else ""

    def _build_boot_message(self) -> str:
        parts = ["HeiwaClaw online. Here's what I see:"]

        if self.db.stdb:
            try:
                tiers = self.db.stdb.get_model_tiers(enabled_only=True)
                parts.append(f"- **{len(tiers)} models** loaded in the tier matrix")
            except Exception:
                parts.append("- Model tier matrix: couldn't read")
        else:
            parts.append("- No STDB connection — running degraded")

        try:
            if self.llm.is_available():
                parts.append("- Local LLM: up")
            else:
                parts.append("- Local LLM: down (tasks will route to cloud)")
        except Exception:
            parts.append("- Local LLM: can't reach Ollama")

        try:
            nodes = self.db.list_nodes()
            n = len(nodes) if isinstance(nodes, list) else 0
            parts.append(f"- Fleet: {n} node{'s' if n != 1 else ''}")
        except Exception:
            parts.append("- Fleet: no nodes yet")

        parts.append(f"- Runtime: **{self.executor_runtime}**")
        parts.append(f"- OpenClaw adapters: {len(self.gateway._adapters)}")
        parts.append("\nDM me anytime — I'm watching everything and spawning work to the team.")

        return "\n".join(parts)
