"""Baseline smoke tests for Class 3 runtime safety."""
from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path


HOME = Path("/Users/dmcgregsauce")
GEMINI_POLICY = HOME / ".gemini/hooks/runtime_policy.js"
CLAUDE_POLICY = HOME / ".claude/plugins/devon-runtime-safety/hooks/pretool_policy.py"
CODEX_SAFE = HOME / ".codex/bin/codex-safe"
DEVONX = HOME / "heiwa_archive/heiwa-core/bin/devonx"


def run_json_command(cmd: list[str], payload: dict[str, object]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        input=json.dumps(payload),
        text=True,
        capture_output=True,
        check=False,
    )


def assert_exists(path: Path, label: str) -> None:
    assert path.exists(), f"Missing {label}: {path}"


def test_gemini_blocks_root_delete() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    payload = {"tool_name": "run_shell_command", "tool_input": {"command": "rm -rf /"}}
    proc = run_json_command(["node", str(GEMINI_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_gemini_blocks_sensitive_write() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    payload = {"tool_name": "write_file", "tool_input": {"path": "/Users/dmcgregsauce/.ssh/config"}}
    proc = run_json_command(["node", str(GEMINI_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_claude_blocks_network_post() -> None:
    assert_exists(CLAUDE_POLICY, "Claude policy hook")
    payload = {"tool_name": "WebFetch", "tool_input": {"url": "https://example.com", "method": "POST"}}
    proc = run_json_command(["python3", str(CLAUDE_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_codex_safe_rejects_dangerous_bypass_flag() -> None:
    assert_exists(CODEX_SAFE, "Codex safe wrapper")
    proc = subprocess.run(
        [str(CODEX_SAFE), "--dangerously-bypass-approvals-and-sandbox", "--help"],
        text=True,
        capture_output=True,
        check=False,
    )
    assert proc.returncode != 0
    assert "blocked" in proc.stderr.lower()


def test_antigravity_operator_denies_off_limits_write() -> None:
    proc = subprocess.run(
        [
            str(DEVONX),
            "dispatch",
            "submit",
            "--json",
            "--from",
            "antigravity",
            "--action",
            "write-file",
            "--target-surface",
            "filesystem",
            "--target-scope",
            "/Users/dmcgregsauce/.gemini/settings.json",
            "--mode",
            "write",
        ],
        text=True,
        capture_output=True,
        check=False,
        env={
            **os.environ,
            "DEVON_OPERATOR_ROOT": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator",
        },
    )
    result = json.loads(proc.stdout)
    assert result["result"]["status"] == "denied"
