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
                return self.stdb.list_nodes(status=status)
            except Exception as e:
                logger.error("STDB list_nodes failed: %s", e)
        return []

    def list_provider_accounts(
        self,
        provider_id: str | None = None,
        node_id: str | None = None,
        status: str | None = None,
        limit: int = 50,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = dict(
                provider_id=provider_id,
                node_id=node_id,
                status=status,
                limit=limit,
            )
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.list_provider_accounts(**kwargs)
        return []

    def get_provider_account(self, account_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_provider_account(account_id)
        return None

    def upsert_provider_account_status(self, account_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.upsert_provider_account_status(account_data)
        return False

    def get_mission(self, mission_id: str, user_id: str | None = None, owner_id: str | None = None) -> dict[str, Any] | None:
        if self.stdb:
            kwargs: dict[str, Any] = {}
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.get_mission(mission_id, **kwargs)
        return None

    def get_missions(
        self,
        status: str | None = None,
        limit: int = 50,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = {"status": status, "limit": limit}
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.get_missions(**kwargs)
        return []

    def create_mission(self, mission_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.create_mission(mission_data)
        return False

    def append_mission_step(self, step_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.append_mission_step(step_data)
        return False

    def get_mission_steps(self, mission_id: str, limit: int = 100) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_mission_steps(mission_id, limit=limit)
        return []

    def start_cell_run(self, run_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.start_cell_run(run_data)
        return False

    def finish_cell_run(self, run_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.finish_cell_run(run_data)
        return False

    def get_cell_runs(
        self,
        mission_id: str | None = None,
        status: str | None = None,
        limit: int = 100,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = dict(
                mission_id=mission_id,
                status=status,
                limit=limit,
            )
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.get_cell_runs(**kwargs)
        return []

    def pause_mission(self, mission_id: str, summary: str | None = None) -> bool:
        if self.stdb:
            return self.stdb.pause_mission(mission_id, summary=summary)
        return False

    def resume_mission(self, mission_id: str, summary: str | None = None) -> bool:
        if self.stdb:
            return self.stdb.resume_mission(mission_id, summary=summary)
        return False

    def complete_mission(self, mission_id: str, summary: str | None = None) -> bool:
        if self.stdb:
            return self.stdb.complete_mission(mission_id, summary=summary)
        return False

    def fail_mission(self, mission_id: str, error: str | None = None) -> bool:
        if self.stdb:
            return self.stdb.fail_mission(mission_id, error=error)
        return False

    def set_mission_status(self, mission_id: str, status: str, summary: str | None = None) -> bool:
        normalized = str(status or "").strip().lower()
        if normalized in {"waiting_approval", "paused"}:
            return self.pause_mission(mission_id, summary=summary)
        if normalized in {"running", "resumed"}:
            return self.resume_mission(mission_id, summary=summary)
        if normalized in {"completed", "complete", "done"}:
            return self.complete_mission(mission_id, summary=summary)
        if normalized in {"failed", "error"}:
            return self.fail_mission(mission_id, error=summary)
        logger.warning("Unsupported mission status transition: %s for %s", status, mission_id)
        return False

    def write_session_summary(self, summary_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.write_session_summary(summary_data)
        return False

    def list_session_summaries(
        self,
        node_id: str | None = None,
        session_id: str | None = None,
        limit: int = 50,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = dict(
                node_id=node_id,
                session_id=session_id,
                limit=limit,
            )
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.list_session_summaries(**kwargs)
        return []

    def register_artifact(self, artifact_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.register_artifact(artifact_data)
        return False

    def list_artifacts(
        self,
        mission_id: str | None = None,
        limit: int = 100,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = {"mission_id": mission_id, "limit": limit}
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.list_artifacts(**kwargs)
        return []

    def get_discord_channel(self, purpose: str) -> int | None:
        if self.stdb:
            return self.stdb.get_discord_channel(purpose)
        return None

    def upsert_discord_channel(
        self,
        purpose: str,
        channel_id: int,
        *,
        category_name: str | None = None,
    ) -> bool:
        if self.stdb:
            metadata = {"category": category_name or "none"}
            return self.stdb.call(
                "register_discord_channel",
                int(channel_id),
                str(purpose),
                str(purpose),
                json.dumps(metadata),
            )
        return False

    def upsert_discord_role(self, role_name: str, role_id: int) -> bool:
        # Roles are inferred from Discord membership at runtime. Keep the old
        # facade method as a compatibility no-op so sync utilities do not crash.
        logger.info(
            "[DB] Discord role persistence is not modeled in STDB yet; keeping %s (%s) in compatibility mode.",
            role_name,
            role_id,
        )
        return True

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

    def record_route_decision(self, route_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.record_route_decision(route_data)
        return False

    def get_runs(
        self,
        proposal_id: str | None = None,
        limit: int = 50,
        user_id: str | None = None,
        owner_id: str | None = None,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            kwargs: dict[str, Any] = {"proposal_id": proposal_id, "limit": limit}
            if user_id is not None:
                kwargs["user_id"] = user_id
            if owner_id is not None:
                kwargs["owner_id"] = owner_id
            return self.stdb.get_runs(**kwargs)
        return []

    def get_run(self, run_id: str) -> dict[str, Any] | None:
        if self.stdb:
            rows = self.stdb.query(
                f"SELECT * FROM runs WHERE run_id = '{run_id}' LIMIT 1"
            )
            return rows[0] if rows else None
        return None

    def set_liveness_state(self, key: str, state: str, changed_at: str | None = None) -> bool:
        if self.stdb:
            return self.stdb.upsert_liveness_state(key, state, changed_at)
        return False

    def get_liveness_state(self, key: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_liveness_state(key)
        return None

    def get_node(self, node_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_node(node_id)
        return None

    def set_node_status(self, node_id: str, status: str) -> bool:
        if self.stdb:
            return self.stdb.set_node_status(node_id, status)
        return False

    def add_proposal(self, proposal: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.add_proposal(proposal)
        return False

    def get_proposal(self, proposal_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_proposal(proposal_id)
        return None

    def get_proposals(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_proposals(status=status, limit=limit)
        return []

    def get_routable_proposals(self) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_routable_proposals()
        return []

    def assign_proposal(
        self,
        proposal_id: str,
        assigned_node_id: str,
        assignment_expires_at: str,
        hub_signature: str,
        proposal_hash: str,
        attempt_count: int,
        eligibility_snapshot: dict[str, Any] | str | None = None,
    ) -> bool:
        if self.stdb:
            return self.stdb.assign_proposal(
                proposal_id, assigned_node_id, assignment_expires_at,
                hub_signature, proposal_hash, attempt_count, eligibility_snapshot,
            )
        return False

    def assign_proposal_to_node(
        self,
        proposal_id: str,
        node_id: str,
        assignment_expires_at: str,
        *,
        proposal_hash: str = "",
        hub_signature: str = "",
        attempt_count: int = 1,
        eligibility_snapshot: dict[str, Any] | str | None = None,
    ) -> bool:
        if not hub_signature:
            hub_signature = f"SIG-{proposal_hash or proposal_id}"
        return self.assign_proposal(
            proposal_id, node_id, assignment_expires_at,
            hub_signature, proposal_hash, attempt_count, eligibility_snapshot,
        )

    def claim_proposal(
        self,
        proposal_id: str,
        node_id: str,
        claimed_at: str | None = None,
        lease_expires_at: str | None = None,
    ) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.claim_proposal(proposal_id, node_id, claimed_at, lease_expires_at)
        return None

    def get_next_proposal(self, node_id: str) -> dict[str, Any] | None:
        if not self.stdb:
            return None
        result = self.stdb.claim_next_approved_proposal(node_id)
        if not result:
            return None
        targeting = result.get("execution_targeting")
        if isinstance(targeting, str):
            try:
                targeting = json.loads(targeting)
            except (json.JSONDecodeError, TypeError):
                targeting = {}
        lease_id = f"LEASE-{uuid.uuid4().hex[:12]}"
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        lease_expires = (
            datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=30)
        ).isoformat()
        lease_data = {
            "lease_id": lease_id,
            "proposal_id": result["proposal_id"],
            "parent_lease_id": (targeting or {}).get("parent_lease_id"),
            "holder_kind": "node",
            "holder_id": node_id,
            "tool_scope": (targeting or {}).get("allowed_tools", []),
            "network_scope": (targeting or {}).get("network_scope", {}),
            "filesystem_scope": (targeting or {}).get("filesystem_scope", {}),
            "secret_scope": (targeting or {}).get("secret_scope", []),
            "privilege_tier": (targeting or {}).get("privilege_tier", "cloud_safe"),
            "failure_policy": str((targeting or {}).get("failure_policy", "ABORT")).upper(),
            "chain_state": str((targeting or {}).get("chain_state", "ACTIVE")).upper(),
            "routing_lock": (targeting or {}).get("routing_lock"),
            "status": "ACTIVE",
            "issued_at": now_iso,
            "expires_at": lease_expires,
            "hub_signature": result.get("hub_signature", ""),
        }
        self.stdb.issue_capability_lease(lease_data)
        result["lease_id"] = lease_id
        return result

    def claim_for_node(self, node_id: str, max_items: int = 1) -> list[dict[str, Any]]:
        if not self.stdb:
            return []
        assigned = self.stdb.query(
            f"SELECT * FROM proposals WHERE status = 'ASSIGNED' "
            f"AND assigned_node_id = '{node_id}'"
        )
        assigned = sorted(assigned, key=lambda r: r.get("created_at", ""))
        claimed: list[dict[str, Any]] = []
        for row in assigned[:max_items]:
            result = self.stdb.claim_proposal(row["proposal_id"], node_id)
            if result:
                targeting = result.get("execution_targeting")
                if isinstance(targeting, str):
                    try:
                        targeting = json.loads(targeting)
                    except (json.JSONDecodeError, TypeError):
                        targeting = {}
                lease_id = f"LEASE-{uuid.uuid4().hex[:12]}"
                now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
                lease_expires = (
                    datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=30)
                ).isoformat()
                lease_data = {
                    "lease_id": lease_id,
                    "proposal_id": row["proposal_id"],
                    "parent_lease_id": (targeting or {}).get("parent_lease_id"),
                    "holder_kind": "node",
                    "holder_id": node_id,
                    "tool_scope": (targeting or {}).get("allowed_tools", []),
                    "network_scope": (targeting or {}).get("network_scope", {}),
                    "filesystem_scope": (targeting or {}).get("filesystem_scope", {}),
                    "secret_scope": (targeting or {}).get("secret_scope", []),
                    "privilege_tier": (targeting or {}).get("privilege_tier", "cloud_safe"),
                    "failure_policy": str((targeting or {}).get("failure_policy", "ABORT")).upper(),
                    "chain_state": str((targeting or {}).get("chain_state", "ACTIVE")).upper(),
                    "routing_lock": (targeting or {}).get("routing_lock"),
                    "status": "ACTIVE",
                    "issued_at": now_iso,
                    "expires_at": lease_expires,
                    "hub_signature": result.get("hub_signature", ""),
                }
                self.stdb.issue_capability_lease(lease_data)
                result["lease_id"] = lease_id
                result["lease_token"] = lease_id
                claimed.append(result)
        return claimed

    def get_eligible_nodes(
        self, required_capabilities: list[str], privilege_tier: str
    ) -> list[dict[str, Any]]:
        nodes = self.list_nodes(status="ONLINE")
        eligible = []
        for node in nodes:
            caps_raw = node.get("capabilities_json") or node.get("capabilities", "[]")
            if isinstance(caps_raw, str):
                try:
                    caps = json.loads(caps_raw)
                except (json.JSONDecodeError, TypeError):
                    caps = []
            else:
                caps = caps_raw
            meta_raw = node.get("meta_json") or node.get("meta", "{}")
            if isinstance(meta_raw, str):
                try:
                    meta = json.loads(meta_raw)
                except (json.JSONDecodeError, TypeError):
                    meta = {}
            else:
                meta = meta_raw
            if not all(cap in caps for cap in required_capabilities):
                continue
            node_tier = meta.get("privilege_tier", "cloud_safe")
            if node_tier != privilege_tier:
                continue
            eligible.append(node)
        return eligible

    def update_heartbeat(
        self,
        proposal_id: str,
        node_id: str,
        node_instance_id: str,
        ts_iso: str,
        detail: dict[str, Any] | None = None,
    ) -> tuple[dict[str, Any] | None, str | None]:
        if not self.stdb:
            return None, "No STDB backend"
        result = self.stdb.record_proposal_heartbeat(
            proposal_id, node_id, node_instance_id, ts_iso, detail,
        )
        if result is None:
            return None, "Heartbeat rejected"
        lease = self.stdb.get_active_capability_lease(proposal_id, node_id)
        if lease:
            now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
            new_expires = (
                datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=30)
            ).isoformat()
            self.stdb.renew_capability_lease(lease["lease_id"], now_iso, new_expires)
        return result, None

    def record_consent(self, consent_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.record_consent(consent_data)
        return False

    def add_approval_request(self, request_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.add_approval_request(request_data)
        return False

    def record_approval_decision(self, decision_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.record_approval_decision(decision_data)
        return False

    def list_approval_requests(
        self,
        proposal_id: str | None = None,
        status: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.list_approval_requests(
                proposal_id=proposal_id,
                status=status,
                limit=limit,
            )
        return []

    def list_approval_decisions(
        self,
        proposal_id: str | None = None,
        request_id: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.list_approval_decisions(
                proposal_id=proposal_id,
                request_id=request_id,
                limit=limit,
            )
        return []

    def get_consents_for_proposal(self, proposal_id: str) -> list[dict[str, Any]]:
        if self.stdb:
            return self.stdb.get_consents_for_proposal(proposal_id)
        return []

    def approve_proposal(
        self, proposal_id: str, proposal_hash: str, approved_at: str, expires_at: str | None = None,
    ) -> bool:
        if self.stdb:
            return self.stdb.approve_proposal(proposal_id, proposal_hash, approved_at, expires_at)
        return False

    def reject_proposal(self, proposal_id: str) -> bool:
        if self.stdb:
            return self.stdb.reject_proposal(proposal_id)
        return False

    def requeue_proposal(self, proposal_id: str) -> bool:
        if self.stdb:
            return self.stdb.requeue_proposal(proposal_id)
        return False

    def issue_capability_lease(self, lease_data: dict[str, Any]) -> bool:
        if self.stdb:
            return self.stdb.issue_capability_lease(lease_data)
        return False

    def get_active_capability_lease(self, proposal_id: str, holder_id: str) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.get_active_capability_lease(proposal_id, holder_id)
        return None

    def renew_capability_lease(self, lease_id: str, renewed_at: str, expires_at: str) -> bool:
        if self.stdb:
            return self.stdb.renew_capability_lease(lease_id, renewed_at, expires_at)
        return False

    def revoke_capability_lease(
        self, lease_id: str, revoked_at: str | None = None, revocation_reason: str | None = None,
    ) -> bool:
        if self.stdb:
            return self.stdb.revoke_capability_lease(lease_id, revoked_at, revocation_reason)
        return False

    def revoke_capability_chain(
        self, lease_id: str, revoked_at: str | None = None, revocation_reason: str | None = None,
    ) -> bool:
        if self.stdb:
            return self.stdb.revoke_capability_chain(lease_id, revoked_at, revocation_reason)
        return False

    def record_proposal_heartbeat(
        self,
        proposal_id: str,
        node_id: str,
        node_instance_id: str,
        ts_iso: str,
        detail: dict[str, Any] | None = None,
    ) -> dict[str, Any] | None:
        if self.stdb:
            return self.stdb.record_proposal_heartbeat(
                proposal_id, node_id, node_instance_id, ts_iso, detail,
            )
        return None

    def get_alerts(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        return []

    def update_alert_status(self, alert_id: str, status: str) -> bool:
        return False

    def scan_alerts(self, now: Any) -> list:
        return []

    def generate_proposals_from_alerts(self) -> list:
        return []

    def scan_lease_alerts(self, now: Any) -> list:
        return []

    def get_ops_snapshot(self, limit: int = 20) -> dict[str, Any]:
        return {"alerts": [], "claimed_proposals": []}


try:
    db = Database()
except Exception as e:
    logger.warning("[DB] Failed to create default Database instance: %s", e)
    db = None  # type: ignore[assignment]
