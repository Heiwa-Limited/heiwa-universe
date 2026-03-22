from __future__ import annotations

import copy
import datetime
import json

from heiwa_sdk.proposal_dispatch import dispatch_routable_proposals


class FakeDB:
    def __init__(self) -> None:
        self.nodes = [
            {
                "node_id": "macbook@heiwa-node-a",
                "status": "ONLINE",
                "capabilities_json": json.dumps(["shell", "build"]),
                "meta_json": json.dumps({"privilege_tier": "privileged_local"}),
            }
        ]
        self.proposals = [
            {
                "proposal_id": "prop-reactive-1",
                "status": "APPROVED",
                "payload": json.dumps({"task": "reactive-assign"}),
                "execution_targeting": json.dumps(
                    {
                        "requires": ["shell"],
                        "privilege_tier": "privileged_local",
                        "assignment_ttl_seconds": 900,
                    }
                ),
                "attempt_count": 0,
                "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            }
        ]
        self.assigned: list[tuple[str, str, str, str, str, int, dict | None]] = []
        self.queued: list[tuple[str, dict | None]] = []
        self.expired: list[tuple[str, dict | None]] = []

    def get_routable_proposals(self) -> list[dict]:
        return [copy.deepcopy(row) for row in self.proposals]

    def get_eligible_nodes(self, required_capabilities: list[str], privilege_tier: str) -> list[dict]:
        eligible: list[dict] = []
        for node in self.nodes:
            caps = json.loads(node["capabilities_json"])
            meta = json.loads(node["meta_json"])
            if all(cap in caps for cap in required_capabilities) and meta.get("privilege_tier") == privilege_tier:
                eligible.append(copy.deepcopy(node))
        return eligible

    def assign_proposal_to_node(
        self,
        proposal_id: str,
        node_id: str,
        assignment_expires_at: str,
        *,
        proposal_hash: str = "",
        hub_signature: str = "",
        attempt_count: int = 1,
        eligibility_snapshot: dict | None = None,
    ) -> bool:
        self.assigned.append(
            (
                proposal_id,
                node_id,
                assignment_expires_at,
                proposal_hash,
                hub_signature,
                attempt_count,
                copy.deepcopy(eligibility_snapshot),
            )
        )
        return True

    def queue_proposal(self, proposal_id: str, eligibility_snapshot: dict | None = None) -> bool:
        self.queued.append((proposal_id, copy.deepcopy(eligibility_snapshot)))
        return True

    def expire_proposal(self, proposal_id: str, eligibility_snapshot: dict | None = None) -> bool:
        self.expired.append((proposal_id, copy.deepcopy(eligibility_snapshot)))
        return True


def test_reactive_assignment_routes_to_eligible_node():
    db = FakeDB()
    result = dispatch_routable_proposals(db)

    assert result.get("routed") == 1
    assert not db.queued, f"expected no queued proposals, got {db.queued}"
    assert not db.expired, f"expected no expired proposals, got {db.expired}"
    assert len(db.assigned) == 1

    proposal_id, node_id, _, proposal_hash, hub_signature, attempt_count, snapshot = db.assigned[0]
    assert proposal_id == "prop-reactive-1"
    assert node_id == "macbook@heiwa-node-a"
    assert proposal_hash, "proposal hash should be populated"
    assert hub_signature.startswith("SIG-"), "hub signature should be populated"
    assert attempt_count == 1, "attempt count should increment to 1"
    assert snapshot and snapshot.get("eligible_count") == 1, "eligibility snapshot should have eligible_count=1"
