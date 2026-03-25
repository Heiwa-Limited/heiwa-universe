from __future__ import annotations

import time
from typing import Any

from .db import Database


class HubStateService:
    """Shared state access for HTTP/MCP/websocket surfaces."""

    def __init__(self, db: Database | None = None) -> None:
        self.db = db or Database()

    def get_public_status(self, minutes: int = 60) -> dict[str, Any]:
        summary = self.db.get_model_usage_summary(minutes=minutes)
        live_nodes = self.db.list_nodes(status="ONLINE")
        return {
            "status": "OPERATIONAL",
            "state_backend": self.db.state_backend,
            "active_models": len(summary),
            "live_nodes": len(live_nodes),
            "usage_summary": summary,
            "timestamp": time.time(),
        }

    def get_recent_runs(self, limit: int = 10) -> list[dict[str, Any]]:
        return self.db.get_runs(limit=max(1, int(limit)))

    def get_provider_accounts(
        self,
        provider_id: str | None = None,
        node_id: str | None = None,
        user_id: str | None = None,
    ) -> list[dict[str, Any]]:
        return self.db.list_provider_accounts(
            provider_id=provider_id,
            node_id=node_id,
            limit=100,
            user_id=user_id,
        )

    def get_provider_status(self, provider_id: str, user_id: str | None = None) -> dict[str, Any] | None:
        rows = self.db.list_provider_accounts(provider_id=provider_id, limit=10, user_id=user_id)
        return rows[0] if rows else None

    def get_missions(
        self,
        status: str | None = None,
        limit: int = 50,
        user_id: str | None = None,
    ) -> list[dict[str, Any]]:
        return self.db.get_missions(status=status, limit=max(1, int(limit)), user_id=user_id)

    def get_mission_detail(self, mission_id: str, user_id: str | None = None) -> dict[str, Any] | None:
        mission = self.db.get_mission(mission_id, user_id=user_id)
        if not mission:
            return None
        mission["steps"] = self.db.get_mission_steps(mission_id, limit=100)
        mission["cell_runs"] = self.db.get_cell_runs(mission_id=mission_id, limit=100, user_id=user_id)
        mission["artifacts"] = self.db.list_artifacts(mission_id=mission_id, limit=100, user_id=user_id)
        return mission

    def pause_mission(self, mission_id: str, summary: str | None = None) -> bool:
        return self.db.pause_mission(mission_id, summary=summary)

    def resume_mission(self, mission_id: str, summary: str | None = None) -> bool:
        return self.db.resume_mission(mission_id, summary=summary)

    def get_history(self, limit: int = 20, user_id: str | None = None) -> dict[str, Any]:
        return {
            "session_summaries": self.db.list_session_summaries(limit=max(1, int(limit)), user_id=user_id),
            "completed_missions": self.db.get_missions(status="completed", limit=max(1, int(limit)), user_id=user_id),
            "failed_missions": self.db.get_missions(status="failed", limit=max(1, int(limit)), user_id=user_id),
            "artifacts": self.db.list_artifacts(limit=max(1, int(limit)), user_id=user_id),
            "recent_runs": self.db.get_runs(limit=max(1, int(limit)), user_id=user_id),
        }
