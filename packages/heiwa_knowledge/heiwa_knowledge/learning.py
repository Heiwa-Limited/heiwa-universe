from __future__ import annotations

import re
from typing import Any

from .registry import KnowledgeRegistry

_SECRET_MARKERS = ("api key", "token", "oauth", "credential", "secret")
_INTENT_ALLOWLIST = {"build", "operate", "self_buff", "fix"}
_TEMPLATES = (
    {
        "name": "stdb_over_compat",
        "keywords": ("spacetimedb", "sqlite"),
        "path_hints": ("db.py", "spacetimedb.py"),
        "title": "SpacetimeDB is the authoritative Heiwa state layer",
        "rule": "In Heiwa, route durable state and control-plane state through SpacetimeDB instead of compatibility backends.",
        "why": "The active runtime is STDB-native; compatibility backends create false confidence and drift.",
        "applies_to_intents": ["build", "operate", "self_buff"],
        "trigger_terms": ["spacetimedb", "sqlite", "state backend", "compatibility_sqlite"],
        "confidence": 92,
    },
    {
        "name": "sovereign_before_transport",
        "keywords": ("sovereign",),
        "path_hints": ("gateway.py", "delivery.py", "spine.py"),
        "title": "Sovereign routing clamps before transport selection",
        "rule": "In Heiwa, enforce sovereign privacy before transport selection and never allow cloud or ACP fallback for sovereign work.",
        "why": "Privacy guarantees must hold before the task leaves the trusted local boundary.",
        "applies_to_intents": ["build", "operate", "self_buff"],
        "trigger_terms": ["sovereign", "privacy router", "acp", "delivery"],
        "confidence": 91,
    },
    {
        "name": "native_cli_lanes",
        "keywords": ("wrapper", "native"),
        "path_hints": ("tool_mesh.py", "native_agents.py"),
        "title": "Provider CLIs run natively in the mesh",
        "rule": "In Heiwa, invoke provider CLIs directly through ToolMesh and native mesh agents instead of shell-wrapper shims.",
        "why": "Native lanes reduce indirection, drift, and wrapper-specific failure modes.",
        "applies_to_intents": ["build", "self_buff"],
        "trigger_terms": ["wrapper", "native cli", "toolmesh", "mesh agent"],
        "confidence": 88,
    },
    {
        "name": "harness_gate_scope",
        "keywords": ("harness", "progress"),
        "path_hints": ("executor.py", "progress.md"),
        "title": "Harness gates verify task-owned evidence only",
        "rule": "In Heiwa, harness gates must validate task-owned evidence and must not fail successful work because the repo is already dirty.",
        "why": "The monorepo is often intentionally dirty during active development, so global dirtiness is not a safe completion signal.",
        "applies_to_intents": ["build", "operate", "fix"],
        "trigger_terms": ["harness gate", "progress.md", "git status", "dirty worktree"],
        "confidence": 87,
    },
    {
        "name": "strict_mcp_scope",
        "keywords": ("mcp", "scope"),
        "path_hints": ("mcp_server.py", "ai_router.json", "orchestration.py"),
        "title": "MCP servers are intent-scoped",
        "rule": "In Heiwa, attach only the MCP servers explicitly scoped for the current intent and keep Heiwa-native tools as the baseline.",
        "why": "Unscoped MCP catalogs waste context and hide the route's true operating surface.",
        "applies_to_intents": ["research", "build", "strategy", "review", "self_buff"],
        "trigger_terms": ["mcp", "scope", "intent-scoped", "figma", "notion"],
        "confidence": 85,
    },
    {
        "name": "autoresearch_benchmark_loop",
        "keywords": ("heiwabench", "autoresearch"),
        "path_hints": ("run_evolution.py", "bench.py", "learning.py"),
        "title": "Autoresearch loops should target the canonical HeiwaBench surface",
        "rule": "In Heiwa, self-improvement loops should benchmark the canonical HeiwaBench surface and feed results into the learning pipeline instead of scoring isolated manifest scripts.",
        "why": "Narrow script-only loops learn the wrong layer and never improve the control plane.",
        "applies_to_intents": ["build", "self_buff"],
        "trigger_terms": ["heiwabench", "autoresearch", "suite-based", "knowledge_learn"],
        "confidence": 94,
    },
)


def should_learn_from_event(payload: dict[str, Any]) -> bool:
    if str(payload.get("status") or "").upper() not in {"PASS", "DELIVERED"}:
        return False
    intent = str(payload.get("intent_class") or "").strip().lower()
    return intent in _INTENT_ALLOWLIST


def extract_instinct_candidates(event: dict[str, Any]) -> list[dict[str, Any]]:
    trace = dict(event.get("decision_trace") or {})
    summary = " ".join(
        str(part or "").strip()
        for part in (
            event.get("summary"),
            trace.get("rationale"),
            ((trace.get("artifacts") or {}) if isinstance(trace.get("artifacts"), dict) else {}).get("summary"),
            event.get("progress"),
        )
        if str(part or "").strip()
    ).strip()
    lowered = summary.lower()
    changed_paths = _normalize_paths((trace.get("artifacts") or {}).get("changed_paths"))
    if not _looks_heiwa_specific(summary, changed_paths):
        return []
    if _contains_secret_material(summary, changed_paths):
        return []

    candidates: list[dict[str, Any]] = []
    for template in _TEMPLATES:
        if not _matches_template(template, lowered, changed_paths):
            continue
        candidate = {
            "title": template["title"],
            "applies_to_intents": list(template["applies_to_intents"]),
            "trigger_terms": list(template["trigger_terms"]),
            "rule": template["rule"],
            "why": template["why"],
            "confidence": int(template["confidence"]),
            "status": "active",
            "source_task_ids": [str(event.get("task_id") or "").strip()],
            "evidence_paths": changed_paths[:12],
            "last_validated_at": str(event.get("timestamp") or trace.get("timestamp") or ""),
            "body": _render_candidate_body(template["rule"], template["why"], summary, changed_paths),
        }
        candidates.append(candidate)

    deduped: list[dict[str, Any]] = []
    seen_hashes: set[str] = set()
    for candidate in candidates:
        rule_hash = KnowledgeRegistry.rule_hash(candidate["rule"])
        if rule_hash in seen_hashes:
            continue
        seen_hashes.add(rule_hash)
        deduped.append(candidate)
    return deduped


def _matches_template(template: dict[str, Any], lowered_summary: str, changed_paths: list[str]) -> bool:
    keywords = template.get("keywords") or ()
    if keywords and not all(str(keyword).lower() in lowered_summary for keyword in keywords):
        return False
    path_hints = template.get("path_hints") or ()
    if path_hints and not any(any(hint in path.lower() for hint in path_hints) for path in changed_paths):
        return False
    return True


def _looks_heiwa_specific(summary: str, changed_paths: list[str]) -> bool:
    lowered = summary.lower()
    if "heiwa" in lowered:
        return True
    return any(
        path.startswith(prefix)
        for path in changed_paths
        for prefix in (
            "apps/heiwa_hub/",
            "packages/heiwa_sdk/",
            "packages/heiwa_protocol/",
            "config/swarm/",
            "docs/superpowers/",
        )
    )


def _contains_secret_material(summary: str, changed_paths: list[str]) -> bool:
    lowered = summary.lower()
    if any(marker in lowered for marker in _SECRET_MARKERS):
        return True
    return any(KnowledgeRegistry._is_secret_path(path) for path in changed_paths)


def _normalize_paths(paths: Any) -> list[str]:
    if not isinstance(paths, list):
        return []
    cleaned: list[str] = []
    seen: set[str] = set()
    for value in paths:
        path = str(value or "").strip()
        if not path:
            continue
        key = path.lower()
        if key in seen:
            continue
        seen.add(key)
        cleaned.append(path)
    return cleaned


def _render_candidate_body(rule: str, why: str, summary: str, changed_paths: list[str]) -> str:
    evidence = ", ".join(changed_paths[:6]) if changed_paths else "no file evidence captured"
    compact_summary = re.sub(r"\s+", " ", summary).strip()
    if len(compact_summary) > 300:
        compact_summary = f"{compact_summary[:297]}..."
    return (
        f"When working in Heiwa, {rule}\n\n"
        f"Why: {why}\n\n"
        f"Evidence:\n"
        f"- Summary: {compact_summary}\n"
        f"- Paths: {evidence}\n"
    )
