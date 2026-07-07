"""Tests for the opt-in Heiwa SDK tool sandbox policy."""
# pyright: reportMissingImports=false

from __future__ import annotations

import sys
from pathlib import Path

SDK_ROOT = Path(__file__).resolve().parents[2] / "packages" / "heiwa_sdk"
if str(SDK_ROOT) not in sys.path:
    sys.path.insert(0, str(SDK_ROOT))

from heiwa_sdk import tools  # noqa: E402
from heiwa_sdk.tools_policy import ToolPolicy, guarded_invoke  # noqa: E402


def test_run_command_denied_by_default(tmp_path: Path) -> None:
    policy = ToolPolicy(roots=[tmp_path.resolve()])

    result = guarded_invoke("run_command", policy=policy, command="echo hello", cwd=str(tmp_path))

    assert "error" in result
    assert "denied by policy" in result["error"]


def test_run_command_shell_metacharacters_are_not_interpreted(tmp_path: Path) -> None:
    marker = tmp_path / "should_not_exist"
    policy = ToolPolicy(
        roots=[tmp_path.resolve()],
        allow_exec=True,
        exec_allowlist=frozenset({"python3"}),
    )

    result = guarded_invoke(
        "run_command",
        policy=policy,
        command=f"python3 -c 'print(123)' ; touch {marker}",
        cwd=str(tmp_path),
    )

    assert result["success"] is True
    assert result["stdout"].strip() == "123"
    assert not marker.exists()


def test_run_command_non_allowlisted_executable_blocked(tmp_path: Path) -> None:
    policy = ToolPolicy(
        roots=[tmp_path.resolve()],
        allow_exec=True,
        exec_allowlist=frozenset({"echo"}),
    )

    result = guarded_invoke("run_command", policy=policy, command="curl https://example.com")

    assert "error" in result
    assert "not in allowlist" in result["error"]


def test_file_read_escape_is_blocked(tmp_path: Path) -> None:
    policy = ToolPolicy(roots=[tmp_path.resolve()])

    result = guarded_invoke("read_file", policy=policy, path="/etc/passwd")

    assert "error" in result
    assert "escapes sandbox" in result["error"]


def test_in_root_write_is_allowed(tmp_path: Path) -> None:
    policy = ToolPolicy(roots=[tmp_path.resolve()])
    target = tmp_path / "nested" / "note.txt"

    result = guarded_invoke("write_file", policy=policy, path=str(target), content="ok")

    assert result["success"] is True
    assert target.read_text() == "ok"


def test_legacy_invoke_tool_has_opt_in_policy_wiring(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.setenv("HEIWA_TOOLS_POLICY", "1")
    monkeypatch.setenv("HEIWA_TOOLS_ROOT", str(tmp_path))
    monkeypatch.delenv("HEIWA_TOOLS_ALLOW_EXEC", raising=False)

    result = tools.invoke_tool("run_command", command="echo hello", cwd=str(tmp_path))

    assert "error" in result
    assert "denied by policy" in result["error"]
