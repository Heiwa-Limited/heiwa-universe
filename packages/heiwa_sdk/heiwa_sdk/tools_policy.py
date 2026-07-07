"""
[HEIWA-SECURITY] Policy wrapper for heiwa_sdk.tools

Drop-in hardening for the autonomous-agent tool surface. The base `tools.py`
exposes read_file / write_file / grep / list_directory / run_command with no
confinement: run_command uses shell=True, and file ops accept any absolute
path. This module enforces a deny-by-default policy without rewriting the base
tools, so existing behavior is unchanged until an agent opts in by dispatching
through `guarded_invoke` instead of `tools.invoke_tool`.

Policy (all configurable via env or ToolPolicy):
  - File reads/writes confined to an allowlisted root (default: cwd).
  - run_command:
      * shell=False, command parsed with shlex (no shell metachar injection).
      * executable checked against an allowlist.
      * denied entirely unless HEIWA_TOOLS_ALLOW_EXEC=1 (or policy.allow_exec).
  - Every call returns {"error": "..."} on policy violation; nothing raises
    to the agent loop.

Wire-on (single line in the agent host):
    from heiwa_sdk.tools_policy import guarded_invoke as invoke_tool
"""

from __future__ import annotations

import os
import shlex
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional

from . import tools as _base

# Conservative default: read-only shell utilities. Extend deliberately.
_DEFAULT_EXEC_ALLOWLIST = frozenset(
    {"ls", "cat", "grep", "rg", "find", "git", "python3", "cargo", "node", "echo", "pwd"}
)


@dataclass
class ToolPolicy:
    """Runtime policy for the agent tool surface."""

    roots: List[Path] = field(default_factory=list)
    allow_exec: bool = False
    exec_allowlist: frozenset = _DEFAULT_EXEC_ALLOWLIST
    approval_hook: Optional[Callable[[str, Dict[str, Any]], bool]] = None

    @classmethod
    def from_env(cls) -> "ToolPolicy":
        raw_roots = os.environ.get("HEIWA_TOOLS_ROOT", os.getcwd())
        roots = [Path(p).expanduser().resolve() for p in raw_roots.split(os.pathsep) if p]
        allow_exec = os.environ.get("HEIWA_TOOLS_ALLOW_EXEC", "0") == "1"
        extra = os.environ.get("HEIWA_TOOLS_EXEC_ALLOWLIST", "")
        allowlist = _DEFAULT_EXEC_ALLOWLIST
        if extra:
            allowlist = frozenset(allowlist | {c.strip() for c in extra.split(",") if c.strip()})
        return cls(roots=roots, allow_exec=allow_exec, exec_allowlist=allowlist)

    def path_allowed(self, path: str) -> Optional[str]:
        """Return an error string if `path` escapes every allowed root, else None."""
        try:
            resolved = Path(path).expanduser().resolve()
        except (OSError, RuntimeError) as e:
            return f"Path could not be resolved: {e}"
        for root in self.roots:
            try:
                resolved.relative_to(root)
                return None
            except ValueError:
                continue
        return f"Path escapes sandbox root(s) {[str(r) for r in self.roots]}: {resolved}"


def _confined_read(policy: ToolPolicy, **kw) -> Dict[str, Any]:
    err = policy.path_allowed(kw.get("path", ""))
    return {"error": err} if err else _base.read_file(**kw)


def _confined_write(policy: ToolPolicy, **kw) -> Dict[str, Any]:
    err = policy.path_allowed(kw.get("path", ""))
    return {"error": err} if err else _base.write_file(**kw)


def _confined_grep(policy: ToolPolicy, **kw) -> Dict[str, Any]:
    err = policy.path_allowed(kw.get("path", ""))
    return {"error": err} if err else _base.grep(**kw)


def _confined_list(policy: ToolPolicy, **kw) -> Dict[str, Any]:
    err = policy.path_allowed(kw.get("path", ""))
    return {"error": err} if err else _base.list_directory(**kw)


def _guarded_run(policy: ToolPolicy, **kw) -> Dict[str, Any]:
    if not policy.allow_exec:
        return {"error": "run_command denied by policy (set HEIWA_TOOLS_ALLOW_EXEC=1 to enable)"}

    command = kw.get("command", "")
    try:
        argv = shlex.split(command)
    except ValueError as e:
        return {"error": f"Unparseable command: {e}"}
    if not argv:
        return {"error": "Empty command"}

    exe = os.path.basename(argv[0])
    if exe not in policy.exec_allowlist:
        return {"error": f"Executable '{exe}' not in allowlist {sorted(policy.exec_allowlist)}"}

    cwd = kw.get("cwd")
    if cwd:
        err = policy.path_allowed(cwd)
        if err:
            return {"error": err}

    if policy.approval_hook and not policy.approval_hook("run_command", {"argv": argv, "cwd": cwd}):
        return {"error": "run_command denied by approval hook"}

    # shell=False: argv is passed directly, no shell metacharacter interpretation.
    import subprocess

    try:
        result = subprocess.run(
            argv,
            shell=False,
            capture_output=True,
            text=True,
            cwd=cwd,
            timeout=kw.get("timeout", 60),
        )
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
            "success": result.returncode == 0,
        }
    except subprocess.TimeoutExpired:
        return {"error": "Command timed out", "returncode": -1}
    except Exception as e:  # noqa: BLE001 — never raise into the agent loop
        return {"error": str(e), "returncode": -1}


_GUARDED = {
    "read_file": _confined_read,
    "write_file": _confined_write,
    "grep": _confined_grep,
    "list_directory": _confined_list,
    "run_command": _guarded_run,
}


def guarded_invoke(name: str, policy: Optional[ToolPolicy] = None, **kwargs) -> Dict[str, Any]:
    """Policy-enforcing replacement for tools.invoke_tool."""
    policy = policy or ToolPolicy.from_env()
    fn = _GUARDED.get(name)
    if fn is None:
        return {"error": f"Unknown or non-whitelisted tool: {name}"}
    return fn(policy, **kwargs)
