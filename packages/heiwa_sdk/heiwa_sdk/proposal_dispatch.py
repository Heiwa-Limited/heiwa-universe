from __future__ import annotations

import datetime
import hashlib
import json
from typing import Any


def _parse_targeting(raw: Any) -> dict[str, Any]:
    if isinstance(raw, dict):
        return dict(raw)
    if isinstance(raw, str) and raw.strip():
        try:
            parsed = json.loads(raw)
            if isinstance(parsed, dict):
                return parsed
        except Exception:
            return {}
    return {}


def dispatch_routable_proposals(db: Any) -> dict[str, Any]:
    now = datetime.datetime.now(datetime.timezone.utc)
    now_iso = now.isoformat()
    result = {
        "status": "OK",
        "routed": 0,
        "queued": 0,
        "expired": 0,
        "errors": [],
    }

    try:
        proposals = db.get_routable_proposals()
        for proposal in proposals:
            proposal_id = proposal["proposal_id"]
            targeting = _parse_targeting(proposal.get("execution_targeting"))
            requires = targeting.get("requires", [])
            privilege_tier = targeting.get("privilege_tier", "cloud_safe")
            policy = targeting.get("policy", "QUEUE")
            eligible = db.get_eligible_nodes(requires, privilege_tier)

            if eligible:
                node = eligible[0]
                node_id = node["node_id"]
                assignment_ttl = targeting.get("assignment_ttl_seconds", 900)
                assignment_expires = (
                    now + datetime.timedelta(seconds=assignment_ttl)
                ).isoformat()
                payload_str = proposal.get("payload") or ""
                proposal_hash = (
                    proposal.get("proposal_hash")
                    or hashlib.sha256(str(payload_str).encode("utf-8")).hexdigest()
                )
                hub_signature = f"SIG-{proposal_hash[:16]}"
                attempt_count = int(proposal.get("attempt_count") or 0) + 1
                success = db.assign_proposal_to_node(
                    proposal_id,
                    node_id,
                    assignment_expires,
                    proposal_hash=proposal_hash,
                    hub_signature=hub_signature,
                    attempt_count=attempt_count,
                    eligibility_snapshot={
                        "eligible_count": len(eligible),
                        "assigned_to": node_id,
                        "timestamp": now_iso,
                    },
                )
                if success:
                    result["routed"] += 1
                else:
                    result["errors"].append(f"Failed to assign {proposal_id}")
                continue

            snapshot = {
                "eligible_count": 0,
                "assigned_to": None,
                "timestamp": now_iso,
            }
            if policy == "EXPIRE":
                db.expire_proposal(proposal_id, snapshot)
                result["expired"] += 1
            else:
                if proposal.get("status") != "QUEUED":
                    db.queue_proposal(proposal_id, snapshot)
                result["queued"] += 1
    except Exception as exc:
        result["status"] = "FAIL"
        result["errors"].append(f"Reactive dispatch error: {exc}")

    return result


def sync_remote_worker_node(
    db: Any,
    worker_id: str,
    capabilities: dict[str, Any] | None = None,
    *,
    status: str = "ONLINE",
) -> None:
    payload = dict(capabilities or {})
    capability_list = payload.get("capabilities")
    if not isinstance(capability_list, list):
        capability_list = []
    tags = payload.get("tags")
    if not isinstance(tags, list):
        tags = []
    meta = {
        "runtime": payload.get("runtime"),
        "node_id": payload.get("node_id") or worker_id,
        "ollama": bool(payload.get("ollama")),
        "privilege_tier": payload.get("privilege_tier") or "privileged_local",
    }
    db.upsert_node_heartbeat(
        node_id=worker_id,
        meta=meta,
        capabilities=capability_list,
        agent_version=str(payload.get("agent_version") or ""),
        tags=tags,
        max_concurrency=int(payload.get("max_concurrency") or 1),
    )
    db.set_node_status(worker_id, status)
