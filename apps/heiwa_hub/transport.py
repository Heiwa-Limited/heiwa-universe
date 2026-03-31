"""
Hub-native transport layer.

Provides two transport backends that share one interface:
  - LocalBusTransport: asyncio-based in-process pub/sub for Railway agents
  - WorkerSessionManager: outbound WebSocket delivery for remote Mac/WSL workers

Agents call speak() and listen() on the transport exactly as before.
The transport decides whether delivery is local or remote.
"""

import asyncio
import json
import logging
import time
from collections import defaultdict
from typing import Any, Callable, Coroutine, Dict, Optional

from heiwa_protocol.protocol import Subject, Payload
from heiwa_protocol.schemas.pod import PodRecord

logger = logging.getLogger("Hub.Transport")

# Type alias for event callbacks
EventCallback = Callable[[Dict[str, Any]], Coroutine[Any, Any, None]]


class LocalBusTransport:
    """
    In-process event bus for agents running inside the Railway hub.

    Subscribers receive events via asyncio tasks for non-blocking fanout.
    """

    def __init__(self):
        self._subscribers: dict[str, list[EventCallback]] = defaultdict(list)
        self._started = True

    async def publish(self, subject: Subject, data: Dict[str, Any], sender_id: str = "hub"):
        """Publish an event to all local subscribers of this subject.

        Callbacks are dispatched sequentially within a subject to preserve
        ordering (e.g. STATUS before EXEC_RESULT), but each publish call
        returns immediately — the dispatch runs in a background task so the
        publisher is never blocked by slow subscribers.
        """
        from heiwa_sdk.config import settings
        envelope = {
            Payload.SENDER_ID: sender_id,
            Payload.TIMESTAMP: time.time(),
            Payload.TYPE: subject.name,
            Payload.DATA: data,
            "auth_token": settings.HEIWA_AUTH_TOKEN, # Critical: Spine requires this
        }
        key = subject.value
        # Snapshot the subscriber list so mutations during dispatch are safe
        callbacks = list(self._subscribers.get(key, []))
        if callbacks:
            asyncio.create_task(self._ordered_dispatch(callbacks, envelope, key))

    async def _ordered_dispatch(self, callbacks: list[EventCallback], data: Dict[str, Any], subject: str):
        """Dispatch to all callbacks in order, awaiting each one."""
        for cb in callbacks:
            await self._safe_dispatch(cb, data, subject)

    async def subscribe(self, subject: Subject, callback: EventCallback):
        """Register a callback for a subject."""
        self._subscribers[subject.value].append(callback)
        logger.debug("LocalBus: subscribed to %s", subject.value)

    def unsubscribe(self, subject: Subject, callback: EventCallback):
        """Remove a specific callback from a subject."""
        try:
            self._subscribers[subject.value].remove(callback)
        except ValueError:
            pass

    async def request(
        self, subject: Subject, data: Dict[str, Any], sender_id: str = "hub", timeout: float = 5.0
    ) -> Optional[Dict[str, Any]]:
        """
        Request-reply pattern over the local bus.
        Publishes to subject and waits for a single reply via a temporary
        reply subject.  Used for Spine -> Broker enrichment when both are
        in-process (kept for interface compatibility but Spine should
        prefer direct BrokerEnrichmentService.enrich() calls).
        """
        from heiwa_sdk.config import settings
        reply_event: asyncio.Event = asyncio.Event()
        reply_data: list[Dict[str, Any]] = []

        async def _capture(data: Dict[str, Any]) -> None:
            reply_data.append(data)
            reply_event.set()

        reply_subject_key = f"_reply.{subject.value}.{id(reply_event)}"
        self._subscribers.setdefault(reply_subject_key, []).append(_capture)

        envelope = {
            Payload.SENDER_ID: sender_id,
            Payload.TIMESTAMP: time.time(),
            Payload.TYPE: subject.name,
            Payload.DATA: data,
            "auth_token": getattr(settings, "HEIWA_AUTH_TOKEN", ""), # Critical
            "_reply_subject": reply_subject_key,
        }
        for cb in self._subscribers.get(subject.value, []):
            asyncio.create_task(self._safe_dispatch(cb, envelope, subject.value))

        try:
            await asyncio.wait_for(reply_event.wait(), timeout=timeout)
            return reply_data[0] if reply_data else None
        except asyncio.TimeoutError:
            return None
        finally:
            if _capture in self._subscribers.get(reply_subject_key, []):
                self._subscribers[reply_subject_key].remove(_capture)

    async def reply(self, reply_subject_key: str, data: Dict[str, Any]):
        """Send a reply to a request-reply exchange."""
        for cb in self._subscribers.get(reply_subject_key, []):
            asyncio.create_task(self._safe_dispatch(cb, data, reply_subject_key))

    @staticmethod
    async def _safe_dispatch(cb: EventCallback, data: Dict[str, Any], subject: str):
        try:
            await cb(data)
        except Exception as exc:
            logger.error("LocalBus dispatch error on %s: %s", subject, exc)

    async def shutdown(self):
        self._started = False
        self._subscribers.clear()


class WorkerSessionManager:
    """
    Manages outbound WebSocket connections from remote workers (Mac/WSL).

    Workers call `WS /ws/worker` on the hub. This manager tracks active
    sessions, pushes task assignments, and receives results/heartbeats.
    """

    def __init__(self) -> None:
        # worker_id -> WebSocket instance
        self._sessions: Dict[str, Any] = {}
        # worker_id -> PodRecord
        self._pods: Dict[str, PodRecord] = {}
        # worker_id -> last heartbeat timestamp
        self._heartbeats: Dict[str, float] = {}
        # worker_id -> last STDB heartbeat write timestamp
        self._last_stdb_heartbeats: Dict[str, float] = {}
        # request_id -> (event, text_list) for LLM proxy requests
        self._llm_pending: Dict[str, tuple[asyncio.Event, list[str]]] = {}

    def _get_stdb(self) -> Any:
        import os
        from heiwa_sdk.spacetimedb import SpacetimeDB
        identity = os.environ.get("STDB_IDENTITY", "heiwaproductiondb")
        server = os.environ.get("STDB_SERVER", "local")
        return SpacetimeDB(db_identity=identity, server=server)

    async def _persist_pod(self, pod: PodRecord) -> None:
        db = self._get_stdb()
        # To avoid dataclasses recursive list parsing bugs, map manually or use local to_dict
        await asyncio.to_thread(db.upsert_pod, pod.to_dict())

    async def _persist_gpu_slots(self, pod: PodRecord) -> None:
        db = self._get_stdb()
        for slot in pod.gpu_inventory:
            if not slot.loaded_models:
                continue
            payload = {
                "slot_id": f"{pod.pod_id}:{slot.loaded_models[0]}",
                "pod_id": pod.pod_id,
                "gpu_type": slot.gpu_type,
                "vram_gb": slot.vram_gb,
                "loaded_models": slot.loaded_models,
                "available_slots": slot.available_slots,
            }
            await asyncio.to_thread(db.upsert_gpu_slot, payload)

    async def _update_heartbeat_stdb(self, pod_id: str, ts: str, liveness: str) -> None:
        db = self._get_stdb()
        await asyncio.to_thread(db.update_pod_heartbeat, pod_id, ts, liveness)

    def register(self, worker_id: str, ws: Any, capabilities: Optional[Dict[str, Any]] = None):
        """Register a worker WebSocket session."""
        self._sessions[worker_id] = ws
        caps = capabilities or {}

        # Parse runtime and capabilities list
        runtime_capabilities = []
        _caps_list = caps.get("capabilities", [])
        if isinstance(_caps_list, list):
            runtime_capabilities.extend(list(_caps_list))
        if caps.get("ollama"):
            runtime_capabilities.append("ollama")

        # Read real GPU hardware from worker detection
        gpu_type = str(caps.get("gpu_type", "cpu"))
        gpu_vram_gb = int(caps.get("gpu_vram_gb", 0))

        # Map loaded Ollama models into GpuSlots with actual hardware data
        from heiwa_protocol.schemas.pod import GpuSlot
        gpu_inventory = [
            GpuSlot(gpu_type=gpu_type, vram_gb=gpu_vram_gb, loaded_models=[m], available_slots=1)
            for m in list(caps.get("ollama_models", []))
        ]
        # If GPU detected but no Ollama models yet, still record the hardware
        if gpu_vram_gb > 0 and not gpu_inventory:
            gpu_inventory = [GpuSlot(gpu_type=gpu_type, vram_gb=gpu_vram_gb, loaded_models=[], available_slots=1)]

        # Instantiate PodRecord schema
        pod = PodRecord(
            pod_id=worker_id,
            host_identity=worker_id,
            provider="local",
            runtime_capabilities=runtime_capabilities,
            trust_tier="local" if "macbook" in worker_id.lower() or "local" in caps.get("runtime", "").lower() else "untrusted",
            privacy_floor="sovereign" if "macbook" in worker_id.lower() else "public",
            gpu_inventory=gpu_inventory,
            liveness="online",
            leaseable=True,
            registered_at=str(time.time()),
            last_heartbeat=str(time.time()),
        )
        self._pods[worker_id] = pod

        self._heartbeats[worker_id] = time.time()
        self._last_stdb_heartbeats[worker_id] = time.time()

        try:
            asyncio.create_task(self._persist_pod(pod))
            if gpu_inventory:
                asyncio.create_task(self._persist_gpu_slots(pod))
        except RuntimeError:
            pass  # Fallback if no running loop, though Hub always has one

        logger.info("Worker registered: %s (gpu=%s/%dGB, capabilities=%s, models=%d)",
                     worker_id, gpu_type, gpu_vram_gb, runtime_capabilities, len(gpu_inventory))

    def unregister(self, worker_id: str):
        """Remove a worker session."""
        self._sessions.pop(worker_id, None)
        if worker_id in self._pods:
             self._pods[worker_id].liveness = "offline"
             try:
                 asyncio.create_task(self._update_heartbeat_stdb(worker_id, str(time.time()), "offline"))
             except RuntimeError:
                 pass
        self._heartbeats.pop(worker_id, None)
        logger.info("Worker unregistered: %s", worker_id)

    def heartbeat(self, worker_id: str, capabilities: Optional[Dict[str, Any]] = None):
        """Update heartbeat timestamp for a worker."""
        now = time.time()
        self._heartbeats[worker_id] = now
        pod = self._pods.get(worker_id)
        
        models_changed = False
        if pod and capabilities and "ollama_models" in capabilities:
            from heiwa_protocol.schemas.pod import GpuSlot
            # Use real GPU data from heartbeat, falling back to existing pod values
            gpu_type = str(capabilities.get("gpu_type", pod.gpu_inventory[0].gpu_type if pod.gpu_inventory else "cpu"))
            gpu_vram_gb = int(capabilities.get("gpu_vram_gb", pod.gpu_inventory[0].vram_gb if pod.gpu_inventory else 0))

            new_models = set(capabilities.get("ollama_models", []))
            current_models = {s.loaded_models[0] for s in pod.gpu_inventory if s.loaded_models}
            if new_models != current_models:
                new_inventory = []
                for s in pod.gpu_inventory:
                    if s.loaded_models and s.loaded_models[0] not in new_models:
                        s.available_slots = 0
                    else:
                        # Update existing slots with latest GPU data
                        s.gpu_type = gpu_type
                        s.vram_gb = gpu_vram_gb
                    new_inventory.append(s)
                for m in new_models - current_models:
                    new_inventory.append(GpuSlot(gpu_type=gpu_type, vram_gb=gpu_vram_gb, loaded_models=[m], available_slots=1))
                pod.gpu_inventory = new_inventory
                models_changed = True
        
        last_write = self._last_stdb_heartbeats.get(worker_id, 0.0)
        if now - last_write > 10.0:
            self._last_stdb_heartbeats[worker_id] = now
            if pod:
                try:
                    asyncio.create_task(self._update_heartbeat_stdb(worker_id, str(now), pod.liveness))
                    if models_changed:
                        asyncio.create_task(self._persist_gpu_slots(pod))
                except RuntimeError:
                    pass

    def get_capabilities(self, worker_id: str) -> Dict[str, Any]:
        """Return the last advertised capabilities for a worker in legacy payload shape."""
        pod = self._pods.get(worker_id)
        if not pod:
            return {}
            
        caps = list(pod.runtime_capabilities)
        ollama = "ollama" in caps
        if ollama:
            caps.remove("ollama")

        models = [s.loaded_models[0] for s in pod.gpu_inventory if s.loaded_models and s.available_slots > 0]

        return {
            "node_id": worker_id,
            "runtime": "local_node" if pod.trust_tier == "local" else "remote_node",
            "capabilities": caps,
            "ollama_models": models,
            "ollama": ollama,
        }

    def get_pod_record(self, worker_id: str) -> Optional[PodRecord]:
        """Return the full PodRecord metadata for a worker."""
        return self._pods.get(worker_id)

    def get_active_workers(self, max_stale_sec: float = 60.0) -> list[str]:
        """Return worker IDs with recent heartbeats."""
        now = time.time()
        return [
            wid for wid, ts in self._heartbeats.items()
            if now - ts < max_stale_sec and wid in self._sessions
        ]

    def get_worker_for_capabilities(self, requires: list[str]) -> Optional[str]:
        """Find an active worker whose runtime_capabilities satisfy all requirements.

        Uses set intersection: the worker must advertise every capability in
        ``requires``.  When multiple workers match, prefer the one with the
        most capabilities (likely the more specialised node).
        """
        if not requires:
            active = self.get_active_workers()
            return active[0] if active else None

        required = set(requires)
        best: Optional[str] = None
        best_score = -1
        for wid in self.get_active_workers():
            pod = self._pods.get(wid)
            if not pod:
                continue
            caps = set(pod.runtime_capabilities)
            if required <= caps:
                score = len(caps)
                if score > best_score:
                    best = wid
                    best_score = score
        return best

    def get_worker_for_runtime(self, target_runtime: str, requires: Optional[list[str]] = None) -> Optional[str]:
        """Find an active worker matching the target runtime.

        When ``requires`` is provided, uses capability set intersection
        instead of substring matching.  Falls back to name-based matching
        for backward compatibility when no requirements are specified.

        Matches against (legacy path):
          - worker_id itself (e.g. ``macbook@heiwa-node-a``)
          - the ``node_id`` capability field
          - substring match on worker_id (e.g. ``macbook`` matches ``macbook@heiwa-node-a``)
          - ``any`` / ``both`` match any active worker
        """
        target = target_runtime.lower()
        if target in {"any", "both"}:
            if requires:
                return self.get_worker_for_capabilities(requires)
            active = self.get_active_workers()
            return active[0] if active else None

        # Capability-based dispatch when requirements are specified
        if requires:
            return self.get_worker_for_capabilities(requires)

        # Legacy substring matching fallback
        for wid in self.get_active_workers():
            pod = self._pods.get(wid)
            if not pod:
                 continue

            node_id = pod.pod_id.lower()
            wid_lower = wid.lower()
            if (
                target == wid_lower
                or target == node_id
                or target in wid_lower
                or target in node_id
            ):
                return wid
        return None

    async def push_task(self, worker_id: str, task_payload: Dict[str, Any]) -> bool:
        """Push a task assignment to a connected worker."""
        ws = self._sessions.get(worker_id)
        if not ws:
            return False
        try:
            await ws.send_json({"type": "task_assignment", "data": task_payload})
            return True
        except Exception as exc:
            logger.error("Failed to push task to worker %s: %s", worker_id, exc)
            self.unregister(worker_id)
            return False

    def _worker_has_ollama(self, worker_id: str) -> bool:
        """Check if a worker has Ollama capability via validated registration data."""
        pod = self._pods.get(worker_id)
        if not pod:
            return False
        return "ollama" in pod.runtime_capabilities

    def has_ollama_worker(self) -> bool:
        """Check if any active worker advertises Ollama capability."""
        return any(self._worker_has_ollama(wid) for wid in self.get_active_workers())

    def get_ollama_worker(self) -> Optional[str]:
        """Return the best active worker with Ollama capability."""
        for wid in self.get_active_workers():
            if self._worker_has_ollama(wid):
                return wid
        return None

    async def request_llm(
        self, worker_id: str, prompt: str, model: str = "", system: str = "", timeout: float = 10.0
    ) -> Optional[str]:
        """Request-reply LLM inference via a boost node worker.

        Sends an llm_request frame and waits for llm_response.
        Returns the generated text, or None on failure/timeout.
        """
        ws = self._sessions.get(worker_id)
        if not ws:
            return None

        import uuid as _uuid
        request_id = str(_uuid.uuid4())
        response_event: asyncio.Event = asyncio.Event()
        response_text: list[str] = []
        self._llm_pending[request_id] = (response_event, response_text)

        try:
            await ws.send_json({
                "type": "llm_request",
                "request_id": request_id,
                "prompt": prompt,
                "model": model,
                "system": system,
            })
            await asyncio.wait_for(response_event.wait(), timeout=timeout)
            return response_text[0] if response_text else None
        except asyncio.TimeoutError:
            logger.warning("LLM proxy to %s timed out after %.1fs", worker_id, timeout)
            return None
        except Exception as exc:
            logger.error("LLM proxy to %s failed: %s", worker_id, exc)
            return None
        finally:
            self._llm_pending.pop(request_id, None)

    def resolve_llm_response(self, request_id: str, text: str) -> bool:
        """Called when a worker sends an llm_response frame."""
        entry = self._llm_pending.get(request_id)
        if entry:
            event, text_list = entry
            text_list.append(text)
            event.set()
            return True
        return False

    async def broadcast(self, message: Dict[str, Any]):
        """Send a message to all connected workers."""
        dead = []
        for wid, ws in self._sessions.items():
            try:
                await ws.send_json(message)
            except Exception:
                dead.append(wid)
        for wid in dead:
            self.unregister(wid)


# Singleton instances for the hub process
_bus: Optional[LocalBusTransport] = None
_workers: Optional[WorkerSessionManager] = None


def get_bus() -> LocalBusTransport:
    global _bus
    if _bus is None:
        _bus = LocalBusTransport()
    return _bus


def get_worker_manager() -> WorkerSessionManager:
    global _workers
    if _workers is None:
        _workers = WorkerSessionManager()
    return _workers
