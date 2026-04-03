"""
Heiwa Worker Manager — connects to Railway hub via WebSocket.

Registers this machine as a remote worker, receives task assignments,
executes locally, and streams results back to the hub.
"""

import asyncio
import json
import logging
import os
import platform
import sys
import time
import uuid
from pathlib import Path
from typing import Any, Dict
from urllib.parse import quote

# Ensure monorepo roots are on sys.path
ROOT = Path(__file__).resolve().parents[4]
for pkg in ["heiwa_sdk", "heiwa_protocol", "heiwa_identity"]:
    path = str(ROOT / f"packages/{pkg}")
    if path not in sys.path:
        sys.path.insert(0, path)
if str(ROOT / "apps") not in sys.path:
    sys.path.insert(0, str(ROOT / "apps"))

from heiwa_sdk.config import load_swarm_env, settings
load_swarm_env()

from heiwa_sdk.tool_mesh import ToolMesh
from heiwa_sdk.routing import ModelRouter

logger = logging.getLogger("WorkerManager")


class WorkerManager:
    """Connects to the Railway hub via WS /ws/worker and executes assigned tasks."""

    def __init__(self) -> None:
        self.root = ROOT
        self.node_id = os.getenv("HEIWA_NODE_ID", "macbook@heiwa-agile")
        self.instance_id = os.getenv("HEIWA_INSTANCE_ID", str(uuid.uuid4()))
        self.hub_url = (
            os.getenv("HEIWA_HUB_URL")
            or getattr(settings, "HUB_BASE_URL", None)
            or "https://api.heiwa.ltd"
        )
        self.auth_token = (
            os.getenv("HEIWA_MACHINE_AUTH_TOKEN")
            or os.getenv("HEIWA_AUTH_TOKEN")
            or getattr(settings, "HEIWA_MACHINE_AUTH_TOKEN", "")
            or getattr(settings, "HEIWA_AUTH_TOKEN", "")
            or ""
        )
        self.router = ModelRouter()
        self.mesh = ToolMesh(self.root)
        self.concurrency = int(os.getenv("HEIWA_EXECUTOR_CONCURRENCY", "4"))
        self.capabilities = self._detect_capabilities()
        self.sem = asyncio.Semaphore(max(1, self.concurrency))
        self.running = True
        self.session_id: str | None = None

    @staticmethod
    def _probe_ollama_models() -> list[str] | None:
        """Check if Ollama is reachable on this machine and list loaded models."""
        import urllib.request
        base = os.getenv("OLLAMA_BASE_URL", "http://localhost:11434")
        try:
            req = urllib.request.Request(f"{base}/api/tags", method="GET")
            with urllib.request.urlopen(req, timeout=3) as resp:
                if resp.status == 200:
                    import json
                    body = json.loads(resp.read())
                    return [m.get("name") for m in body.get("models", []) if m.get("name")]
        except Exception:
            pass
        return None

    @staticmethod
    def _quantization_metadata(models: list[str]) -> list[dict[str, str]]:
        metadata: list[dict[str, str]] = []
        for model in models:
            quantization = "unknown"
            lower = model.lower()
            for marker in ("q8_0", "q6_k", "q5_k_m", "q4_k_m", "q4", "q3", "q2", "1bit", "1-bit"):
                if marker in lower:
                    quantization = marker
                    break
            metadata.append({"model": model, "quantization": quantization})
        return metadata

    def _detect_capabilities(self) -> dict:
        caps_str = os.getenv("HEIWA_CAPABILITIES", "")
        caps = {c.strip().lower() for c in caps_str.split(",") if c.strip()}
        node_type = os.getenv("HEIWA_NODE_TYPE", "mobile_node")
        if not caps:
            if node_type == "heavy_compute":
                caps = {"heavy_compute", "gpu_native", "standard_compute"}
            else:
                caps = {"standard_compute", "workspace_interaction", "agile_coding"}

        # Auto-detect Ollama
        ollama_models = self._probe_ollama_models()
        ollama_available = ollama_models is not None
        quantization = self._quantization_metadata(ollama_models or [])
        result: dict = {
            "platform": f"{platform.system().lower()}-{platform.machine().lower()}",
            "host_role": node_type,
            "capabilities": list(caps),
            "node_id": self.node_id,
            "models": ollama_models or [],
            "quantization": quantization,
            "vram_mb": int(os.getenv("HEIWA_NODE_VRAM_MB", "0") or "0"),
            "embedding_capable": any("embedding" in model.lower() for model in (ollama_models or [])),
            "media_capable": any(
                marker in model.lower() for marker in ("sdxl", "image", "flux") for model in (ollama_models or [])
            ),
            "filesystem_capable": True,
            "max_concurrency": self.concurrency,
        }
        if ollama_available:
            logger.info("Ollama detected — advertising LLM proxy capabilities (%d model(s))", len(ollama_models or []))
        return result

    def _canonical_register_payload(self) -> dict[str, Any]:
        return {
            "version": "v1",
            "type": "REGISTER",
            "timestamp": self._iso_now(),
            "node_id": self.node_id,
            "payload": {
                "instance_id": self.instance_id,
                "runtime": "python",
                "runtime_version": platform.python_version(),
                "worker_version": settings.HEIWA_VERSION,
                "capabilities": self.capabilities.get("capabilities", []),
                "max_concurrency": self.concurrency,
                "platform": self.capabilities.get("platform"),
                "host_role": self.capabilities.get("host_role"),
                "models": self.capabilities.get("models", []),
                "quantization": self.capabilities.get("quantization", []),
                "vram_mb": self.capabilities.get("vram_mb"),
                "embedding_capable": self.capabilities.get("embedding_capable"),
                "media_capable": self.capabilities.get("media_capable"),
                "filesystem_capable": self.capabilities.get("filesystem_capable"),
            },
        }

    @staticmethod
    def _iso_now() -> str:
        return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

    def _worker_ws_url(self) -> str:
        ws_url = self.hub_url.replace("https://", "wss://").replace("http://", "ws://")
        return f"{ws_url}/ws/worker?token={quote(self.auth_token)}"

    async def execute(self, payload: Dict[str, Any], ws: Any) -> None:
        """Execute a task locally and send the result back via WebSocket."""
        async with self.sem:
            start = time.time()
            task_id = str(payload.get("task_id", "unknown"))
            lease_id = str(payload.get("lease_id", ""))
            tool = str(payload.get("target_tool", "openclaw")).lower()
            instruction = str(payload.get("instruction") or payload.get("raw_text") or "").strip()

            logger.info("Executing %s (tool=%s) ...", task_id, tool)
            code, out = await self.mesh.execute(tool, instruction, proposal_id=task_id)
            status = "PASS" if code == 0 else "FAIL"
            duration = int((time.time() - start) * 1000)

            result_msg = {
                "version": "v1",
                "type": "RESULT",
                "timestamp": self._iso_now(),
                "node_id": self.node_id,
                "session_id": self.session_id,
                "payload": {
                    "task_id": task_id,
                    "lease_id": lease_id,
                    "status": "success" if code == 0 else "failure",
                    "artifacts": [
                        {
                            "artifact_id": f"artifact-{uuid.uuid4()}",
                            "type": "log",
                            "hash": f"sha256:{abs(hash(str(out or ''))):x}",
                            "size_bytes": len(str(out or "").encode("utf-8")),
                            "location": f"artifact://worker/{task_id}/summary",
                        }
                    ],
                    "metrics": {
                        "duration_ms": duration,
                        "tokens_in": 0,
                        "tokens_out": 0,
                    },
                },
            }
            try:
                await ws.send(json.dumps(result_msg))
            except Exception as e:
                logger.error("Failed to send result for %s: %s", task_id, e)

    async def run(self) -> None:
        """Connect to hub and process task assignments."""
        try:
            import websockets
        except ImportError:
            logger.error("websockets package required: pip install websockets")
            sys.exit(1)

        ws_url = self._worker_ws_url()

        while self.running:
            try:
                logger.info("Connecting to hub at %s ...", ws_url)
                async with websockets.connect(ws_url, open_timeout=10) as ws:
                    await ws.send(json.dumps(self._canonical_register_payload()))
                    reg_resp = json.loads(await ws.recv())
                    if reg_resp.get("type") == "ERROR":
                        logger.error("Registration failed: %s", reg_resp.get("payload", {}).get("message"))
                        return
                    if reg_resp.get("type") != "AUTH_OK":
                        logger.error("Unexpected auth response: %s", reg_resp)
                        return
                    self.session_id = reg_resp.get("session_id")
                    logger.info("Registered as %s (session=%s)", self.node_id, self.session_id)

                    # Start heartbeat loop
                    asyncio.create_task(self._heartbeat_loop(ws))

                    # Main message loop
                    async for raw in ws:
                        msg = json.loads(raw)
                        msg_type = msg.get("type", "")
                        if msg_type == "DISPATCH":
                            payload = msg.get("payload", {})
                            ack = {
                                "version": "v1",
                                "type": "DISPATCH_ACK",
                                "timestamp": self._iso_now(),
                                "node_id": self.node_id,
                                "session_id": self.session_id,
                                "payload": {
                                    "task_id": payload.get("task_id"),
                                    "lease_id": payload.get("lease_id"),
                                    "accepted": True,
                                },
                            }
                            await ws.send(json.dumps(ack))
                            asyncio.create_task(self.execute(payload, ws))
                        elif msg_type == "TASK_CANCEL":
                            logger.warning("Task cancel not yet implemented: %s", msg)
                        elif msg_type == "ERROR":
                            logger.error("Worker protocol error: %s", msg.get("payload", {}).get("message"))

            except Exception as e:
                logger.warning("Connection lost: %s. Reconnecting in 5s...", e)
                await asyncio.sleep(5)

    async def _handle_llm_request(self, msg: dict, ws: Any) -> None:
        """Proxy an LLM request through local Ollama."""
        request_id = msg.get("request_id", "")
        prompt = msg.get("prompt", "")
        model = msg.get("model") or os.getenv("OLLAMA_MODEL", "llama3.2")
        system = msg.get("system", "")

        base = os.getenv("OLLAMA_BASE_URL", "http://localhost:11434")
        payload = {"model": model, "prompt": prompt, "stream": False}
        if system:
            payload["system"] = system

        text = ""
        try:
            import urllib.request
            req = urllib.request.Request(
                f"{base}/api/generate",
                data=json.dumps(payload).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = json.loads(resp.read())
                text = body.get("response", "")
        except Exception as e:
            logger.error("Ollama LLM request failed: %s", e)

        try:
            await ws.send(json.dumps({
                "type": "llm_response",
                "request_id": request_id,
                "text": text,
            }))
        except Exception as e:
            logger.error("Failed to send llm_response: %s", e)

    async def _heartbeat_loop(self, ws: Any) -> None:
        try:
            while self.running:
                self.capabilities = self._detect_capabilities()
                await ws.send(json.dumps({
                    "version": "v1",
                    "type": "HEARTBEAT",
                    "timestamp": self._iso_now(),
                    "node_id": self.node_id,
                    "session_id": self.session_id,
                    "payload": {
                        "status": "busy" if self.sem.locked() else "idle",
                        "active_tasks": max(0, self.concurrency - self.sem._value),
                        "load": round((self.concurrency - self.sem._value) / max(1, self.concurrency), 3),
                    },
                }))
                await asyncio.sleep(15)
        except Exception:
            pass


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s")
    asyncio.run(WorkerManager().run())
