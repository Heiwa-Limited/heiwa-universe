import datetime
import json
import logging
import subprocess
from typing import Any, List

logger = logging.getLogger("SDK.SpacetimeDB")


def _parse_stdb_value(val: str) -> Any:
    """Parse a single value from STDB pipe-delimited output.

    Handles: quoted strings, integers, floats, booleans, empty → None.
    """
    if not val or val == "":
        return None
    # Quoted string — strip outer quotes
    if val.startswith('"') and val.endswith('"'):
        return val[1:-1]
    # Boolean
    if val.lower() == "true":
        return True
    if val.lower() == "false":
        return False
    # Number
    try:
        if "." in val:
            return float(val)
        return int(val)
    except ValueError:
        return val


class SpacetimeDB:
    """
    Heiwa SpacetimeDB bridge.

    This remains CLI-backed for now, but the API surface is typed around the
    control-plane entities Heiwa treats as authoritative.
    """

    def __init__(self, db_identity: str, server: str = "maincloud"):
        self.db_identity = db_identity
        self.server = server

    @staticmethod
    def _escape_sql_literal(value: str) -> str:
        return str(value).replace("'", "''")

    @staticmethod
    def _json_text(value: Any) -> str:
        if value is None:
            return ""
        return json.dumps(value, separators=(",", ":"))

    @staticmethod
    def _normalize_json_column(value: Any) -> str | None:
        if value is None:
            return None
        if isinstance(value, str):
            return value
        return json.dumps(value, separators=(",", ":"))

    def _run(self, cmd: list[str], timeout: int = 10, cwd: str | None = None) -> subprocess.CompletedProcess[str] | None:
        logger.debug("STDB cmd: %s (cwd=%s)", cmd, cwd)
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=cwd)
        except Exception as exc:
            logger.error("STDB bridge error: %s", exc)
            return None

        if result.returncode != 0:
            # Log full command to diagnose argument issues
            logger.error("STDB command failed (cmd=%r): %s", cmd, result.stderr.strip())
            return None
        return result

    def call(self, reducer_name: str, *args: Any) -> bool:
        if not self.db_identity:
            logger.warning("STDB call %s skipped: no db_identity", reducer_name)
            return False
            
        cmd = ["spacetime", "call", "--server", self.server, self.db_identity, reducer_name]
        for arg in args:
            cmd.append(json.dumps(arg))

        result = self._run(cmd)
        if not result:
            return False
        logger.debug("STDB call %s succeeded: %s", reducer_name, result.stdout.strip())
        return True

    def query(self, sql: str) -> List[dict]:
        if not self.db_identity:
            logger.warning("STDB query skipped: no db_identity")
            return []
            
        cmd = ["spacetime", "sql", "--server", self.server, self.db_identity, sql]
        result = self._run(cmd)
        if not result:
            return []

        stdout = result.stdout.strip()
        if not stdout:
            return []

        # STDB CLI 2.0 outputs pipe-delimited table format (no JSON flag available).
        # Parse the tabular output into list of dicts.
        return self._parse_table_output(stdout)

    @staticmethod
    def _parse_table_output(text: str) -> List[dict]:
        """Parse STDB CLI pipe-delimited table output into list of dicts.

        Format:
            col1 | col2 | col3
           ------+------+------
            val1 | val2 | val3
        """
        lines = [line for line in text.splitlines()
                 if line.strip() and not line.strip().startswith("WARNING:")]
        if len(lines) < 2:
            return []

        # Header is first line, separator (---+---) is second
        header_line = lines[0]
        headers = [h.strip() for h in header_line.split("|")]

        rows = []
        for line in lines[2:]:  # skip header + separator
            if not line.strip() or line.strip().startswith("-"):
                continue
            values = [v.strip() for v in line.split("|")]
            if len(values) != len(headers):
                continue
            row = {}
            for col, val in zip(headers, values):
                row[col] = _parse_stdb_value(val)
            rows.append(row)
        return rows

    def _first(self, sql: str) -> dict[str, Any] | None:
        rows = self.query(sql)
        return rows[0] if rows else None

    def record_route_decision(self, route: dict[str, Any]) -> bool:
        created_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "record_route_decision",
            route.get("request_id", ""),
            route.get("task_id", ""),
            route.get("envelope_version", ""),
            route.get("raw_text", ""),
            route.get("source_surface", "cli"),
            route.get("intent_class", "general"),
            route.get("risk_level", "low"),
            route.get("privacy_level", "local"),
            int(route.get("compute_class", 1)),
            route.get("assigned_worker", ""),
            route.get("target_tool", ""),
            route.get("target_model", ""),
            route.get("target_runtime", ""),
            route.get("target_tier", ""),
            bool(route.get("requires_approval", False)),
            route.get("rationale", ""),
            float(route.get("confidence", 0.0)),
            route.get("gateway_transport", "websocket"),
            created_at,
        )

    def record_run(self, run_data: dict[str, Any]) -> bool:
        ended_at = run_data.get("ended_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "record_run",
            run_data.get("run_id", ""),
            run_data.get("proposal_id", ""),
            run_data.get("started_at") or "",
            ended_at,
            run_data.get("status", "UNKNOWN"),
            self._json_text(run_data.get("chain_result")),
            self._json_text(run_data.get("signals")),
            self._json_text(run_data.get("artifact_index")),
            run_data.get("node_id") or "",
            self._json_text(run_data.get("replay_receipt")),
            run_data.get("mode") or "",
            run_data.get("model_id") or "",
            int(run_data.get("tokens_input") or 0),
            int(run_data.get("tokens_output") or 0),
            int(run_data.get("tokens_total") or 0),
            float(run_data.get("cost") or 0.0),
        )

    def get_runs(self, proposal_id: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        query = (
            "SELECT run_id, proposal_id, started_at, ended_at, status, "
            "chain_result_json AS chain_result, signals_json AS signals, "
            "artifact_index_json AS artifact_index, node_id, replay_receipt_json AS replay_receipt, "
            "mode, model_id, tokens_input, tokens_output, tokens_total, cost "
            "FROM runs"
        )
        if proposal_id:
            query += f" WHERE proposal_id = '{self._escape_sql_literal(proposal_id)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("ended_at", ""), reverse=True)

    def get_model_usage_summary(self, minutes: int = 60) -> list[dict[str, Any]]:
        cutoff = (
            datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=minutes)
        ).isoformat()
        # STDB 2.0 SQL doesn't support GROUP BY / COUNT / SUM — aggregate in Python
        rows = self.query(
            f"SELECT model_id, tokens_total, cost FROM runs "
            f"WHERE ended_at > '{self._escape_sql_literal(cutoff)}' AND model_id <> ''"
        )
        summary: dict[str, dict] = {}
        for r in rows:
            mid = r.get("model_id", "")
            if mid not in summary:
                summary[mid] = {"model_id": mid, "request_count": 0, "total_tokens": 0, "total_cost": 0.0}
            summary[mid]["request_count"] += 1
            summary[mid]["total_tokens"] += int(r.get("tokens_total", 0) or 0)
            summary[mid]["total_cost"] += float(r.get("cost", 0) or 0)
        return list(summary.values())

    def upsert_provider_account_status(self, account_data: dict[str, Any]) -> bool:
        updated_at = account_data.get("updated_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "upsert_provider_account_status",
            account_data["account_id"],
            account_data["provider_id"],
            account_data["node_id"],
            account_data.get("auth_kind", "unknown"),
            account_data.get("local_handle_ref", ""),
            account_data.get("status", "unknown"),
            account_data.get("display_name"),
            account_data.get("default_model"),
            account_data.get("rate_group", ""),
            self._normalize_json_column(account_data.get("available_models")) or "[]",
            account_data.get("last_validated_at"),
            account_data.get("last_error"),
            updated_at,
        )

    def list_provider_accounts(
        self,
        provider_id: str | None = None,
        node_id: str | None = None,
        status: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if provider_id:
            clauses.append(f"provider_id = '{self._escape_sql_literal(provider_id)}'")
        if node_id:
            clauses.append(f"node_id = '{self._escape_sql_literal(node_id)}'")
        if status:
            clauses.append(f"status = '{self._escape_sql_literal(status)}'")
        query = "SELECT * FROM provider_accounts"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("updated_at", ""), reverse=True)

    def get_provider_account(self, account_id: str) -> dict[str, Any] | None:
        return self._first(
            f"SELECT * FROM provider_accounts WHERE account_id = '{self._escape_sql_literal(account_id)}' LIMIT 1"
        )

    def create_mission(self, mission_data: dict[str, Any]) -> bool:
        created_at = mission_data.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        updated_at = mission_data.get("updated_at") or created_at
        return self.call(
            "create_mission",
            mission_data["mission_id"],
            created_at,
            updated_at,
            mission_data.get("status", "draft"),
            mission_data.get("source_surface", "cli"),
            mission_data.get("node_id", ""),
            mission_data.get("prompt", ""),
            mission_data.get("intent_class", "general"),
            mission_data.get("risk_level", "low"),
            mission_data.get("active_step_id"),
            mission_data.get("active_cell_id"),
            mission_data.get("target_tool"),
            mission_data.get("target_model"),
            mission_data.get("summary"),
            mission_data.get("error"),
            self._normalize_json_column(mission_data.get("metadata")) or "{}",
        )

    def get_missions(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        query = "SELECT * FROM missions"
        if status:
            query += f" WHERE status = '{self._escape_sql_literal(status)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("updated_at", ""), reverse=True)

    def get_mission(self, mission_id: str) -> dict[str, Any] | None:
        return self._first(
            f"SELECT * FROM missions WHERE mission_id = '{self._escape_sql_literal(mission_id)}' LIMIT 1"
        )

    def append_mission_step(self, step_data: dict[str, Any]) -> bool:
        created_at = step_data.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        updated_at = step_data.get("updated_at") or created_at
        return self.call(
            "append_mission_step",
            step_data["step_id"],
            step_data["mission_id"],
            int(step_data.get("position") or 0),
            step_data.get("status", "pending"),
            step_data.get("step_kind", "turn"),
            step_data.get("cell_role", ""),
            step_data.get("title", ""),
            step_data.get("detail"),
            self._normalize_json_column(step_data.get("input")) or "{}",
            self._normalize_json_column(step_data.get("output")) or "{}",
            created_at,
            updated_at,
        )

    def get_mission_steps(self, mission_id: str, limit: int = 100) -> list[dict[str, Any]]:
        rows = self.query(
            "SELECT * FROM mission_steps "
            f"WHERE mission_id = '{self._escape_sql_literal(mission_id)}' "
            f"LIMIT {int(limit)}"
        )
        return sorted(rows, key=lambda r: (r.get("position", 0), r.get("updated_at", "")))

    def start_cell_run(self, run_data: dict[str, Any]) -> bool:
        started_at = run_data.get("started_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "start_cell_run",
            run_data["cell_run_id"],
            run_data["mission_id"],
            run_data.get("step_id"),
            run_data.get("status", "running"),
            run_data.get("cell_id", ""),
            run_data.get("cell_role", ""),
            run_data.get("provider", ""),
            run_data.get("model", ""),
            run_data.get("rate_group", ""),
            run_data.get("tool", ""),
            started_at,
            int(run_data.get("tokens_input") or 0),
            int(run_data.get("tokens_output") or 0),
            int(run_data.get("tokens_total") or 0),
            run_data.get("output_summary"),
        )

    def finish_cell_run(self, run_data: dict[str, Any]) -> bool:
        return self.call(
            "finish_cell_run",
            run_data["cell_run_id"],
            run_data.get("status", "completed"),
            run_data.get("ended_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            int(run_data.get("tokens_input") or 0),
            int(run_data.get("tokens_output") or 0),
            int(run_data.get("tokens_total") or 0),
            run_data.get("output_summary"),
        )

    def get_cell_runs(
        self,
        mission_id: str | None = None,
        status: str | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if mission_id:
            clauses.append(f"mission_id = '{self._escape_sql_literal(mission_id)}'")
        if status:
            clauses.append(f"status = '{self._escape_sql_literal(status)}'")
        query = "SELECT * FROM cell_runs"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("started_at", ""), reverse=True)

    def pause_mission(self, mission_id: str, summary: str | None = None) -> bool:
        return self.call(
            "pause_mission",
            mission_id,
            datetime.datetime.now(datetime.timezone.utc).isoformat(),
            summary,
        )

    def resume_mission(self, mission_id: str, summary: str | None = None) -> bool:
        return self.call(
            "resume_mission",
            mission_id,
            datetime.datetime.now(datetime.timezone.utc).isoformat(),
            summary,
        )

    def complete_mission(self, mission_id: str, summary: str | None = None) -> bool:
        return self.call(
            "complete_mission",
            mission_id,
            datetime.datetime.now(datetime.timezone.utc).isoformat(),
            summary,
        )

    def fail_mission(self, mission_id: str, error: str | None = None) -> bool:
        return self.call(
            "fail_mission",
            mission_id,
            datetime.datetime.now(datetime.timezone.utc).isoformat(),
            error,
        )

    def write_session_summary(self, summary_data: dict[str, Any]) -> bool:
        created_at = summary_data.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "write_session_summary",
            summary_data["summary_id"],
            summary_data["session_id"],
            summary_data.get("node_id", ""),
            created_at,
            summary_data.get("summary_text", ""),
            self._normalize_json_column(summary_data.get("metadata")) or "{}",
        )

    def list_session_summaries(
        self,
        node_id: str | None = None,
        session_id: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if node_id:
            clauses.append(f"node_id = '{self._escape_sql_literal(node_id)}'")
        if session_id:
            clauses.append(f"session_id = '{self._escape_sql_literal(session_id)}'")
        query = "SELECT * FROM session_summaries"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("created_at", ""), reverse=True)

    def register_artifact(self, artifact_data: dict[str, Any]) -> bool:
        return self.call(
            "register_artifact",
            artifact_data["artifact_id"],
            artifact_data.get("mission_id", ""),
            artifact_data.get("cell_run_id"),
            artifact_data.get("artifact_type", "summary"),
            artifact_data.get("title", ""),
            artifact_data.get("uri"),
            artifact_data.get("path"),
            self._normalize_json_column(artifact_data.get("content")) or "{}",
            artifact_data.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
        )

    def list_artifacts(self, mission_id: str | None = None, limit: int = 100) -> list[dict[str, Any]]:
        query = "SELECT * FROM artifacts"
        if mission_id:
            query += f" WHERE mission_id = '{self._escape_sql_literal(mission_id)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("created_at", ""), reverse=True)

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
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        return self.call(
            "upsert_node_heartbeat",
            node_id,
            now_iso,
            now_iso,
            self._json_text(meta or {}),
            self._json_text(capabilities or {}),
            agent_version or "",
            self._json_text(tags or []),
            int(max_concurrency),
        )

    def set_node_status(self, node_id: str, status: str) -> bool:
        return self.call("set_node_status", node_id, status)

    def list_nodes(self, status: str | None = None) -> list[dict[str, Any]]:
        query = "SELECT * FROM nodes"
        if status:
            query += f" WHERE status = '{self._escape_sql_literal(status)}'"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("node_id", ""))

    def get_node(self, node_id: str) -> dict[str, Any] | None:
        return self._first(
            f"SELECT * FROM nodes WHERE node_id = '{self._escape_sql_literal(node_id)}' LIMIT 1"
        )

    def upsert_liveness_state(self, key: str, state: str, changed_at: str | None = None) -> bool:
        return self.call(
            "upsert_liveness_state",
            key,
            state,
            changed_at or datetime.datetime.now(datetime.timezone.utc).isoformat(),
        )

    def get_liveness_state(self, key: str) -> dict[str, Any] | None:
        return self._first(
            "SELECT key, last_state, last_changed_at FROM liveness_state "
            f"WHERE key = '{self._escape_sql_literal(key)}' LIMIT 1"
        )

    def add_proposal(self, proposal: dict[str, Any]) -> bool:
        payload = proposal.get("payload")
        payload_str = json.dumps(payload) if isinstance(payload, (dict, list)) else str(payload or "")
        return self.call(
            "add_proposal",
            proposal["proposal_id"],
            proposal.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            proposal.get("status", "QUEUED"),
            proposal.get("fingerprint"),
            payload_str,
            proposal.get("payload_raw"),
            proposal.get("mode", "PRODUCTION"),
            self._normalize_json_column(proposal.get("execution_targeting")),
            proposal.get("assigned_node_id"),
            proposal.get("hub_signature"),
            proposal.get("assignment_expires_at"),
            int(proposal.get("attempt_count") or 0),
            proposal.get("proposal_hash"),
            proposal.get("approved_at"),
            proposal.get("expires_at"),
            self._normalize_json_column(proposal.get("eligibility_snapshot")),
        )

    def get_proposals(self, status: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        query = "SELECT * FROM proposals"
        if status:
            query += f" WHERE status = '{self._escape_sql_literal(status)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("created_at", ""), reverse=True)

    def get_proposal(self, proposal_id: str) -> dict[str, Any] | None:
        return self._first(
            f"SELECT * FROM proposals WHERE proposal_id = '{self._escape_sql_literal(proposal_id)}' LIMIT 1"
        )

    def get_routable_proposals(self) -> list[dict[str, Any]]:
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        rows = self.query(
            "SELECT * FROM proposals "
            "WHERE status IN ('APPROVED', 'QUEUED') "
            f"AND (expires_at IS NULL OR expires_at = '' OR expires_at > '{self._escape_sql_literal(now_iso)}')"
        )
        return sorted(rows, key=lambda r: r.get("created_at", ""))

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
        return self.call(
            "assign_proposal",
            proposal_id,
            assigned_node_id,
            assignment_expires_at,
            hub_signature,
            proposal_hash,
            int(attempt_count),
            self._normalize_json_column(eligibility_snapshot),
        )

    def claim_proposal(
        self,
        proposal_id: str,
        node_id: str,
        claimed_at: str | None = None,
        lease_expires_at: str | None = None,
    ) -> dict[str, Any] | None:
        claimed_at = claimed_at or datetime.datetime.now(datetime.timezone.utc).isoformat()
        lease_expires_at = lease_expires_at or (
            datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=30)
        ).isoformat()
        if not self.call("claim_proposal", proposal_id, node_id, claimed_at, lease_expires_at):
            return None
        return self.get_proposal(proposal_id)

    def claim_next_approved_proposal(self, node_id: str) -> dict[str, Any] | None:
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        candidates = self.query(
            "SELECT * FROM proposals "
            "WHERE status = 'APPROVED' "
            f"AND (expires_at IS NULL OR expires_at = '' OR expires_at > '{self._escape_sql_literal(now_iso)}')"
        )
        candidates = sorted(candidates, key=lambda r: r.get("created_at", ""))
        proposal = candidates[0] if candidates else None
        if not proposal:
            return None
        return self.claim_proposal(proposal["proposal_id"], node_id)

    def approve_proposal(
        self,
        proposal_id: str,
        proposal_hash: str,
        approved_at: str,
        expires_at: str | None = None,
    ) -> bool:
        return self.call("approve_proposal", proposal_id, approved_at, expires_at, proposal_hash)

    def reject_proposal(self, proposal_id: str) -> bool:
        return self.call("reject_proposal", proposal_id)

    def queue_proposal(self, proposal_id: str, eligibility_snapshot: dict[str, Any] | str | None = None) -> bool:
        return self.call("queue_proposal", proposal_id, self._normalize_json_column(eligibility_snapshot))

    def expire_proposal(self, proposal_id: str, eligibility_snapshot: dict[str, Any] | str | None = None) -> bool:
        return self.call("expire_proposal", proposal_id, self._normalize_json_column(eligibility_snapshot))

    def requeue_proposal(self, proposal_id: str) -> bool:
        return self.call("requeue_proposal", proposal_id)

    def record_proposal_heartbeat(
        self,
        proposal_id: str,
        node_id: str,
        node_instance_id: str,
        ts_iso: str,
        detail: dict[str, Any] | None = None,
    ) -> dict[str, Any] | None:
        if not self.call(
            "record_proposal_heartbeat",
            proposal_id,
            node_id,
            node_instance_id,
            ts_iso,
            self._normalize_json_column(detail),
        ):
            return None
        return self.get_proposal(proposal_id)

    def record_consent(self, consent_data: dict[str, Any]) -> bool:
        metadata = consent_data.get("metadata")
        request_payload = consent_data.get("request_payload")
        return self.call(
            "record_consent",
            consent_data["consent_id"],
            consent_data["proposal_id"],
            consent_data.get("proposal_hash", "UNKNOWN"),
            consent_data["actor_type"],
            consent_data["actor_id"],
            str(consent_data["decision"]).upper(),
            consent_data.get("comment"),
            self._normalize_json_column(metadata) or "{}",
            consent_data.get("approval_request_id"),
            consent_data.get("requested_by"),
            consent_data.get("requested_at"),
            consent_data.get("request_expires_at"),
            consent_data.get("request_reason"),
            self._normalize_json_column(request_payload),
            consent_data.get("approved_at"),
            consent_data.get("expires_at"),
        )

    def get_consents_for_proposal(self, proposal_id: str) -> list[dict[str, Any]]:
        rows = self.query(
            "SELECT * FROM proposal_consents "
            f"WHERE proposal_id = '{self._escape_sql_literal(proposal_id)}'"
        )
        return sorted(rows, key=lambda r: r.get("created_at", ""))

    def add_approval_request(self, request_data: dict[str, Any]) -> bool:
        return self.call(
            "add_approval_request",
            request_data["request_id"],
            request_data["proposal_id"],
            request_data.get("status", "PENDING"),
            request_data.get("requested_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            request_data.get("expires_at"),
            request_data.get("requested_by", "heiwa-hub"),
            request_data.get("reason"),
            self._normalize_json_column(request_data.get("payload")) or "{}",
        )

    def record_approval_decision(self, decision_data: dict[str, Any]) -> bool:
        return self.call(
            "record_approval_decision",
            decision_data["decision_id"],
            decision_data["request_id"],
            decision_data["proposal_id"],
            decision_data["actor_type"],
            decision_data["actor_id"],
            str(decision_data["decision"]).upper(),
            decision_data.get("reason"),
            decision_data.get("created_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            self._normalize_json_column(decision_data.get("metadata")) or "{}",
        )

    def list_approval_requests(
        self,
        proposal_id: str | None = None,
        status: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if proposal_id:
            clauses.append(f"proposal_id = '{self._escape_sql_literal(proposal_id)}'")
        if status:
            clauses.append(f"status = '{self._escape_sql_literal(status)}'")
        query = "SELECT * FROM approval_requests"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("requested_at", ""), reverse=True)

    def list_approval_decisions(
        self,
        proposal_id: str | None = None,
        request_id: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if proposal_id:
            clauses.append(f"proposal_id = '{self._escape_sql_literal(proposal_id)}'")
        if request_id:
            clauses.append(f"request_id = '{self._escape_sql_literal(request_id)}'")
        query = "SELECT * FROM approval_decisions"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("created_at", ""), reverse=True)

    def issue_capability_lease(self, lease_data: dict[str, Any]) -> bool:
        return self.call(
            "issue_capability_lease",
            lease_data["lease_id"],
            lease_data["proposal_id"],
            lease_data.get("run_id"),
            lease_data.get("holder_kind", "node"),
            lease_data["holder_id"],
            self._normalize_json_column(lease_data.get("tool_scope")) or "[]",
            self._normalize_json_column(lease_data.get("network_scope")) or "{}",
            self._normalize_json_column(lease_data.get("filesystem_scope")) or "{}",
            self._normalize_json_column(lease_data.get("secret_scope")) or "[]",
            lease_data.get("privilege_tier", "cloud_safe"),
            lease_data.get("status", "ACTIVE"),
            lease_data.get("issued_at") or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            lease_data["expires_at"],
            lease_data.get("hub_signature", ""),
        )

    def renew_capability_lease(self, lease_id: str, renewed_at: str, expires_at: str) -> bool:
        return self.call("renew_capability_lease", lease_id, renewed_at, expires_at)

    def revoke_capability_lease(
        self,
        lease_id: str,
        revoked_at: str | None = None,
        revocation_reason: str | None = None,
    ) -> bool:
        return self.call(
            "revoke_capability_lease",
            lease_id,
            revoked_at or datetime.datetime.now(datetime.timezone.utc).isoformat(),
            revocation_reason,
        )

    def get_capability_leases(
        self,
        proposal_id: str | None = None,
        holder_id: str | None = None,
        status: str | None = None,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        clauses: list[str] = []
        if proposal_id:
            clauses.append(f"proposal_id = '{self._escape_sql_literal(proposal_id)}'")
        if holder_id:
            clauses.append(f"holder_id = '{self._escape_sql_literal(holder_id)}'")
        if status:
            clauses.append(f"status = '{self._escape_sql_literal(status)}'")
        query = "SELECT * FROM capability_leases"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("issued_at", ""), reverse=True)

    def get_active_capability_lease(self, proposal_id: str, holder_id: str) -> dict[str, Any] | None:
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        rows = self.query(
            "SELECT * FROM capability_leases "
            f"WHERE proposal_id = '{self._escape_sql_literal(proposal_id)}' "
            f"AND holder_id = '{self._escape_sql_literal(holder_id)}' "
            "AND status = 'ACTIVE' "
            f"AND expires_at > '{self._escape_sql_literal(now_iso)}'"
        )
        rows = sorted(rows, key=lambda r: r.get("issued_at", ""), reverse=True)
        return rows[0] if rows else None

    def get_discord_channel(self, purpose: str) -> int | None:
        row = self._first(
            "SELECT channel_id FROM discord_channels "
            f"WHERE purpose = '{self._escape_sql_literal(purpose)}' LIMIT 1"
        )
        if row and "channel_id" in row:
            return int(row["channel_id"])
        return None

    def get_user_trust(self, user_id: int) -> float:
        row = self._first(
            f"SELECT trust_score FROM discord_users WHERE user_id = {int(user_id)} LIMIT 1"
        )
        if row and "trust_score" in row:
            return float(row["trust_score"])
        return 0.5

    # ── Model Tiers ──────────────────────────────────────────────────────

    def get_model_tiers(self, *, capability_class: int | None = None,
                        enabled_only: bool = True) -> list[dict]:
        """Query model_tiers table, optionally filtered."""
        where_clauses = []
        if enabled_only:
            where_clauses.append("enabled = true")
        if capability_class is not None:
            where_clauses.append(f"capability_class = {capability_class}")
        where = f" WHERE {' AND '.join(where_clauses)}" if where_clauses else ""
        rows = self.query(
            f"SELECT * FROM model_tiers{where}"
        )
        return sorted(rows, key=lambda r: (r.get("effort_level", 0), r.get("cost_per_turn", 0.0)))

    def get_model_tier(self, model_id: str) -> dict | None:
        """Get a single model tier by model_id."""
        rows = self.query(
            f"SELECT * FROM model_tiers WHERE model_id = '{self._escape_sql_literal(model_id)}'"
        )
        return rows[0] if rows else None

    def upsert_model_tier(
        self,
        model_id: str,
        provider_model_id: str,
        provider: str,
        rate_group: str,
        capability_class: int,
        effort_knob: str,
        effort_level: int,
        cost_per_turn: float,
        max_context_tokens: int,
        strengths: list[str],
        enabled: bool,
    ) -> bool:
        """Insert or update a model tier."""
        import json
        return self.call(
            "upsert_model_tier",
            model_id,
            provider_model_id,
            provider,
            rate_group,
            capability_class,
            effort_knob,
            effort_level,
            cost_per_turn,
            max_context_tokens,
            json.dumps(strengths),
            enabled,
        )

    def update_model_tier_stats(
        self,
        model_id: str,
        success_rate: float,
        avg_latency_ms: int,
        latency_p95_ms: int,
    ) -> bool:
        """Update performance stats for a model tier (called by Captain)."""
        return self.call(
            "update_model_tier_stats",
            model_id,
            success_rate,
            avg_latency_ms,
            latency_p95_ms,
        )

    # ── Memory ───────────────────────────────────────────────────────────

    def insert_execution_memory(
        self,
        task_dispatch_id: str,
        model_used: str,
        outcome: str,
        duration_ms: int,
        error_summary: str | None = None,
        feedback_score: float | None = None,
    ) -> bool:
        """Insert a record into execution_memory."""
        return self.call(
            "insert_execution_memory",
            task_dispatch_id,
            model_used,
            outcome,
            int(duration_ms),
            error_summary,
            feedback_score,
        )

    def insert_knowledge_embedding(
        self,
        source_type: str,
        source_id: str,
        content_hash: str,
        embedding: list[float],
        ttl_hours: int = 0,
    ) -> bool:
        """Insert a record into knowledge_embeddings."""
        import json
        return self.call(
            "insert_knowledge_embedding",
            source_type,
            source_id,
            content_hash,
            json.dumps(embedding),
            int(ttl_hours),
        )

    def search_knowledge_embeddings(self, limit: int = 100) -> list[dict]:
        """
        Fetch knowledge embeddings for similarity search in Python.
        STDB SQL doesn't support vector search natively yet.
        """
        return self.query(f"SELECT * FROM knowledge_embeddings LIMIT {int(limit)}")

    def insert_captain_directive(
        self,
        directive_type: str,
        payload: dict[str, Any],
        priority: int = 1,
    ) -> bool:
        """Create a new proactive directive for the system to execute."""
        import json
        return self.call(
            "insert_captain_directive",
            directive_type,
            json.dumps(payload),
            int(priority),
        )

    # ── Captain Memory (Heiwa Agent conversation persistence) ──────

    def insert_captain_message(
        self,
        message_id: str,
        session_id: str,
        role: str,
        content: str,
        timestamp: int,
        source: str,
    ) -> bool:
        return self.call(
            "insert_captain_message",
            message_id, session_id, role, content, timestamp, source,
        )

    def get_uncompressed_messages(
        self,
        session_id: str | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        query = "SELECT * FROM captain_messages WHERE compressed = false"
        if session_id:
            query += f" AND session_id = '{self._escape_sql_literal(session_id)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("timestamp", 0))

    def mark_messages_compressed(self, session_id: str, before_timestamp: int) -> bool:
        return self.call("mark_messages_compressed", session_id, before_timestamp)

    def insert_captain_summary(
        self,
        summary_id: str,
        summary_type: str,
        content: str,
        range_start: int,
        range_end: int,
        messages_compressed: int,
    ) -> bool:
        return self.call(
            "insert_captain_summary",
            summary_id, summary_type, content, range_start, range_end, messages_compressed,
        )

    def get_recent_summaries(self, limit: int = 5) -> list[dict[str, Any]]:
        rows = self.query("SELECT * FROM captain_summaries")
        return sorted(rows, key=lambda r: r.get("created_at", 0), reverse=True)[:limit]

    def upsert_captain_focus(
        self,
        focus_id: str,
        topic: str,
        context_json: str,
        priority: int,
    ) -> bool:
        return self.call("upsert_captain_focus", focus_id, topic, context_json, int(priority))

    def resolve_captain_focus(self, focus_id: str, resolved_at: int) -> bool:
        return self.call("resolve_captain_focus", focus_id, resolved_at)

    def get_active_focuses(self) -> list[dict[str, Any]]:
        rows = self.query("SELECT * FROM captain_focus WHERE resolved_at = 0")
        return sorted(rows, key=lambda r: r.get("priority", 0), reverse=True)

    def publish_schema(self, module_dir: str) -> bool:
        """Publish the schema from the given directory to the server."""
        cmd = ["spacetime", "publish", "--server", self.server, self.db_identity]
        return self._run(cmd, timeout=60, cwd=module_dir) is not None

    def prune_captain_messages(self, before_timestamp: int) -> bool:
        return self.call("prune_captain_messages", before_timestamp)

    def prune_knowledge_embeddings(self, ttl_hours: int) -> bool:
        """Delete embeddings older than the specified TTL."""
        # STDB SQL doesn't support complex date math well, so we use a reducer if available
        # or a simple DELETE if we can compute the timestamp in Python.
        # For now, we scaffold the call.
        return self.call("prune_knowledge_embeddings", int(ttl_hours))

    def prune_execution_memory(self, keep_last: int) -> bool:
        """Keep only the N most recent execution memory records."""
        return self.call("prune_execution_memory", int(keep_last))
