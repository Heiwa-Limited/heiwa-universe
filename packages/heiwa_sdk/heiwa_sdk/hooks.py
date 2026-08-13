import logging
import os
import json
from typing import Any, Dict, Optional, Tuple

logger = logging.getLogger("SDK.Hooks")

class ExecutionHookManager:
    """Manages pre and post tool execution hooks, enforcing leases and security gates."""

    def __init__(self, root_dir, backend: Any | None = None):
        self.root = root_dir
        self.db = backend

    def before_tool_call(
        self, 
        tool: str, 
        proposal_id: str, 
        node_id: str, 
        payload: dict
    ) -> Tuple[bool, str, Optional[dict]]:
        """
        Pre-execution validation gate executing rollout mode guidelines.
        Returns (allow, reason, append_metadata).
        """
        mode = os.getenv("HEIWA_ROLLOUT_MODE", "observe")  # enforce | observe
        logger.info("Hook pre-check tool=%s proposal=%s node=%s mode=%s", tool, proposal_id, node_id, mode)

        if self.db is None:
            reason = "Rust runtime lease backend unavailable"
            if mode == "enforce":
                return False, reason, None
            logger.warning("[OBSERVE] %s", reason)
            return True, reason, None

        try:
            lease = self.db.get_active_capability_lease(proposal_id, node_id)
        except Exception as e:
            logger.error("Internal hook failure during lease lookup: %s", e)
            # Internal errors fail-closed if in enforce mode
            if mode == "enforce":
                return False, f"Internal hook failure: {e}", None
            return True, f"Internal lookup failed (observing): {e}", None

        if not lease:
            reason = f"No active lease found for proposal={proposal_id} on node={node_id}."
            if mode == "enforce":
                return False, reason, None
            logger.warning("[OBSERVE] %s", reason)
            return True, reason, None

        expected_scope_fields = (
            "tool_scope_json",
            "filesystem_scope_json",
            "network_scope_json",
            "secret_scope_json",
        )
        for field_name in expected_scope_fields:
            if field_name not in lease or lease.get(field_name) is None:
                reason = f"Missing lease scope field: {field_name}"
                if mode == "enforce":
                    return False, reason, None
                logger.warning("[OBSERVE] %s", reason)

        # Scope validation: Tool Scope (Exact match)
        tool_scope = self._parse_json_field(lease.get("tool_scope_json"))
        if tool_scope and tool not in tool_scope:
            reason = f"Tool '{tool}' not authorized in lease tool_scope: {tool_scope}"
            if mode == "enforce":
                return False, reason, None
            logger.warning("[OBSERVE] %s", reason)

        # TODO: Implement complete scope matching semantics
        # - filesystem_scope: Path prefix match
        # - network_scope: Host/domain allowlist match
        # - secret_scope: Exact secret ID match
        for scope_name in [
            "filesystem_scope_json",
            "network_scope_json",
            "secret_scope_json",
        ]:
            val = lease.get(scope_name)
            if val and val not in ("{}", "[]"):
                logger.warning("[TODO] Skipping %s verification for: %s", scope_name, val)

        routing_lock = self._parse_json_field(lease.get("routing_lock_json"))
        if routing_lock:
            lock_mismatches = self._routing_lock_mismatches(routing_lock, payload)
            if lock_mismatches:
                reason = f"Routing lock mismatch: {', '.join(lock_mismatches)}"
                if mode == "enforce":
                    return False, reason, None
                logger.warning("[OBSERVE] %s", reason)

        return True, "Authorized", {"lease_id": lease.get("lease_id")}

    def after_tool_call(
        self, 
        tool: str, 
        proposal_id: str, 
        exit_code: int, 
        output: str, 
        audit_metadata: Optional[dict] = None
    ) -> Optional[dict]:
        """Post-execution log appending and state updates."""
        logger.info("Hook post-exec tool=%s proposal=%s exit_code=%d", tool, proposal_id, exit_code)
        
        mode = os.getenv("HEIWA_ROLLOUT_MODE", "observe")
        import uuid
        event_id = uuid.uuid4().hex[:12]

        if self.db is None:
            logger.warning("Execution audit not recorded: Rust runtime evidence backend unavailable")
            return {
                "audit_ts": event_id,
                "status": "not_recorded",
                "reason": "rust_runtime_backend_unavailable",
            }
        
        def _commit_audit() -> None:
            try:
                self.db.register_artifact({
                    "artifact_id": f"audit-{event_id}",
                    "lease_id": (audit_metadata or {}).get("lease_id"),
                    "mission_id": proposal_id,
                    "artifact_type": "execution_audit",
                    "title": f"Execution ({mode}): {tool}",
                    "content": {
                        "tool": tool,
                        "exit_code": exit_code,
                        "output_preview": output[:1500] if output else "",
                        "metadata": audit_metadata or {},
                        "hook_mode": mode
                    }
                })
            except Exception as e:
                logger.error("Failed to persist execution audit: %s", e)
                
        import threading
        threading.Thread(target=_commit_audit, daemon=True).start()
        
        return {"audit_ts": event_id, "status": "logged"}

    @staticmethod
    def _parse_json_field(field_val: Any) -> Any:
        if isinstance(field_val, str):
            try:
                return json.loads(field_val)
            except Exception:
                return []
        return field_val or []

    @staticmethod
    def _routing_lock_mismatches(routing_lock: Any, payload: dict[str, Any]) -> list[str]:
        if not isinstance(routing_lock, dict):
            return []

        comparisons = {
            "model_id": payload.get("target_model") or payload.get("model") or payload.get("model_id"),
            "provider": payload.get("provider"),
            "runtime": payload.get("target_runtime") or payload.get("runtime"),
        }
        mismatches: list[str] = []
        for key, actual in comparisons.items():
            expected = routing_lock.get(key)
            if expected is None:
                continue
            if actual is None:
                mismatches.append(f"{key} missing")
            elif str(actual) != str(expected):
                mismatches.append(f"{key} expected={expected} actual={actual}")
        return mismatches
