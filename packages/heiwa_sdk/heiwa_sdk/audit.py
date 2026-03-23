"""Repo Auditor: run lint and tests to verify system integrity."""
from __future__ import annotations

import asyncio
import logging
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

logger = logging.getLogger("SDK.Audit")


@dataclass
class AuditResult:
    passed: bool
    lint_ok: bool
    tests_ok: bool
    summary: str
    errors: list[str]


class RepoAuditor:
    """Runs automated health checks on the repository."""

    def __init__(self, root: Path) -> None:
        self.root = root

    async def run_audit(self) -> AuditResult:
        """Run all audit checks."""
        logger.info("Starting repository audit...")
        
        lint_ok, lint_summary = await self._run_lint()
        tests_ok, test_summary = await self._run_smoke_tests()
        ollama_summary = await self._run_ollama_audit()
        
        errors = []
        if not lint_ok:
            errors.append(f"Lint failure: {lint_summary}")
        if not tests_ok:
            errors.append(f"Test failure: {test_summary}")
            
        passed = lint_ok and tests_ok
        status_line = f"Lint: {'OK' if lint_ok else 'FAIL'}, Tests: {'OK' if tests_ok else 'FAIL'}"
        summary = f"Audit {'PASSED' if passed else 'FAILED'}. {status_line}\n\nOllama Report: {ollama_summary}"
        
        return AuditResult(
            passed=passed,
            lint_ok=lint_ok,
            tests_ok=tests_ok,
            summary=summary,
            errors=errors
        )

    async def _run_ollama_audit(self) -> str:
        """Use local LLM to perform a high-level structural health check."""
        try:
            # Only run LLM audit when local Ollama is available — skip on Railway
            # to avoid burning Gemini API quota on a cosmetic "vibe check"
            if os.environ.get("RAILWAY_ENVIRONMENT") or os.environ.get("RAILWAY_PROJECT_ID"):
                return ""
            
            # Gather repo stats
            file_count = 0
            for root, _, files in os.walk(str(self.root)):
                if ".git" in root or ".venv" in root or "node_modules" in root:
                    continue
                file_count += len(files)
            
            prompt = (
                f"You are the Heiwa Repo Auditor. The repository at {self.root} has {file_count} active files.\n"
                "Based on the system integrity markers (Lint and Tests passed), provide a one-sentence natural status report "
                "on the 'vibe' of the codebase. Be observant and machine-astute."
            )

            from heiwa_cognition.llm import llm_generate_async

            return await llm_generate_async(prompt=prompt, intent="audit", risk="low", runtime="macbook")
        except Exception as e:
            return f"Ollama audit failed: {e}"

    async def _run_lint(self) -> tuple[bool, str]:
        """Run the configuration linter."""
        script = self.root / "apps/heiwa_cli/scripts/lint_config.py"
        if not script.exists():
            return False, "lint_config.py script not found"
            
        try:
            proc = await asyncio.create_subprocess_exec(
                sys.executable, str(script),
                cwd=str(self.root),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            stdout, stderr = await proc.communicate()
            if proc.returncode == 0:
                return True, "Lint passed"
            else:
                err = (stdout + stderr).decode(errors="ignore").strip()
                return False, f"Lint failed (exit {proc.returncode}): {err[:200]}"
        except Exception as e:
            return False, f"Lint error: {e}"

    async def _run_smoke_tests(self) -> tuple[bool, str]:
        """Run a minimal subset of tests as a smoke check."""
        try:
            # Run one simple test file that doesn't need external services
            test_file = "apps/heiwa_hub/tests/test_cli_import_chain.py"
            proc = await asyncio.create_subprocess_exec(
                sys.executable, "-m", "pytest", test_file, "-v",
                cwd=str(self.root),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env={**os.environ, "PYTHONPATH": self._get_pythonpath()}
            )
            stdout, stderr = await proc.communicate()
            if proc.returncode == 0:
                return True, "Smoke tests passed"
            else:
                return False, f"Smoke tests failed (exit {proc.returncode})"
        except Exception as e:
            return False, f"Test error: {e}"

    def _get_pythonpath(self) -> str:
        pkg_paths = [str(self.root / "packages" / p) for p in
                     ["heiwa_cli", "heiwa_cognition", "heiwa_sdk", "heiwa_protocol", "heiwa_identity", "heiwa_ui"]]
        pkg_paths.append(str(self.root / "apps"))
        return ":".join(pkg_paths)
