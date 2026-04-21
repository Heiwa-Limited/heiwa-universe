"""Baseline smoke tests for Class 3 runtime safety."""
import json
import os
import subprocess
import pytest
from pathlib import Path


HOME = Path("/Users/dmcgregsauce")
GEMINI_POLICY = HOME / ".gemini/hooks/runtime_policy.js"
CLAUDE_POLICY = HOME / ".claude/plugins/devon-runtime-safety/hooks/pretool_policy.py"
CODEX_SAFE = HOME / ".codex/bin/codex-safe"
DEVONX = HOME / "heiwa_archive/heiwa-core/bin/devonx"


def run_json_command(cmd: list[str], payload: str | dict[str, object]) -> dict[str, object]:
    input_str = payload if isinstance(payload, str) else json.dumps(payload)
    proc = subprocess.run(
        cmd,
        input=input_str,
        text=True,
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        raise AssertionError(
            f"{cmd[0]} exited with {proc.returncode}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise AssertionError(
            f"{cmd[0]} did not emit valid JSON\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        ) from exc


def assert_exists(path: Path, label: str) -> None:
    assert path.exists(), f"Missing {label}: {path}"


def test_gemini_blocks_root_delete() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    payload = {"tool_name": "run_shell_command", "tool_input": {"command": "rm -rf /"}}
    result = run_json_command(["node", str(GEMINI_POLICY)], payload)
    assert result["decision"] == "deny"


def test_gemini_blocks_sensitive_write() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    payload = {"tool_name": "write_file", "tool_input": {"path": "/Users/dmcgregsauce/.ssh/config"}}
    result = run_json_command(["node", str(GEMINI_POLICY)], payload)
    assert result["decision"] == "deny"


def test_gemini_fails_closed_on_malformed_input() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    result = run_json_command(["node", str(GEMINI_POLICY)], "{ malformed json }")
    assert result["decision"] == "deny"
    assert "parse" in result["reason"].lower()


@pytest.mark.xfail(reason="Gemini lease-based allow is dormant until identity fields are stable")
def test_gemini_lease_bypass_is_dormant() -> None:
    assert_exists(GEMINI_POLICY, "Gemini policy hook")
    # This payload is dangerous but has dummy lease info; should still deny in Phase 1A
    payload = {
        "tool_name": "run_shell_command",
        "tool_input": {"command": "rm -rf /tmp/test"},
        "session_id": "test-session",
        "proposal_id": "test-proposal"
    }
    result = run_json_command(["node", str(GEMINI_POLICY)], payload)
    assert result["decision"] == "deny"


def test_claude_blocks_network_post() -> None:
    assert_exists(CLAUDE_POLICY, "Claude policy hook")
    payload = {"tool_name": "WebFetch", "tool_input": {"url": "https://example.com", "method": "POST"}}
    result = run_json_command(["python3", str(CLAUDE_POLICY)], payload)
    assert result["decision"] == "deny"


def test_claude_fails_closed_on_malformed_input() -> None:
    assert_exists(CLAUDE_POLICY, "Claude policy hook")
    result = run_json_command(["python3", str(CLAUDE_POLICY)], "{ malformed json }")
    assert result["decision"] == "deny"


@pytest.mark.xfail(reason="Claude lease-based allow is dormant until identity fields are stable")
def test_claude_lease_bypass_is_dormant() -> None:
    assert_exists(CLAUDE_POLICY, "Claude policy hook")
    payload = {
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf /tmp/test"},
        "session_id": "test-session"
    }
    result = run_json_command(["python3", str(CLAUDE_POLICY)], payload)
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
    denial = result["result"]
    assert denial["status"] == "denied"
    assert denial["executed_mode"] == "none"
    assert "missing required approval metadata" in denial["summary"]
