"""Shared Wire client for CLI-to-Hub communication."""

from __future__ import annotations

import asyncio
import json
from typing import Any

from heiwa_cli.stream import render_output


class WireClient:
    """Persistent Hub client using `/ws/client` for event delivery."""

    def __init__(self, *, base_url: str, token: str, battlefield_id: str, session_id: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.battlefield_id = battlefield_id
        self.session_id = session_id
        self.last_seen_event_id: str | None = None
        self._ws = None
        self._recv_task: asyncio.Task | None = None
        self._event_queue: asyncio.Queue[dict[str, Any]] = asyncio.Queue()

    async def connect(self) -> None:
        if self._ws is not None:
            return
        try:
            import websockets
        except ImportError as exc:
            raise RuntimeError("websockets package is required for WireClient") from exc

        ws_url = self.base_url.replace("https://", "wss://").replace("http://", "ws://")
        query = [f"token={self.token}"]
        if self.last_seen_event_id:
            query.append(f"last_seen_event_id={self.last_seen_event_id}")
        self._ws = await websockets.connect(f"{ws_url}/ws/client?{'&'.join(query)}", open_timeout=10)
        self._recv_task = asyncio.create_task(self._receive_loop())

    async def close(self) -> None:
        if self._recv_task is not None:
            self._recv_task.cancel()
            try:
                await self._recv_task
            except asyncio.CancelledError:
                pass
            self._recv_task = None
        if self._ws is not None:
            await self._ws.close()
            self._ws = None

    async def register_battlefield(self, payload: dict[str, Any]) -> dict[str, Any]:
        return await self._post_json("/battlefields", payload)

    async def submit_task(
        self,
        *,
        raw_text: str,
        sender_id: str,
        source_surface: str,
        task_id: str,
        privacy_level: str | None = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "raw_text": raw_text,
            "sender_id": sender_id,
            "source_surface": source_surface,
            "task_id": task_id,
            "session_id": self.session_id,
            "battlefield_id": self.battlefield_id,
        }
        if privacy_level:
            payload["privacy_level"] = privacy_level
        return await self._post_json("/tasks", payload)

    async def wait_for_task(self, task_id: str, timeout: float = 120.0, trace: bool = False) -> tuple[int, str]:
        deadline = asyncio.get_running_loop().time() + timeout
        while True:
            remaining = deadline - asyncio.get_running_loop().time()
            if remaining <= 0:
                return 1, ""
            event = await asyncio.wait_for(self._event_queue.get(), timeout=remaining)
            if event.get("type") == "heartbeat":
                continue
            if not self._matches_task(event, task_id):
                continue

            payload = self._event_payload(event)
            event_type = str(event.get("event_type") or "")
            status = str(payload.get("status") or "").upper()

            if event_type == "task_progress":
                if trace:
                    message = render_output(str(payload.get("content") or payload.get("message") or ""))
                    if message:
                        print(message)
                continue

            if status in {"ACKNOWLEDGED", "DISPATCHED_PLAN", "DISPATCHED_FALLBACK", "ACCEPTED"}:
                if trace:
                    message = render_output(str(payload.get("message") or status))
                    if message:
                        print(message)
                continue

            if status == "AWAITING_APPROVAL":
                return 2, ""
            if status in {"REJECTED", "EXPIRED"}:
                return 1, ""
            if status in {"PASS", "DELIVERED"}:
                summary = render_output(str(payload.get("summary") or payload.get("message") or ""))
                return 0, summary
            if status in {"FAIL", "BLOCKED_AUTH", "BLOCKED_NO_CONTENT"}:
                return 1, ""

    async def _receive_loop(self) -> None:
        try:
            while True:
                raw = await self._ws.recv()
                event = json.loads(raw)
                event_id = event.get("event_id")
                if event_id:
                    self.last_seen_event_id = str(event_id)
                await self._event_queue.put(event)
        except asyncio.CancelledError:
            raise
        except Exception:
            return

    async def _post_json(self, path: str, payload: dict[str, Any]) -> dict[str, Any]:
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.token}",
        }
        body = json.dumps(payload).encode()
        try:
            import httpx

            async with httpx.AsyncClient(timeout=10.0) as client:
                response = await client.post(f"{self.base_url}{path}", content=body, headers=headers)
                response.raise_for_status()
                return response.json()
        except ImportError:
            pass

        import urllib.request

        req = urllib.request.Request(f"{self.base_url}{path}", data=body, headers=headers, method="POST")
        with urllib.request.urlopen(req, timeout=10) as response:
            return json.loads(response.read().decode())

    @staticmethod
    def _event_payload(event: dict[str, Any]) -> dict[str, Any]:
        payload = event.get("payload_json")
        return payload if isinstance(payload, dict) else event

    @classmethod
    def _matches_task(cls, event: dict[str, Any], task_id: str) -> bool:
        if str(event.get("mission_id") or "") == task_id:
            return True
        payload = cls._event_payload(event)
        return str(payload.get("task_id") or "") == task_id
