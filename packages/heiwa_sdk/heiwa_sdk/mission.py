"""Mission Service: Manage long-running agent workflows in STDB."""
from __future__ import annotations

import datetime
import json
import uuid
from typing import Any, Optional

from heiwa_sdk.spacetimedb import SpacetimeDB


class MissionService:
    """Manages multi-step missions and their persistence in SpacetimeDB."""

    def __init__(self, stdb: SpacetimeDB) -> None:
        self.stdb = stdb

    def create_mission(
        self,
        mission_id: str,
        prompt: str,
        intent_class: str = "general",
        risk_level: str = "low",
        source_surface: str = "cli",
        node_id: str = "local",
    ) -> bool:
        """Initialize a new mission in STDB."""
        now = datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.stdb.call(
            "create_mission",
            mission_id,
            now,  # created_at
            now,  # updated_at
            "pending", # status
            source_surface,
            node_id,
            prompt,
            intent_class,
            risk_level,
            None, # active_step_id
            None, # active_cell_id
            None, # target_tool
            None, # target_model
            None, # summary
            None, # error
            "{}", # metadata_json
            self.stdb._sats_option(None),
        )

    def append_step(
        self,
        mission_id: str,
        title: str,
        step_kind: str = "tool_call",
        cell_role: str = "executor",
        detail: Optional[str] = None,
        input_data: Any = None,
        output_data: Any = None,
        status: str = "complete",
        position: int = 0,
    ) -> bool:
        """Add a progress step to an existing mission."""
        step_id = f"step-{uuid.uuid4().hex[:8]}"
        now = datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.stdb.call(
            "append_mission_step",
            step_id,
            mission_id,
            int(position),
            status,
            step_kind,
            cell_role,
            title,
            detail,
            json.dumps(input_data or {}),
            json.dumps(output_data or {}),
            now,  # created_at
            now,  # updated_at
        )
