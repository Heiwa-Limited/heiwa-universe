"""Persistent routing memory — records feedback to improve future model selection."""

from __future__ import annotations

import json
import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

logger = logging.getLogger("SDK")

_MEMORY_DIR = Path.home() / ".heiwa" / "memory"
_FEEDBACK_FILE = _MEMORY_DIR / "routing_feedback.jsonl"
_PREFS_FILE = _MEMORY_DIR / "model_preferences.json"


def _ensure_dir() -> None:
    _MEMORY_DIR.mkdir(parents=True, exist_ok=True)


def record_feedback(
    quality: str,
    intent_class: str | None = None,
    model: str | None = None,
    provider: str | None = None,
    prompt_snippet: str | None = None,
    note: str | None = None,
) -> dict[str, Any]:
    """Record a feedback signal. quality should be 'good' or 'bad'."""
    _ensure_dir()
    entry = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "quality": quality,
        "intent_class": intent_class or "",
        "model": model or "",
        "provider": provider or "",
        "prompt_snippet": (prompt_snippet or "")[:120],
        "note": note or "",
    }
    with _FEEDBACK_FILE.open("a", encoding="utf-8") as f:
        f.write(json.dumps(entry) + "\n")

    # Update preference scores
    if intent_class and model:
        _update_preferences(intent_class, model, quality)

    return entry


def _update_preferences(intent_class: str, model: str, quality: str) -> None:
    """Adjust model preference scores based on feedback."""
    prefs = load_preferences()
    key = intent_class.lower()
    if key not in prefs:
        prefs[key] = {}
    if model not in prefs[key]:
        prefs[key][model] = {"good": 0, "bad": 0, "score": 0.0}

    entry = prefs[key][model]
    if quality == "good":
        entry["good"] += 1
    else:
        entry["bad"] += 1
    total = entry["good"] + entry["bad"]
    entry["score"] = entry["good"] / total if total > 0 else 0.0

    _PREFS_FILE.write_text(json.dumps(prefs, indent=2), encoding="utf-8")


def load_preferences() -> dict[str, Any]:
    """Load model preferences per intent class."""
    if not _PREFS_FILE.exists():
        return {}
    try:
        return json.loads(_PREFS_FILE.read_text(encoding="utf-8"))
    except Exception:
        return {}


def preferred_model_for(intent_class: str) -> str | None:
    """Return the best-scoring model for an intent class, or None."""
    prefs = load_preferences()
    intent_prefs = prefs.get(intent_class.lower(), {})
    if not intent_prefs:
        return None

    best_model = None
    best_score = -1.0
    for model, data in intent_prefs.items():
        total = data.get("good", 0) + data.get("bad", 0)
        if total >= 2 and data.get("score", 0) > best_score:
            best_score = data["score"]
            best_model = model

    return best_model


def list_feedback(limit: int = 20) -> list[dict[str, Any]]:
    """Return recent feedback entries."""
    if not _FEEDBACK_FILE.exists():
        return []
    try:
        lines = _FEEDBACK_FILE.read_text(encoding="utf-8").strip().splitlines()
        entries = []
        for line in reversed(lines[-limit:]):
            try:
                entries.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        return entries
    except Exception:
        return []


def summary() -> dict[str, Any]:
    """Return a summary of feedback and preferences."""
    entries = list_feedback(limit=1000)
    prefs = load_preferences()
    good = sum(1 for e in entries if e.get("quality") == "good")
    bad = sum(1 for e in entries if e.get("quality") == "bad")
    return {
        "total_feedback": len(entries),
        "good": good,
        "bad": bad,
        "intent_preferences": {
            intent: {
                model: f"{data.get('score', 0):.0%} ({data.get('good', 0)}g/{data.get('bad', 0)}b)"
                for model, data in models.items()
            }
            for intent, models in prefs.items()
        },
    }
