"""Seed loader for an injected compatibility backend."""
from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


class SeedLoader:
    """Loads checked-in seed data through an injected backend."""

    def __init__(self, stdb: Any) -> None:
        self.stdb = stdb

    def seed_model_tiers(self, seed_path: Path) -> None:
        """Seed model_tiers from JSON. Skips if table already has data."""
        existing = self.stdb.get_model_tiers(enabled_only=False)
        if existing:
            logger.info("model_tiers already populated (%d rows), skipping seed.", len(existing))
            return

        with open(seed_path) as f:
            tiers = json.load(f)

        for tier in tiers:
            self.stdb.upsert_model_tier(
                model_id=tier["model_id"],
                provider_model_id=tier["provider_model_id"],
                provider=tier["provider"],
                rate_group=tier["rate_group"],
                capability_class=tier["capability_class"],
                effort_knob=tier["effort_knob"],
                effort_level=tier["effort_level"],
                cost_per_turn=tier["cost_per_turn"],
                max_context_tokens=tier["max_context_tokens"],
                vram_requirement_mb=tier.get("vram_requirement_mb", 0),
                quantization_type=tier.get("quantization_type", "n/a"),
                kv_cache_strategy=tier.get("kv_cache_strategy", "n/a"),
                strengths=tier.get("strengths", []),
                enabled=tier.get("enabled", True),
            )

        logger.info("Seeded %d model tiers from %s", len(tiers), seed_path.name)

    def seed_rate_groups(self, router_path: Path) -> None:
        """Seed rate_group_state from ai_router.json rate_limits section."""
        with open(router_path) as f:
            router = json.load(f)

        for group, limits in router.get("rate_limits", {}).items():
            self.stdb.call(
                "upsert_rate_group_state",
                group,
                0,  # turns_used
                limits["max_turns"],
                limits["window_sec"],
                "",  # cooldown_until (empty = not cooling)
                True,  # available
            )

        logger.info("Seeded %d rate groups.", len(router.get("rate_limits", {})))
