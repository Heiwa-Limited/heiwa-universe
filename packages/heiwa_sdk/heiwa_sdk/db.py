"""Authoritative state facade for Heiwa.

Delegates to SpacetimeDB when HEIWA_STATE_BACKEND=spacetimedb.
Falls back to local SQLite for legacy scripts and offline mode.
Postgres support has been retired.
"""
from __future__ import annotations

import sqlite3
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
        
        self.db_path = settings.DATABASE_PATH
        self.use_postgres = False 

    def init_db(self):
        """Bootstraps the database schema on startup (SQLite fallback only)."""
        if self.state_backend == "spacetimedb":
            logger.info("STDB backend selected; skipping compatibility SQL schema bootstrap.")
            return
        
        logger.info("[DB] Initializing compatibility SQLite schema.")
        self._ensure_persistence_check()
        self._init_sqlite_schema()
        logger.info("[DB] Compatibility schema check completed.")

    def _ensure_persistence_check(self):
        path = Path(self.db_path)
        try:
            if not path.parent.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
            path.touch(exist_ok=True)
        except Exception as e:
            logger.critical("Cannot access DATABASE_PATH %s: %s", self.db_path, e)
            sys.exit(1)

    def get_connection(self):
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        return conn

    def _row_to_dict(self, row, cursor=None):
        return dict(row) if row else None

    def _rows_to_dicts(self, rows, cursor=None):
        return [dict(row) for row in rows]

    def _exec(self, cursor, query, params=None):
        if params:
            cursor.execute(query, params)
        else:
            cursor.execute(query)

    # ── Core Operations ────────────────────────────────────────────────

    def record_run(self, run_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.record_run(run_data)
        
        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            self._exec(cursor, """
                INSERT INTO runs (run_id, proposal_id, status, model_id, tokens_total, cost)
                VALUES (?, ?, ?, ?, ?, ?)
            """, (
                run_data.get("run_id"),
                run_data.get("proposal_id"),
                run_data.get("status"),
                run_data.get("model_id"),
                run_data.get("tokens_total", 0),
                run_data.get("cost", 0.0)
            ))
            conn.commit()
            return True
        except Exception as e:
            logger.error("SQLite record_run failed: %s", e)
            return False
        finally:
            conn.close()

    def list_nodes(self, status: str | None = None) -> list[dict]:
        if self.stdb:
            # We'll need a get_nodes method in spacetimedb.py eventually
            # For now, fallback to query if it exists there
            try:
                where = f" WHERE status = '{status}'" if status else ""
                return self.stdb.query(f"SELECT * FROM node_registry{where}")
            except Exception:
                pass

        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            query = "SELECT * FROM nodes"
            params = []
            if status:
                query += " WHERE status = ?"
                params.append(status)
            self._exec(cursor, query, tuple(params))
            return self._rows_to_dicts(cursor.fetchall())
        finally:
            conn.close()

    def get_mission(self, mission_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_mission(mission_id)
            
        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            self._exec(cursor, "SELECT * FROM missions WHERE mission_id = ?", (mission_id,))
            return self._row_to_dict(cursor.fetchone())
        finally:
            conn.close()

    def get_missions(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_missions(status=status, limit=limit)
            
        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            query = "SELECT * FROM missions"
            params = []
            if status:
                query += " WHERE status = ?"
                params.append(status)
            query += f" LIMIT {int(limit)}"
            self._exec(cursor, query, tuple(params))
            return self._rows_to_dicts(cursor.fetchall())
        finally:
            conn.close()

    def get_discord_channel(self, purpose: str) -> int | None:
        if self.stdb:
            return self.stdb.get_discord_channel(purpose)
        
        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            self._exec(cursor, "SELECT channel_id FROM discord_channels WHERE purpose = ?", (purpose,))
            row = cursor.fetchone()
            return int(row["channel_id"]) if row else None
        finally:
            conn.close()

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
        return True # SQLite fallback doesn't track nodes yet

    def get_model_usage_summary(self, minutes: int = 60) -> dict[str, Any]:
        if self.stdb:
            # Future: add get_model_usage_summary to SpacetimeDB
            return {}
        return {}

    # ── Internal Schemas ───────────────────────────────────────────────

    def _init_sqlite_schema(self):
        conn = self.get_connection()
        try:
            cursor = conn.cursor()
            # Minimal legacy schema for fallback
            cursor.executescript("""
                CREATE TABLE IF NOT EXISTS nodes (
                    node_id TEXT PRIMARY KEY,
                    status TEXT,
                    last_seen TEXT
                );
                CREATE TABLE IF NOT EXISTS runs (
                    run_id TEXT PRIMARY KEY,
                    proposal_id TEXT,
                    status TEXT,
                    model_id TEXT,
                    tokens_total INTEGER,
                    cost REAL
                );
                CREATE TABLE IF NOT EXISTS missions (
                    mission_id TEXT PRIMARY KEY,
                    status TEXT,
                    title TEXT,
                    summary TEXT
                );
                CREATE TABLE IF NOT EXISTS discord_channels (
                    purpose TEXT PRIMARY KEY,
                    channel_id TEXT
                );
            """)
            conn.commit()
        finally:
            conn.close()
