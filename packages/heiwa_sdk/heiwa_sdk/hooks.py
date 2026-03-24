import logging
import os
import json
from typing import Any, Dict, Optional, Tuple

logger = logging.getLogger("SDK.Hooks")

class ExecutionHookManager:
    """Manages pre and post tool execution hooks, enforcing leases and security gates."""

    def __init__(self, root_dir):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        self.root = root_dir
        self.db = SpacetimeDB(
            db_identity=os.environ.get("STDB_IDENTITY", "heiwaproductiondb"),
            server=os.environ.get("STDB_SERVER", "local")
        )

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

        # Scope validation: Tool Scope (Exact match)
        tool_scope = self._parse_json_field(lease.get("tool_scope"))
        if tool_scope and tool not in tool_scope:
            reason = f"Tool '{tool}' not authorized in lease tool_scope: {tool_scope}"
            if mode == "enforce":
                return False, reason, None
            logger.warning("[OBSERVE] %s", reason)

        # TODO: Implement complete scope matching semantics
        # - filesystem_scope: Path prefix match
        # - network_scope: Host/domain allowlist match
        # - secret_scope: Exact secret ID match
        for scope_name in ["filesystem_scope", "network_scope", "secret_scope"]:
            val = lease.get(scope_name)
            if val and val not in ("{}", "[]"):
                logger.warning("[TODO] Skipping %s verification for: %s", scope_name, val)

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
        import asyncio
        event_id = uuid.uuid4().hex[:12]
        
        def _commit_audit() -> None:
            try:
                self.db.register_artifact({
                    "artifact_id": f"audit-{event_id}",
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
                logger.error("Failed to persist execution audit to STDB: %s", e)
                
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
