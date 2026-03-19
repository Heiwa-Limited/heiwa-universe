#!/usr/bin/env python3
"""
Heiwa autoresearch loop.

Runs a tiny isolated mutation cycle against the canonical Heiwa benchmark
surface, then publishes a learning event when the experiment beats baseline.
"""

from __future__ import annotations

import asyncio
from datetime import datetime, timezone
import logging
import socket
import shutil
import sys
import time
import uuid
from pathlib import Path
from typing import Any

# Ensure monorepo roots are on sys.path.
ROOT = Path(__file__).resolve().parents[3]
sys.path.append(str(ROOT / "apps"))
sys.path.append(str(ROOT / "packages/heiwa_sdk"))
sys.path.append(str(ROOT / "packages/heiwa_cognition"))
sys.path.append(str(ROOT / "packages/heiwa_knowledge"))

from heiwa_hub.knowledge_pipeline import build_learning_event
from heiwa_hub.transport import get_bus
from heiwa_protocol.protocol import Subject
from heiwa_sdk.db import Database
from heiwa_sdk.bench import HeiwaBench

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("Heiwa.Evolution")


class HardenedKarpathyLoop:
    def __init__(self, root: Path):
        self.root = root
        self.evolution_id = str(uuid.uuid4())[:8]
        self.worktree_path = Path(f"/tmp/heiwa-evolution-{self.evolution_id}")
        self.program_path = root / "docs" / "superpowers" / "status" / "evolution_program.md"
        self.target_suite = "routing_matrix"
        self.manifest_targets = {
            "IntentNormalizer": "packages/heiwa_cognition/heiwa_cognition/intent_manifest.py",
            "ComputeRouter": "packages/heiwa_cognition/heiwa_cognition/router_manifest.py",
        }

    async def run_cycle(self) -> None:
        logger.info("Starting autoresearch cycle %s", self.evolution_id)
        await self._setup_worktree()

        started_at = self._now_iso()
        target_name = "IntentNormalizer"
        target_rel_path = self.manifest_targets[target_name]
        benchmark_summary: dict[str, Any] = {
            "ok": False,
            "suite": self.target_suite,
            "total_cases": 0,
            "passed_cases": 0,
            "failed_cases": 0,
            "results": [],
            "failures": [],
        }
        baseline_score = 0.0
        experiment_score = 0.0
        status = "FAIL"
        cycle_error: str | None = None
        try:
            worktree_file = self.worktree_path / target_rel_path

            baseline_summary = await self._benchmark_suite(self.target_suite)
            baseline_score = self._benchmark_score(baseline_summary)
            logger.info("Baseline score: %.4f", baseline_score)

            logger.info("Applying mutation to %s...", target_rel_path)
            self._mutate_manifest(worktree_file)

            benchmark_summary = await self._benchmark_suite(self.target_suite)
            experiment_score = self._benchmark_score(benchmark_summary)
            logger.info("Experiment score: %.4f", experiment_score)

            if experiment_score > baseline_score:
                status = "PASS"
                logger.info("Improvement detected; publishing learning event.")
                shutil.copy(worktree_file, self.root / target_rel_path)
                await self._publish_learning_event(
                    target_name=target_name,
                    benchmark_summary=benchmark_summary,
                    baseline=baseline_score,
                    experiment=experiment_score,
                )
                self._log_result(target_name, baseline_score, experiment_score, status)
            else:
                status = "FAIL"
                logger.info("No improvement; discarding worktree.")
                self._log_result(target_name, baseline_score, experiment_score, status)
        except Exception as exc:
            cycle_error = repr(exc)
            logger.exception("Autoresearch cycle %s failed", self.evolution_id)
            raise
        finally:
            await self._persist_cycle_run(
                target_name=target_name,
                benchmark_summary=benchmark_summary,
                baseline=baseline_score,
                experiment=experiment_score,
                status=status,
                started_at=started_at,
                ended_at=self._now_iso(),
                error=cycle_error,
            )
            await self._cleanup()

    async def _setup_worktree(self) -> None:
        logger.info("Creating isolated worktree at %s", self.worktree_path)
        proc = await asyncio.create_subprocess_exec(
            "git",
            "worktree",
            "add",
            "-d",
            str(self.worktree_path),
            cwd=str(self.root),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

    async def _benchmark_suite(self, suite: str | None = None) -> dict[str, Any]:
        bench = HeiwaBench(self.root)
        return bench.run(suite=suite)

    @staticmethod
    def _benchmark_score(summary: dict[str, Any]) -> float:
        total_cases = int(summary.get("total_cases") or 0)
        passed_cases = int(summary.get("passed_cases") or 0)
        if total_cases <= 0:
            return 1.0 if summary.get("ok") else 0.0
        return passed_cases / total_cases

    async def _publish_learning_event(
        self,
        *,
        target_name: str,
        benchmark_summary: dict[str, Any],
        baseline: float,
        experiment: float,
    ) -> dict[str, Any]:
        suite_name = str(benchmark_summary.get("suite") or self.target_suite or "all")
        task_id = f"evolution-{self.evolution_id}"
        score_summary = (
            f"HeiwaBench suite {suite_name} improved from {baseline:.2%} to {experiment:.2%}."
        )
        event_summary = (
            f"HeiwaBench drove the autoresearch loop for {target_name} on {suite_name}. "
            f"{score_summary}"
        )
        payload = {
            "task_id": task_id,
            "status": "PASS",
            "intent_class": "self_buff",
            "summary": event_summary,
            "runtime": "railway",
        }
        decision_trace = {
            "task_id": task_id,
            "intent_class": "self_buff",
            "agent": "heiwa-karpathy-loop",
            "event": "task_completed",
            "rationale": score_summary,
            "artifacts": {
                "summary": event_summary,
                "changed_paths": [
                    "apps/heiwa_hub/scripts/run_evolution.py",
                    "packages/heiwa_sdk/heiwa_sdk/bench.py",
                    "apps/heiwa_hub/agents/learning.py",
                ],
                "verification_outcomes": [
                    {"name": "benchmark_score", "passed": experiment > baseline},
                    {"name": "benchmark_ok", "passed": bool(benchmark_summary.get("ok"))},
                ],
            },
            "timestamp": time.time(),
        }
        snapshot = {
            "task_id": task_id,
            "intent_class": "self_buff",
            "runtime": "railway",
            "progress": score_summary,
            "route": {
                "intent_class": "self_buff",
                "normalization": {
                    "knowledge_refs": [],
                    "knowledge_brief": "Heiwa autoresearch loops should benchmark the canonical HeiwaBench surface and feed learning events.",
                },
            },
        }
        event = build_learning_event(snapshot, payload, decision_trace)
        await get_bus().publish(Subject.KNOWLEDGE_LEARN, event, sender_id="heiwa-karpathy-loop")
        return event

    async def _persist_cycle_run(
        self,
        *,
        target_name: str,
        benchmark_summary: dict[str, Any],
        baseline: float,
        experiment: float,
        status: str,
        started_at: str,
        ended_at: str,
        error: str | None = None,
    ) -> bool:
        run_data = {
            "run_id": f"autoresearch-{self.evolution_id}",
            "proposal_id": target_name,
            "started_at": started_at,
            "ended_at": ended_at,
            "status": status,
            "chain_result": {
                "target_name": target_name,
                "suite": benchmark_summary.get("suite"),
                "ok": benchmark_summary.get("ok"),
                "baseline_score": baseline,
                "experiment_score": experiment,
                "benchmark_summary": benchmark_summary,
                "error": error,
            },
            "signals": {
                "benchmark_ok": bool(benchmark_summary.get("ok")),
                "learning_event_emitted": status == "PASS",
                "error": error or "",
            },
            "artifact_index": {
                "benchmark_suite": benchmark_summary.get("suite"),
                "changed_paths": [
                    "apps/heiwa_hub/scripts/run_evolution.py",
                    "packages/heiwa_sdk/heiwa_sdk/bench.py",
                    "apps/heiwa_hub/knowledge_pipeline.py",
                    "apps/heiwa_hub/agents/learning.py",
                ],
            },
            "node_id": self._node_id(),
            "replay_receipt": {
                "worktree_path": str(self.worktree_path),
            },
            "mode": "autoresearch",
            "model_id": "heiwa-karpathy-loop",
            "tokens_input": 0,
            "tokens_output": 0,
            "tokens_total": 0,
            "cost": 0.0,
        }
        try:
            db = Database()
        except Exception as exc:
            logger.info("Skipping STDB persistence for %s: %s", target_name, exc)
            return False
        if not getattr(db, "stdb", None):
            logger.info("Skipping STDB persistence for %s: no STDB client", target_name)
            return False
        return bool(db.record_run(run_data))

    def _mutate_manifest(self, file_path: Path) -> None:
        content = file_path.read_text(encoding="utf-8")
        if '"build", "fabricate"' in content:
            return
        if '"build"' in content:
            mutated = content.replace('"build"', '"build", "fabricate"', 1)
        else:
            mutated = content + '\n# autoresearch mutation marker\n'
        file_path.write_text(mutated, encoding="utf-8")

    async def _cleanup(self) -> None:
        logger.info("Cleaning up worktree...")
        proc = await asyncio.create_subprocess_exec(
            "git",
            "worktree",
            "remove",
            "--force",
            str(self.worktree_path),
            cwd=str(self.root),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()
        if self.worktree_path.exists():
            shutil.rmtree(self.worktree_path)

    def _log_result(self, target: str, baseline: float, score: float, status: str) -> None:
        log_entry = (
            f"\n- [{status}] {target} ({self.evolution_id}): "
            f"{baseline:.4f} -> {score:.4f} ({time.strftime('%Y-%m-%d %H:%M')})"
        )
        with open(self.program_path, "a", encoding="utf-8") as handle:
            handle.write(log_entry)

    @staticmethod
    def _now_iso() -> str:
        return datetime.now(timezone.utc).isoformat()

    @staticmethod
    def _node_id() -> str:
        return socket.gethostname() or "local"


async def main() -> None:
    loop = HardenedKarpathyLoop(ROOT)
    await loop.run_cycle()


if __name__ == "__main__":
    asyncio.run(main())
