from __future__ import annotations

import subprocess
import time
from pathlib import Path
from typing import Any

from heiwa_sdk.security import redact_text

_SECRET_PATH_MARKERS = (
    "infra/local/sovereign",
    ".codex",
    ".gemini",
    ".claude",
    ".ssh",
    ".env",
    "credentials",
    "token",
    "secret",
)


def build_task_finality_evidence(root: Path, snapshot: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    route = dict(snapshot.get("route") or {})
    normalization = dict(route.get("normalization") or {})
    git_state = _git_state(root)
    status = str(payload.get("status") or "").upper()
    summary = _clean_text(payload.get("summary"))

    return {
        "task_id": str(payload.get("task_id") or snapshot.get("task_id") or "").strip(),
        "intent_class": str(payload.get("intent_class") or snapshot.get("intent_class") or route.get("intent_class") or "").strip(),
        "agent": str(payload.get("provider") or payload.get("target_tool") or snapshot.get("provider") or "unknown").strip(),
        "event": "task_completed",
        "rationale": _clean_text(route.get("rationale") or snapshot.get("message")),
        "artifacts": {
            "summary": summary[:1200],
            "status": status,
            "diff_stat": git_state["diff_stat"],
            "changed_paths": git_state["changed_paths"],
            "verification_outcomes": _verification_outcomes(payload, status),
            "knowledge_refs": list(normalization.get("knowledge_refs") or []),
            "knowledge_brief": _clean_text(normalization.get("knowledge_brief")),
        },
        "timestamp": float(payload.get("timestamp") or time.time()),
    }


def build_learning_event(snapshot: dict[str, Any], payload: dict[str, Any], decision_trace: dict[str, Any]) -> dict[str, Any]:
    route = dict(snapshot.get("route") or {})
    normalization = dict(route.get("normalization") or {})
    return {
        "task_id": str(payload.get("task_id") or "").strip(),
        "status": str(payload.get("status") or "").strip(),
        "intent_class": str(payload.get("intent_class") or route.get("intent_class") or "").strip(),
        "summary": str(payload.get("summary") or "").strip(),
        "decision_trace": decision_trace,
        "knowledge_refs": list(normalization.get("knowledge_refs") or []),
        "knowledge_brief": str(normalization.get("knowledge_brief") or "").strip(),
        "progress": str(snapshot.get("progress") or "").strip(),
        "runtime": str(payload.get("runtime") or snapshot.get("runtime") or "").strip(),
        "timestamp": decision_trace.get("timestamp"),
    }


def _git_state(root: Path) -> dict[str, Any]:
    diff_stat = _run_git(root, "diff", "--stat")
    staged_diff_stat = _run_git(root, "diff", "--cached", "--stat")
    changed_paths = _sanitize_paths(_parse_paths(_run_git(root, "diff", "--name-only")))
    changed_paths.extend(path for path in _sanitize_paths(_parse_paths(_run_git(root, "diff", "--cached", "--name-only"))) if path not in changed_paths)
    changed_paths.extend(path for path in _sanitize_paths(_parse_paths(_run_git(root, "ls-files", "--others", "--exclude-standard"))) if path not in changed_paths)
    combined_stat = _sanitize_diff_stat("\n".join(part for part in (diff_stat, staged_diff_stat) if part).strip())
    return {
        "diff_stat": combined_stat[:4000],
        "changed_paths": changed_paths[:40],
    }


def _run_git(root: Path, *args: str) -> str:
    try:
        proc = subprocess.run(
            ["git", *args],
            cwd=str(root),
            capture_output=True,
            text=True,
            check=False,
        )
    except Exception:
        return ""
    return proc.stdout.strip() if proc.returncode == 0 else ""


def _parse_paths(stdout: str) -> list[str]:
    return [line.strip() for line in str(stdout or "").splitlines() if line.strip()]


def _sanitize_paths(paths: list[str]) -> list[str]:
    return [path for path in paths if not _is_secret_path(path)]


def _sanitize_diff_stat(diff_stat: str) -> str:
    if not diff_stat:
        return ""
    safe_lines = [
        redact_text(line)
        for line in str(diff_stat).splitlines()
        if line.strip() and not _is_secret_path(line)
    ]
    return "\n".join(safe_lines)


def _clean_text(value: Any) -> str:
    return redact_text(str(value or "").strip())


def _is_secret_path(value: str) -> bool:
    lowered = str(value or "").strip().lower()
    return any(marker in lowered for marker in _SECRET_PATH_MARKERS)


def _verification_outcomes(payload: dict[str, Any], status: str) -> list[dict[str, Any]]:
    outcomes = [{"name": "task_status", "passed": status in {"PASS", "DELIVERED"}}]
    for artifact in payload.get("artifacts") or []:
        kind = str(artifact.get("kind") or "").strip()
        if kind == "harness_gate_fail":
            outcomes.append({"name": "harness_gate", "passed": False})
    summary = str(payload.get("summary") or "").lower()
    if "passed" in summary and not any(item.get("name") == "summary_pass_hint" for item in outcomes):
        outcomes.append({"name": "summary_pass_hint", "passed": True})
    return outcomes
