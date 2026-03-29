import asyncio
import datetime
import logging
import time
import uuid
from typing import Dict

from heiwa_hub.agents.base import BaseAgent
from heiwa_hub.cognition.approval import auto_approved, get_approval_registry
from heiwa_hub.cognition.planner import LocalTaskPlanner
from heiwa_hub.cognition.enrichment import BrokerEnrichmentService
from heiwa_hub.cognition.intent_normalizer import IntentProfile
from heiwa_hub.transport import get_worker_manager
from heiwa_protocol.protocol import Subject
from heiwa_protocol.routing import BROKER_ENVELOPE_VERSION, BrokerRouteRequest, BrokerRouteResult

logger = logging.getLogger("Spine")


class SpineAgent(BaseAgent):
    def __init__(self):
        super().__init__(name="heiwa-spine")
        self.fleet_registry: Dict[str, float] = {}
        self.planner = LocalTaskPlanner()
        self.enrichment = BrokerEnrichmentService()
        self.approvals = get_approval_registry()
        self._approval_timers: Dict[str, asyncio.Task] = {}

    async def run(self):
        await self.start()

        await self.listen(Subject.NODE_HEARTBEAT, self.handle_heartbeat)
        await self.listen(Subject.CORE_REQUEST, self.handle_request)
        await self.listen(Subject.TASK_NEW, self.handle_request)
        await self.listen(Subject.TASK_APPROVAL_DECISION, self.handle_approval_decision)

        logger.info("Spine active. Monitoring fleet...")

        fleet_log_counter = 0
        watchdog_counter = 0
        try:
            while self.running:
                self._prune_registry()
                self.approvals.prune()
                
                # Audit Watchdog: reconcile stale assignments every 60s
                watchdog_counter += 1
                if watchdog_counter >= 6:  # Tick is 10s, 6 ticks = 60s
                    await self._audit_watchdog()
                    watchdog_counter = 0

                fleet_log_counter += 1
                if self.fleet_registry and fleet_log_counter >= 30:  # Log every 5 min (30 × 10s)
                    logger.info("Fleet: %d active node(s).", len(self.fleet_registry))
                    fleet_log_counter = 0
                await asyncio.sleep(10)
        except KeyboardInterrupt:
            await self.shutdown()

    async def _audit_watchdog(self):
        """Scan STDB for assigned tasks that never got claimed (stale assignment)."""
        try:
            from heiwa_sdk.db import db
            stale = db.stdb.get_stale_proposals()
            if not stale:
                return

            logger.info("Watchdog found %d stale proposal(s). Requeueing...", len(stale))
            for prop in stale:
                pid = prop["proposal_id"]
                node = prop.get("assigned_node_id", "unknown")
                logger.warning("Requeueing %s (assigned to %s but expired)", pid, node)
                db.stdb.requeue_proposal(pid, reason=f"Assignment expired on node {node}")
                
        except Exception as e:
            logger.error("Audit Watchdog error: %s", e)

    async def handle_heartbeat(self, data: dict):
        sender = data.get("sender_id")
        if sender:
            if sender not in self.fleet_registry:
                logger.info("New node detected: %s", sender)
            self.fleet_registry[sender] = time.time()

    async def handle_request(self, data: dict):
        """Receive a task envelope, verify auth, enrich via broker, dispatch."""
        from heiwa_sdk.config import settings
        from heiwa_hub.envelope import extract_auth_token, extract_payload, normalize_sender

        from heiwa_sdk.security import SecurityService
        ss = SecurityService()
        
        if not ss.get_expected_token():
            logger.error("Digital Barrier misconfigured: HEIWA_AUTH_TOKEN not set.")
            return

        sender_id = normalize_sender(data)
        auth_token = extract_auth_token(data)
        if not ss.validate_token(auth_token):
            logger.warning("Digital Barrier: invalid token from %s", sender_id)
            await self.speak(Subject.TASK_STATUS, {
                "accepted": False,
                "reason": "Invalid or missing auth token.",
                "task_id": data.get("task_id", data.get("data", {}).get("task_id", "unknown")),
                "step_id": "spine-orchestrator",
                "status": "BLOCKED_AUTH",
                "message": "Invalid or missing auth token.",
                "runtime": "spine",
            })
            return

        payload = extract_payload(data)
        task_id = payload.get("task_id", f"task-{int(time.time())}")
        dispatch_status_code = "DISPATCHED_PLAN"
        payload["source_surface"] = payload.get("source_surface") or payload.get("source") or "cli"
        payload["requested_by"] = payload.get("requested_by") or sender_id

        # --- BROKER ENRICHMENT (skip if already enriched by /tasks endpoint) ---
        if payload.get("_pre_enriched"):
            logger.info("Task %s already enriched by ingress.", task_id)
        elif not payload.get("steps") and payload.get("raw_text"):
            try:
                route = await self._enrich_via_broker(payload, task_id, sender_id)
                payload.update(route.to_dict())
                logger.info("Broker enriched task %s.", task_id)
            except Exception as e:
                logger.warning("Broker enrichment failed for %s: %s. Falling back inline.", task_id, e)

        # Save enrichment fields that TaskPlan.to_dict() would overwrite
        _saved_execution_program = payload.get("execution_program")
        _saved_route_fields = {
            k: payload[k] for k in (
                "target_model", "assigned_worker", "compute_class",
                "target_tool", "target_tier", "target_runtime",
            ) if k in payload and payload[k]
        }

        # --- INLINE PLANNING FALLBACK ---
        if not payload.get("steps") and payload.get("raw_text"):
            logger.info("Planning raw request for task %s...", task_id)
            try:
                profile = None
                normalization = payload.get("normalization")
                if isinstance(normalization, dict) and normalization:
                    profile = IntentProfile.from_dict(normalization)
                task_plan = self.planner.plan(
                    task_id=task_id,
                    raw_text=payload.get("raw_text"),
                    requested_by=sender_id,
                    source_channel_id=payload.get("source", "cli"),
                    source_message_id=task_id,
                    response_channel_id=payload.get("response_channel_id", "cli"),
                    response_thread_id=payload.get("response_thread_id"),
                    intent_profile=profile,
                )
                payload = task_plan.to_dict()
                # Restore enrichment route fields lost by TaskPlan.to_dict() overwrite
                if _saved_execution_program:
                    payload["execution_program"] = _saved_execution_program
                payload.update(_saved_route_fields)
                dispatch_status_code = "DISPATCHED_PLAN"
            except Exception as e:
                logger.error("Planning failed: %s. Direct fallback.", e)
                payload["steps"] = [{
                    "step_id": "fallback-exec",
                    "instruction": payload.get("raw_text"),
                    "subject": Subject.TASK_EXEC.value,
                    "target_runtime": "any",
                }]
                dispatch_status_code = "DISPATCHED_FALLBACK"

        intent = payload.get("intent_class", "unknown")
        logger.info("Received task envelope: %s (Task: %s)", intent, task_id)

        # ACK
        await self._emit_task_status(
            payload,
            task_id=task_id,
            step_id="spine-orchestrator",
            status="ACKNOWLEDGED",
            message=f"Spine accepted '{task_id}' for '{intent}'.",
            accepted=True,
        )

        # --- DISPATCH STEPS ---
        steps = payload.get("steps") or []
        if not isinstance(steps, list) or not steps:
            raw_text = payload.get("raw_text") or data.get("raw_text")
            if raw_text:
                steps = [{
                    "step_id": "auto-reflex",
                    "instruction": raw_text,
                    "subject": Subject.TASK_EXEC.value,
                    "target_runtime": "any",
                    "target_tool": "heiwa_claw",
                }]
                dispatch_status_code = "DISPATCHED_FALLBACK"
            else:
                await self._emit_task_status(
                    payload,
                    task_id=task_id,
                    step_id="spine-orchestrator",
                    status="BLOCKED_NO_CONTENT",
                    message="No executable content found in task.",
                    accepted=False,
                    reason="No executable content found in task.",
                )
                return

        payload["steps"] = steps

        if self._requires_manual_approval(payload):
            await self._hold_for_approval(task_id, payload)
            return

        await self._dispatch_steps(payload, task_id, intent, dispatch_status_code)

    async def handle_approval_decision(self, data: dict):
        payload = data.get("data", data)
        task_id = str(payload.get("task_id") or "").strip()
        if not task_id:
            return

        prior_state = self.approvals.get_state(task_id)
        if not prior_state:
            await self._emit_task_status(
                payload,
                task_id=task_id,
                step_id="approval-gate",
                status="APPROVAL_NOT_FOUND",
                message=f"No pending approval found for {task_id}.",
                accepted=False,
                reason="Approval not found.",
            )
            return

        if prior_state.status == "PENDING":
            if payload.get("_state_applied"):
                state = prior_state
            else:
                approved = self._payload_is_approved(payload)
                actor = str(payload.get("actor") or payload.get("decision_by") or "operator")
                reason = payload.get("reason")
                state = self.approvals.decide(task_id, approved=approved, actor=actor, reason=reason)
        else:
            state = prior_state

        if not state:
            return

        self._cancel_approval_timer(task_id)
        held_payload = self.approvals.get_payload(task_id) or payload
        approval_id = str(held_payload.get("approval_id") or f"approval-{task_id}")
        self._persist_approval_terminal_state(task_id, approval_id, held_payload, state)

        if state.status == "APPROVED":
            exec_payload = self.approvals.consume_payload(task_id) or held_payload
            await self._emit_task_status(
                exec_payload,
                task_id=task_id,
                step_id="approval-gate",
                status="APPROVED",
                message=f"Approval granted for {task_id}. Resuming execution.",
                accepted=True,
            )
            await self._dispatch_steps(
                exec_payload,
                task_id,
                exec_payload.get("intent_class", "unknown"),
                "DISPATCHED_PLAN",
            )
            return

        if state.status == "REJECTED":
            self.approvals.consume_payload(task_id)
            await self._emit_task_status(
                held_payload,
                task_id=task_id,
                step_id="approval-gate",
                status="REJECTED",
                message=f"Approval rejected for {task_id}.",
                accepted=False,
                reason=state.reason or "Rejected by operator.",
            )
            return

        if state.status == "EXPIRED":
            self.approvals.consume_payload(task_id)
            await self._emit_task_status(
                held_payload,
                task_id=task_id,
                step_id="approval-gate",
                status="EXPIRED",
                message=f"Approval expired for {task_id}.",
                accepted=False,
                reason=state.reason or "Approval timed out.",
            )

    async def _dispatch_steps(
        self,
        payload: dict,
        task_id: str,
        intent: str,
        dispatch_status_code: str,
    ) -> None:
        steps = payload.get("steps") or []
        for step in steps:
            if not isinstance(step, dict):
                continue
            step_id = str(step.get("step_id", "unknown"))

            exec_payload = {
                "task_id": task_id,
                "plan_id": payload.get("plan_id"),
                "approval_id": payload.get("approval_id"),
                "step_id": step_id,
                "instruction": step.get("instruction", payload.get("raw_text", "")),
                "intent_class": payload.get("intent_class", intent),
                "risk_level": payload.get("risk_level"),
                "privacy_level": payload.get("privacy_level"),
                "requested_by": payload.get("requested_by"),
                "target_runtime": step.get("target_runtime", payload.get("target_runtime", "railway")),
                "target_tool": step.get("target_tool", payload.get("target_tool", "heiwa_claw")),
                "target_model": step.get("target_model", payload.get("target_model", "")),
                "target_tier": step.get("target_tier", payload.get("target_tier")),
                "compute_class": payload.get("compute_class"),
                "assigned_worker": payload.get("assigned_worker"),
                "requires_approval": payload.get("requires_approval"),
                "response_channel_id": payload.get("response_channel_id"),
                "response_thread_id": payload.get("response_thread_id"),
                "raw_text": payload.get("raw_text"),
                "normalization": payload.get("normalization"),
                "envelope_version": payload.get("envelope_version"),
                "execution_program": payload.get("execution_program"),
            }

            # Try remote worker if task targets a non-Railway runtime
            dispatched_remote = False
            target_rt = exec_payload.get("target_runtime", "railway")
            assigned = exec_payload.get("assigned_worker")
            if target_rt not in {"railway", "cloud"} or assigned:
                wm = get_worker_manager()
                worker_id = assigned or wm.get_worker_for_runtime(target_rt)
                if worker_id:
                    pushed = await wm.push_task(worker_id, exec_payload)
                    if pushed:
                        dispatched_remote = True
                        logger.info("Dispatched %s/%s to remote worker %s", task_id, step_id, worker_id)

            if not dispatched_remote:
                await self.speak(Subject.TASK_EXEC, exec_payload)
                logger.info("Dispatched %s/%s to local executor", task_id, step_id)

            await self._emit_task_status(
                payload,
                task_id=task_id,
                step_id=step_id,
                status=dispatch_status_code,
                message=f"Spine dispatched step {step_id}.",
                accepted=True,
            )

    def _requires_manual_approval(self, payload: dict) -> bool:
        if not payload.get("requires_approval"):
            return False
        source_surface = payload.get("source_surface") or payload.get("source") or "cli"
        risk_level = payload.get("risk_level") or "low"
        return not auto_approved(str(source_surface), str(risk_level))

    async def _hold_for_approval(self, task_id: str, payload: dict) -> None:
        approval_id = str(payload.get("approval_id") or f"approval-{task_id}")
        stored_payload = dict(payload)
        stored_payload["approval_id"] = approval_id
        state = self.approvals.add(task_id, stored_payload)
        self._persist_approval_request(task_id, approval_id, stored_payload, state)

        await self.speak(Subject.TASK_APPROVAL_REQUEST, {
            "task_id": task_id,
            "approval_id": approval_id,
            "status": state.status,
            "risk_level": stored_payload.get("risk_level"),
            "source_surface": stored_payload.get("source_surface"),
            "requested_by": stored_payload.get("requested_by"),
            "raw_text_excerpt": str(stored_payload.get("raw_text") or "")[:200],
            "expires_at": state.expires_at,
            "response_channel_id": stored_payload.get("response_channel_id"),
            "response_thread_id": stored_payload.get("response_thread_id"),
        })
        await self._emit_task_status(
            stored_payload,
            task_id=task_id,
            step_id="approval-gate",
            status="AWAITING_APPROVAL",
            message=f"Task {task_id} is awaiting approval.",
            accepted=True,
        )
        self._schedule_approval_timeout(task_id)

    def _schedule_approval_timeout(self, task_id: str) -> None:
        self._cancel_approval_timer(task_id)
        state = self.approvals.get_state(task_id)
        if not state:
            return

        async def _timeout_watch() -> None:
            await asyncio.sleep(max(0.0, state.expires_at - time.time()))
            current = self.approvals.expire(task_id)
            if not current or current.status != "EXPIRED":
                return
            held_payload = self.approvals.consume_payload(task_id) or {}
            approval_id = str(held_payload.get("approval_id") or f"approval-{task_id}")
            self._persist_approval_terminal_state(task_id, approval_id, held_payload, current)
            await self._emit_task_status(
                held_payload,
                task_id=task_id,
                step_id="approval-gate",
                status="EXPIRED",
                message=f"Approval expired for {task_id}.",
                accepted=False,
                reason=current.reason or "Approval timed out.",
            )

        self._approval_timers[task_id] = asyncio.create_task(_timeout_watch())

    def _cancel_approval_timer(self, task_id: str) -> None:
        timer = self._approval_timers.pop(task_id, None)
        if timer and not timer.done():
            timer.cancel()

    @staticmethod
    def _approval_times(
        created_at: float | None = None,
        expires_at: float | None = None,
        decision_at: float | None = None,
    ) -> dict[str, str | None]:
        def _to_iso(value: float | None) -> str | None:
            if value is None:
                return None
            return datetime.datetime.fromtimestamp(
                value,
                tz=datetime.timezone.utc,
            ).isoformat()

        return {
            "created_at": _to_iso(created_at),
            "expires_at": _to_iso(expires_at),
            "decision_at": _to_iso(decision_at),
        }

    @staticmethod
    def _approval_view_payload(payload: dict) -> dict:
        return {
            "risk_level": payload.get("risk_level"),
            "source_surface": payload.get("source_surface"),
            "requested_by": payload.get("requested_by"),
            "raw_text": payload.get("raw_text"),
        }

    def _persist_approval_request(self, task_id: str, approval_id: str, payload: dict, state) -> None:
        try:
            from heiwa_sdk.db import db as state_db

            if not getattr(state_db, "stdb", None):
                return
            times = self._approval_times(state.created_at, state.expires_at, state.decision_at)
            state_db.add_approval_request(
                {
                    "request_id": approval_id,
                    "proposal_id": task_id,
                    "status": state.status,
                    "requested_at": times["created_at"],
                    "expires_at": times["expires_at"],
                    "requested_by": payload.get("requested_by", "heiwa-hub"),
                    "reason": state.reason or "manual_approval_required",
                    "payload": self._approval_view_payload(payload),
                }
            )
        except Exception as exc:
            logger.warning("Failed to persist approval request for %s: %s", task_id, exc)

    def _persist_approval_terminal_state(self, task_id: str, approval_id: str, payload: dict, state) -> None:
        try:
            from heiwa_sdk.db import db as state_db

            if not getattr(state_db, "stdb", None):
                return
            times = self._approval_times(state.created_at, state.expires_at, state.decision_at)
            state_db.add_approval_request(
                {
                    "request_id": approval_id,
                    "proposal_id": task_id,
                    "status": state.status,
                    "requested_at": times["created_at"],
                    "expires_at": times["expires_at"],
                    "requested_by": payload.get("requested_by", "heiwa-hub"),
                    "reason": state.reason,
                    "payload": self._approval_view_payload(payload),
                }
            )
            if state.status in {"APPROVED", "REJECTED"}:
                state_db.record_approval_decision(
                    {
                        "decision_id": f"DEC-{approval_id}-{int((state.decision_at or time.time()) * 1000)}",
                        "request_id": approval_id,
                        "proposal_id": task_id,
                        "actor_type": "human",
                        "actor_id": state.decision_by or "operator",
                        "decision": state.status,
                        "reason": state.reason,
                        "created_at": times["decision_at"],
                        "metadata": self._approval_view_payload(payload),
                    }
                )
        except Exception as exc:
            logger.warning("Failed to persist approval terminal state for %s: %s", task_id, exc)

    async def _emit_task_status(
        self,
        payload: dict,
        *,
        task_id: str,
        step_id: str,
        status: str,
        message: str,
        accepted: bool,
        reason: str | None = None,
    ) -> None:
        await self.speak(Subject.TASK_STATUS, {
            "accepted": accepted,
            "reason": reason,
            "task_id": task_id,
            "step_id": step_id,
            "status": status,
            "message": message,
            "runtime": "spine",
            "response_channel_id": payload.get("response_channel_id"),
            "response_thread_id": payload.get("response_thread_id"),
            "approval_id": payload.get("approval_id"),
            "owner_id": payload.get("owner_id"),
            "principal_id": payload.get("principal_id"),
            "session_id": payload.get("session_id"),
            "battlefield_id": payload.get("battlefield_id"),
        })

    @staticmethod
    def _payload_is_approved(payload: dict) -> bool:
        if "approved" in payload:
            return bool(payload.get("approved"))
        decision = str(payload.get("decision") or "").strip().lower()
        return decision in {"approve", "approved", "true", "1", "yes"}

    async def _enrich_via_broker(self, payload: dict, task_id: str, sender_id: str) -> BrokerRouteResult:
        """Direct call to BrokerEnrichmentService."""
        request_id = f"broker-{task_id}-{uuid.uuid4().hex[:8]}"
        request = BrokerRouteRequest(
            request_id=request_id,
            task_id=task_id,
            raw_text=payload.get("raw_text", ""),
            sender_id=sender_id,
            source_surface=payload.get("source", "cli"),
            response_channel_id=payload.get("response_channel_id", "cli"),
            response_thread_id=payload.get("response_thread_id"),
            auth_validated=True,
            timestamp=time.time(),
            envelope_version=BROKER_ENVELOPE_VERSION,
            privacy_level=payload.get("privacy_level"),
        )
        result = await self.enrichment.enrich(request)
        if result.error:
            raise RuntimeError(f"{result.error}: {result.message or ''}".strip())
        return result

    def _prune_registry(self):
        now = time.time()
        dead = [nid for nid, last_seen in self.fleet_registry.items() if now - last_seen > 30.0]
        for nid in dead:
            logger.warning("Node lost: %s", nid)
            del self.fleet_registry[nid]


if __name__ == "__main__":
    agent = SpineAgent()
    try:
        asyncio.run(agent.run())
    except KeyboardInterrupt:
        asyncio.run(agent.shutdown())
