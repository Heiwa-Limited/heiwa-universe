"""
Heiwa Worker Manager — connects to Railway hub via WebSocket.

Registers this machine as a remote worker, receives task assignments,
executes locally, and streams results back to the hub.
"""

import asyncio
import json
import logging
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict

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
        self.hub_url = (
            os.getenv("HEIWA_HUB_URL")
            or getattr(settings, "HUB_BASE_URL", None)
            or "https://api.heiwa.ltd"
        )
        self.auth_token = os.getenv("HEIWA_AUTH_TOKEN") or getattr(settings, "HEIWA_AUTH_TOKEN", "") or ""
        self.router = ModelRouter()
        self.mesh = ToolMesh(self.root)
        self.capabilities = self._detect_capabilities()
        self.concurrency = int(os.getenv("HEIWA_EXECUTOR_CONCURRENCY", "4"))
        self.sem = asyncio.Semaphore(max(1, self.concurrency))
        self.running = True

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
    def _detect_gpu_hardware() -> tuple[str, int]:
        """Detect GPU type and usable VRAM in GB from actual hardware.

        Returns (gpu_type, vram_gb) — e.g. ("metal", 18) or ("nvidia", 12).
        Falls back to ("cpu", 0) if no accelerator is detected.
        """
        import platform
        import subprocess

        os_type = platform.system()

        if os_type == "Darwin" and platform.machine() == "arm64":
            try:
                mem_bytes = int(subprocess.check_output(
                    ["sysctl", "-n", "hw.memsize"], timeout=5,
                ).strip())
                # macOS reserves ~25% for the OS; ~75% is usable for GPU workloads
                usable_gb = int(mem_bytes / (1024 ** 3) * 0.75)
                return ("metal", usable_gb)
            except Exception:
                pass

        elif os_type == "Linux":
            try:
                smi_output = subprocess.check_output(
                    ["nvidia-smi", "--query-gpu=memory.total",
                     "--format=csv,noheader,nounits"],
                    text=True, timeout=5,
                ).strip()
                vram_mb = int(smi_output.split("\n")[0].strip())
                return ("nvidia", int(vram_mb / 1024))
            except (FileNotFoundError, subprocess.CalledProcessError, ValueError):
                pass

        return ("cpu", 0)

    def _detect_capabilities(self) -> dict:
        caps_str = os.getenv("HEIWA_CAPABILITIES", "")
        caps = {c.strip().lower() for c in caps_str.split(",") if c.strip()}
        node_type = os.getenv("HEIWA_NODE_TYPE", "mobile_node")
        if not caps:
            if node_type == "heavy_compute":
                caps = {"heavy_compute", "gpu_native", "standard_compute"}
            else:
                caps = {"standard_compute", "workspace_interaction", "agile_coding"}

        # Auto-detect GPU hardware
        gpu_type, gpu_vram_gb = self._detect_gpu_hardware()
        if gpu_vram_gb > 0:
            caps.add("gpu_native")
            # Advertise VRAM tier for capability-based dispatch
            if gpu_vram_gb >= 16:
                caps.add("gpu_vram_16gb")
                caps.add("large_model")
            elif gpu_vram_gb >= 8:
                caps.add("gpu_vram_8gb")
                caps.add("fast_inference")

        # Auto-detect Ollama
        ollama_models = self._probe_ollama_models()
        ollama_available = ollama_models is not None
        result: dict = {
            "runtime": node_type,
            "capabilities": list(caps),
            "node_id": self.node_id,
            "ollama": ollama_available,
            "ollama_models": ollama_models or [],
            "gpu_type": gpu_type,
            "gpu_vram_gb": gpu_vram_gb,
        }
        if gpu_vram_gb > 0:
            logger.info("GPU detected: %s with %d GB usable VRAM", gpu_type, gpu_vram_gb)
        if ollama_available:
            logger.info("Ollama detected — advertising LLM proxy capabilities (%d model(s))", len(ollama_models or []))
        return result



    async def execute(self, payload: Dict[str, Any], ws: Any) -> None:
        """Execute a task locally and send the result back via WebSocket."""
        async with self.sem:
            start = time.time()
            task_id = str(payload.get("task_id", "unknown"))
            tool = str(payload.get("target_tool", "openclaw")).lower()
            instruction = str(payload.get("instruction") or payload.get("raw_text") or "").strip()

            logger.info("Executing %s (tool=%s) ...", task_id, tool)
            code, out = await self.mesh.execute(tool, instruction, proposal_id=task_id)
            status = "PASS" if code == 0 else "FAIL"
            duration = int((time.time() - start) * 1000)

            result_msg = {
                "type": "result",
                "data": {
                    "task_id": task_id,
                    "status": status,
                    "summary": str(out or ""),
                    "duration_ms": duration,
                    "runtime": self.node_id,
                    "target_tool": tool,
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

        ws_url = self.hub_url.replace("https://", "wss://").replace("http://", "ws://")
        ws_url = f"{ws_url}/ws/worker"

        while self.running:
            try:
                logger.info("Connecting to hub at %s ...", ws_url)
                async with websockets.connect(ws_url, open_timeout=10) as ws:
                    # Register with auth
                    await ws.send(json.dumps({
                        "type": "register",
                        "worker_id": self.node_id,
                        "auth_token": self.auth_token,
                        "capabilities": self.capabilities,
                    }))
                    reg_resp = json.loads(await ws.recv())
                    if reg_resp.get("type") == "error":
                        logger.error("Registration failed: %s", reg_resp.get("detail"))
                        return
                    logger.info("Registered as %s", reg_resp.get("worker_id"))

                    # Start heartbeat loop
                    asyncio.create_task(self._heartbeat_loop(ws))

                    # Main message loop
                    async for raw in ws:
                        msg = json.loads(raw)
                        msg_type = msg.get("type", "")
                        if msg_type == "task_assignment":
                            asyncio.create_task(self.execute(msg.get("data", {}), ws))
                        elif msg_type == "llm_request":
                            asyncio.create_task(self._handle_llm_request(msg, ws))
                        elif msg_type == "no_work":
                            pass

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
                    "type": "heartbeat",
                    "capabilities": self.capabilities,
                }))
                await asyncio.sleep(15)
        except Exception:
            pass


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s")
    asyncio.run(WorkerManager().run())
