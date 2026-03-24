import asyncio
import logging
import os
import sys
from pathlib import Path
from typing import Optional, Tuple

logger = logging.getLogger("SDK.ToolMesh")


class ToolMesh:
    """Thin execution layer for localized tool wrappers."""

    def __init__(self, root_dir: Path):
        self.root = root_dir
        self.wrappers_dir = self.root / "apps/heiwa_cli/scripts/agents/wrappers"
        from heiwa_sdk.hooks import ExecutionHookManager
        self.hooks = ExecutionHookManager(root_dir)

    def _wrapper_for_tool(self, tool: str) -> Path | None:
        wrapper_map = {
            "heiwa_code": self.wrappers_dir / "codex_exec.sh",
            "heiwa_claude": self.wrappers_dir / "claude_exec.sh",
            "heiwa_claw": self.wrappers_dir / "openclaw_exec.sh",
            "heiwa_gemini": self.wrappers_dir / "gemini_exec.sh",
            "heiwa_reflex": self.wrappers_dir / "ollama_exec.py",
            "heiwa_ops": self.root / "apps/heiwa_cli/scripts/ops/heiwa_360_check.py",
            "heiwa_buff": self.root / "apps/heiwa_cli/scripts/ops/sota_verify.py",
            "codex": self.wrappers_dir / "codex_exec.sh",
            "claude": self.wrappers_dir / "claude_exec.sh",
            "openclaw": self.wrappers_dir / "openclaw_exec.sh",
            "gemini_cli": self.wrappers_dir / "gemini_exec.sh",
            "antigravity": self.wrappers_dir / "openclaw_exec.sh",
            "siliconflow": self.wrappers_dir / "openclaw_exec.sh",
            "cerebras": self.wrappers_dir / "openclaw_exec.sh",
            "ollama": self.wrappers_dir / "ollama_exec.py",
            "local": self.wrappers_dir / "ollama_exec.py",
            "vllm": self.wrappers_dir / "ollama_exec.py",
            "litellm": self.wrappers_dir / "ollama_exec.py",
        }
        return wrapper_map.get(tool.lower())

    async def execute(
        self,
        tool: str,
        instruction: str,
        model: Optional[str] = None,
        extra_env: Optional[dict[str, str]] = None,
        proposal_id: Optional[str] = None,
    ) -> Tuple[int, str]:
        
        # 1. pre_tool_call hook validation
        if os.getenv("OPENCLAW_SESSION_ID"):
             # Skip verification: OpenClaw chokepoint already validated this execution frame.
             allow, reason, hook_metadata = True, "Skipped (OpenClaw verified)", None
        else:
             node_id = os.getenv("HEIWA_NODE_ID", "local_node")
             allow, reason, hook_metadata = self.hooks.before_tool_call(
                 tool=tool,
                 proposal_id=proposal_id or "unknown",
                 node_id=node_id,
                 payload={"instruction": instruction},
             )
        
        if not allow:
            logger.warning("🚫 Execution denied by ToolMesh hook: %s", reason)
            return 1, f"Execution denied by ToolMesh hook: {reason}"

        wrapper = self._wrapper_for_tool(tool)
        if not wrapper or not wrapper.exists():
            return 2, f"❌ [HEIWA TOOLMESH] Tool '{tool}' is unavailable."

        env = os.environ.copy()
        env.update(extra_env or {})
        if model:
            env["HEIWA_ACTIVE_MODEL"] = model
            env["HEIWA_MODEL_BASENAME"] = model.split("/", 1)[1] if "/" in model else model
            env.setdefault("OPENCLAW_MODEL", model)
            env.setdefault("GEMINI_MODEL", env["HEIWA_MODEL_BASENAME"])
            env.setdefault("CLAUDE_MODEL", env["HEIWA_MODEL_BASENAME"])
            env.setdefault("CODEX_MODEL", env["HEIWA_MODEL_BASENAME"])
            if tool.lower() in {"ollama", "local", "vllm", "litellm", "heiwa_reflex"}:
                env["HEIWA_OLLAMA_MODEL"] = model.split("/", 1)[1] if "/" in model else model

        logger.info("🌐 [HEIWA TOOLMESH] Invoking %s via %s", tool, wrapper.name)

        if wrapper.suffix == ".sh":
            cmd = ["bash", str(wrapper), instruction]
        elif wrapper.suffix == ".py":
            cmd = [sys.executable, str(wrapper), instruction]
        else:
            cmd = [str(wrapper), instruction]

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                cwd=str(self.root),
                env=env,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await proc.communicate()
            output = (stdout + stderr).decode(errors="ignore").strip()
            exit_code = int(proc.returncode)

            # 2. post_tool_call hook execution (skip if OpenClaw already validated)
            if not os.getenv("OPENCLAW_SESSION_ID"):
                self.hooks.after_tool_call(
                    tool=tool,
                    proposal_id=proposal_id or "unknown",
                    exit_code=exit_code,
                    output=output,
                    audit_metadata=hook_metadata,
                )

            return exit_code, output
        except Exception as exc:
            return 1, f"Tool mesh execution error: {exc}"
