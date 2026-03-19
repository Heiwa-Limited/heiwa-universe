from __future__ import annotations

from pathlib import Path
import sys
import types

import pytest

from heiwa_protocol.protocol import Subject

ROOT = Path(__file__).resolve().parents[3]
sys.modules.setdefault("aiohttp", types.ModuleType("aiohttp"))
sys.path.insert(0, str(ROOT / "apps" / "heiwa_hub" / "scripts"))

import run_evolution as run_evolution_module  # noqa: E402


@pytest.mark.asyncio
async def test_benchmark_suite_uses_heiwa_bench(monkeypatch, tmp_path: Path) -> None:
    calls: list[tuple[str, object]] = []

    class _FakeHeiwaBench:
        def __init__(self, root_dir: Path | None = None) -> None:
            calls.append(("init", root_dir))

        def run(self, suite: str | None = None) -> dict[str, object]:
            calls.append(("run", suite))
            return {
                "ok": True,
                "suite": suite or "all",
                "total_cases": 4,
                "passed_cases": 3,
                "failed_cases": 1,
                "results": [],
                "failures": [],
            }

    monkeypatch.setattr(run_evolution_module, "HeiwaBench", _FakeHeiwaBench, raising=False)

    loop = run_evolution_module.HardenedKarpathyLoop(tmp_path)
    summary = await loop._benchmark_suite("routing_matrix")

    assert summary["ok"] is True
    assert summary["suite"] == "routing_matrix"
    assert summary["passed_cases"] == 3
    assert calls == [("init", tmp_path), ("run", "routing_matrix")]


@pytest.mark.asyncio
async def test_publish_learning_event_emits_knowledge_signal(monkeypatch, tmp_path: Path) -> None:
    events: list[tuple[Subject, dict[str, object], str | None]] = []

    class _FakeBus:
        async def publish(self, subject: Subject, data: dict[str, object], sender_id: str | None = None) -> None:
            events.append((subject, data, sender_id))

    monkeypatch.setattr(run_evolution_module, "get_bus", lambda: _FakeBus(), raising=False)

    loop = run_evolution_module.HardenedKarpathyLoop(tmp_path)
    benchmark_summary = {
        "ok": True,
        "suite": "routing_matrix",
        "total_cases": 4,
        "passed_cases": 4,
        "failed_cases": 0,
        "results": [],
        "failures": [],
    }

    event = await loop._publish_learning_event(
        target_name="IntentNormalizer",
        benchmark_summary=benchmark_summary,
        baseline=0.5,
        experiment=1.0,
    )

    assert events, "expected a knowledge learning event to be published"
    subject, payload, sender_id = events[0]
    assert subject is Subject.KNOWLEDGE_LEARN
    assert sender_id == "heiwa-karpathy-loop"
    assert payload == event
    assert payload["status"] == "PASS"
    assert payload["intent_class"] == "self_buff"
    assert "HeiwaBench" in payload["summary"]
    changed_paths = payload["decision_trace"]["artifacts"]["changed_paths"]
    assert "apps/heiwa_hub/scripts/run_evolution.py" in changed_paths
    assert "packages/heiwa_sdk/heiwa_sdk/bench.py" in changed_paths


@pytest.mark.asyncio
async def test_persist_cycle_run_writes_stdb_ledger_entry(monkeypatch, tmp_path: Path) -> None:
    captured: dict[str, dict[str, object]] = {}

    class _FakeDatabase:
        def __init__(self) -> None:
            self.stdb = object()

        def record_run(self, run_data: dict[str, object]) -> bool:
            captured["run_data"] = run_data
            return True

    monkeypatch.setattr(run_evolution_module, "Database", _FakeDatabase, raising=False)

    loop = run_evolution_module.HardenedKarpathyLoop(tmp_path)
    result = await loop._persist_cycle_run(
        target_name="IntentNormalizer",
        benchmark_summary={
            "ok": True,
            "suite": "routing_matrix",
            "total_cases": 4,
            "passed_cases": 4,
            "failed_cases": 0,
            "results": [],
            "failures": [],
        },
        baseline=0.5,
        experiment=1.0,
        status="PASS",
        started_at="2026-03-19T00:00:00+00:00",
        ended_at="2026-03-19T00:01:00+00:00",
    )

    assert result is True
    run_data = captured["run_data"]
    assert run_data["mode"] == "autoresearch"
    assert run_data["status"] == "PASS"
    assert run_data["chain_result"]["benchmark_summary"]["suite"] == "routing_matrix"
    assert run_data["signals"]["learning_event_emitted"] is True
    assert run_data["signals"]["benchmark_ok"] is True
