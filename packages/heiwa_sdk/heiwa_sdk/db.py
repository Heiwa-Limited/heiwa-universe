"""Authoritative state facade for Heiwa.

Rely strictly on SpacetimeDB as the mesh source of truth.
SQLite support has been retired to enforce single-ledger sovereignty.
"""
from __future__ import annotations

import json
import datetime
import hashlib
import sys
import uuid
import os
import logging
from pathlib import Path
from typing import Optional, List, Any, Dict, Union

from .config import settings
from .spacetimedb import SpacetimeDB

logger = logging.getLogger("SDK.Database")


class Database:
    def __init__(self):
        self.state_backend = settings.HEIWA_STATE_BACKEND
        self.stdb_identity = settings.STDB_IDENTITY
        self.stdb = None
        
        if self.stdb_identity:
            self.stdb = SpacetimeDB(
                db_identity=self.stdb_identity,
                server=settings.STDB_SERVER,
            )
            
        if self.state_backend == "spacetimedb" and not self.stdb:
            raise ValueError("STDB_IDENTITY is required when HEIWA_STATE_BACKEND=spacetimedb.")
        
        if self.state_backend != "spacetimedb":
            logger.warning("[DB] Non-STDB backend selected (%s). Operating in stateless mode.", self.state_backend)

    def init_db(self):
        """No-op in STDB-native mode."""
        if self.stdb:
            logger.info("STDB backend active; state sovereignty enforced.")
        else:
            logger.warning("[DB] No authoritative state backend available.")

    # ── Core Operations (STDB Delegates) ───────────────────────────────

    def record_run(self, run_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.record_run(run_data)
        return False

    def list_nodes(self, status: str | None = None) -> list[dict]:
        if self.stdb:
            try:
                where = f" WHERE status = '{status}'" if status else ""
                return self.stdb.query(f"SELECT * FROM nodes{where}")
            except Exception as e:
                logger.error("STDB list_nodes query failed: %s", e)
        return []

    def get_mission(self, mission_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_mission(mission_id)
        return None

    def get_missions(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_missions(status=status, limit=limit)
        return []

    def get_discord_channel(self, purpose: str) -> int | None:
        if self.stdb:
            return self.stdb.get_discord_channel(purpose)
        return None

    def upsert_node_heartbeat(
        self,
        *,
        node_id: str,
        meta: dict[str, Any] | None = None,
        capabilities: dict[str, Any] | None = None,
        agent_version: str | None = None,
        tags: list[str] | None = None,
        max_concurrency: int = 1,
    ) -> bool:
        if self.stdb:
            return self.stdb.upsert_node_heartbeat(
                node_id=node_id,
                meta=meta,
                capabilities=capabilities,
                agent_version=agent_version,
                tags=tags,
                max_concurrency=max_concurrency
            )
        return False

    def get_model_usage_summary(self, minutes: int = 60) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_model_usage_summary(minutes=minutes)
        return []
