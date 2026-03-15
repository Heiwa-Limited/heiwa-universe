#!/usr/bin/env python3
"""
Gate A1: cold build sanity.
Performs syntax and structure checks without external dependencies.
"""

from __future__ import annotations

import py_compile
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def tracked_python_files() -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "ls-files", "*.py"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        files = [ROOT / line.strip() for line in result.stdout.splitlines() if line.strip()]
        files = [path for path in files if path.exists()]
        return files
    except Exception:
        return sorted(ROOT.rglob("*.py"))


def compile_all(paths: list[Path]) -> list[str]:
    errors: list[str] = []
    for path in paths:
        try:
            py_compile.compile(str(path), doraise=True)
        except Exception as exc:
            errors.append(f"{path}: {exc}")
    return errors


def check_required_paths() -> list[str]:
    required = [
        ROOT / "apps/heiwa_hub/main.py",
        ROOT / "packages/heiwa_sdk/heiwa_sdk/config.py",
        ROOT / "apps/heiwa_cli/scripts/agents/sentinel.py",
        ROOT / "requirements.txt",
    ]
    missing = [str(p.relative_to(ROOT)) for p in required if not p.exists()]
    return missing


def check_wrapper_flags() -> list[str]:
    issues: list[str] = []
    wrapper_root = ROOT / "apps/heiwa_cli/scripts/agents/wrappers"
    codex_wrapper = wrapper_root / "codex_exec.sh"
    gemini_wrapper = wrapper_root / "gemini_exec.sh"
    claude_wrapper = wrapper_root / "claude_exec.sh"
    if codex_wrapper.exists():
        text = codex_wrapper.read_text(encoding="utf-8")
        if "--approval-mode" in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/codex_exec.sh uses deprecated --approval-mode flag")
        if 'HEIWA_CODEX_SANDBOX:-${CODEX_SANDBOX:-danger-full-access}' not in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/codex_exec.sh no longer defaults Codex to executive full-access sandboxing")
    if gemini_wrapper.exists():
        text = gemini_wrapper.read_text(encoding="utf-8")
        if "--approval-mode plan" in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/gemini_exec.sh hard-clamps Gemini CLI into read-only plan mode")
        if 'cd "$SCRIPT_DIR/../../../../.."' not in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/gemini_exec.sh falls back to the wrong workspace root")
    if claude_wrapper.exists():
        text = claude_wrapper.read_text(encoding="utf-8")
        if "--permission-mode plan" in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/claude_exec.sh hard-clamps Claude Code into read-only plan mode")
        if 'HEIWA_CLAUDE_PERMISSION_MODE:-${CLAUDE_PERMISSION_MODE:-bypassPermissions}' not in text:
            issues.append("apps/heiwa_cli/scripts/agents/wrappers/claude_exec.sh no longer defaults Claude to executive permissions")
    for wrapper in ("openclaw_exec.sh", "opencode_exec.sh", "antigravity_exec.sh"):
        path = wrapper_root / wrapper
        if path.exists():
            text = path.read_text(encoding="utf-8")
            if 'cd "$SCRIPT_DIR/../../../../.."' not in text and 'cd "$SCRIPT_DIR/../../../../.."' not in text.replace("$(dirname \"${BASH_SOURCE[0]}\")", "$SCRIPT_DIR"):
                issues.append(f"apps/heiwa_cli/scripts/agents/wrappers/{wrapper} falls back to the wrong workspace root")
    return issues


def main() -> int:
    failures: list[str] = []
    py_files = tracked_python_files()
    if not py_files:
        print("FAIL: no Python files found")
        return 1

    failures.extend(compile_all(py_files))
    missing = check_required_paths()
    failures.extend([f"missing required path: {m}" for m in missing])
    failures.extend(check_wrapper_flags())

    if failures:
        print("FAIL: gate_build")
        for item in failures:
            print(f"- {item}")
        return 1

    print("PASS: gate_build")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
