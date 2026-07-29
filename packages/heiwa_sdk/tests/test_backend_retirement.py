from __future__ import annotations

import ast
import importlib.util
import subprocess
from pathlib import Path

import pytest

from heiwa_sdk.hooks import ExecutionHookManager
from heiwa_sdk.config import settings


REPO_ROOT = Path(__file__).resolve().parents[3]


def _tracked_python_files() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "*.py"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return [
        path
        for line in result.stdout.splitlines()
        if (path := REPO_ROOT / line).is_file()
    ]


def _spacetimedb_imports(paths: list[Path]) -> list[str]:
    matches: list[str] = []
    for path in paths:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names = [alias.name for alias in node.names]
            elif isinstance(node, ast.ImportFrom):
                names = [node.module or ""]
            else:
                continue
            if any("spacetimedb" in name for name in names):
                matches.append(str(path.relative_to(REPO_ROOT)))
    return matches


def test_retired_spacetimedb_bridge_is_not_importable() -> None:
    assert importlib.util.find_spec("heiwa_sdk.spacetimedb") is None


def test_live_python_has_no_spacetimedb_imports() -> None:
    assert _spacetimedb_imports(_tracked_python_files()) == []


def test_python_state_backend_defaults_to_local_jsonl(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("HEIWA_STATE_BACKEND", raising=False)
    assert settings.HEIWA_STATE_BACKEND == "local-jsonl"


def test_execution_hooks_fail_closed_without_runtime_lease_backend(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    monkeypatch.setenv("HEIWA_ROLLOUT_MODE", "enforce")
    hooks = ExecutionHookManager(tmp_path)

    allowed, reason, metadata = hooks.before_tool_call(
        tool="heiwa_code",
        proposal_id="proposal-1",
        node_id="local",
        payload={},
    )

    assert allowed is False
    assert reason == "Rust runtime lease backend unavailable"
    assert metadata is None
