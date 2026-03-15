"""Identity selector — reads profiles.json and matches tasks to identity + cell chain."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger("Cognition.Identity")


@dataclass(slots=True)
class Cell:
    role: str
    model: str
    difficulty: str


@dataclass(slots=True)
class Identity:
    id: str
    description: str
    trigger_keywords: list[str]
    tool: str
    runtime: str
    required_capabilities: list[str]
    cells: list[Cell]
    models: dict[str, list[str]]
    discord_channel: str = ""
    report_channel: str = ""

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Identity:
        targets = data.get("targets", {})
        actions = data.get("actions", {})
        return cls(
            id=data["id"],
            description=data.get("description", ""),
            trigger_keywords=data.get("trigger_keywords", []),
            tool=targets.get("tool", "heiwa_claw"),
            runtime=targets.get("runtime", ""),
            required_capabilities=targets.get("required_capabilities", []),
            cells=[
                Cell(role=c["role"], model=c["model"], difficulty=c["difficulty"])
                for c in data.get("cells", [])
            ],
            models=data.get("models", {}),
            discord_channel=actions.get("discord_channel", ""),
            report_channel=actions.get("report_channel", ""),
        )


class IdentitySelector:
    """Selects the best identity profile and cell chain for a task.

    Reads profiles.json once at init. Matches by intent class → trigger keywords,
    then returns the identity with its cell hierarchy for step decomposition.
    """

    def __init__(self, profiles_path: Path | None = None) -> None:
        root = Path(__file__).resolve().parents[3]
        self.profiles_path = profiles_path or root / "config" / "identities" / "profiles.json"
        self._identities: list[Identity] = []
        self._default_id: str = "operator-general"
        self._load()

    def _load(self) -> None:
        try:
            data = json.loads(self.profiles_path.read_text(encoding="utf-8"))
            self._default_id = data.get("default_identity", "operator-general")
            self._identities = [
                Identity.from_dict(entry) for entry in data.get("identities", [])
            ]
        except Exception as exc:
            logger.warning("Failed to load profiles.json: %s", exc)

    def _by_id(self, identity_id: str) -> Identity | None:
        for identity in self._identities:
            if identity.id == identity_id:
                return identity
        return None

    def default(self) -> Identity | None:
        return self._by_id(self._default_id)

    def select(self, intent_class: str, raw_text: str = "") -> Identity:
        """Select identity by matching intent to trigger keywords and task domain.

        Falls back to operator-general if no specific match.
        """
        intent = str(intent_class or "").strip().lower()
        lowered = (raw_text or "").lower()

        # Direct intent → identity mapping (highest priority)
        _INTENT_MAP = {
            "build": "codex-builder",
            "research": "research-scout",
            "strategy": "architect-strategist",
        }
        mapped = _INTENT_MAP.get(intent)
        if mapped:
            identity = self._by_id(mapped)
            if identity:
                return identity

        # Keyword match against all identities (skip default, check specific ones first)
        best: Identity | None = None
        best_score = 0
        for identity in self._identities:
            if identity.id == self._default_id:
                continue
            score = sum(
                1 for kw in identity.trigger_keywords
                if kw.lower() in lowered or kw.lower() == intent
            )
            if score > best_score:
                best = identity
                best_score = score

        if best and best_score > 0:
            return best

        # Fall back to default
        default = self.default()
        if default:
            return default

        # Last resort: synthetic minimal identity
        return Identity(
            id="operator-general",
            description="Fallback identity",
            trigger_keywords=[],
            tool="heiwa_claw",
            runtime="railway",
            required_capabilities=[],
            cells=[Cell(role="scout", model="ollama/qwen3.5:4b", difficulty="low")],
            models={},
        )

    def all_identities(self) -> list[Identity]:
        return list(self._identities)
