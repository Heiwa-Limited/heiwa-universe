from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List

@dataclass
class PodRecord:
    pod_id: str
    host_identity: str
    provider: str
    runtime_capabilities: List[str]
    trust_tier: str
    privacy_floor: str
    gpu_inventory: List[Dict[str, Any]]
    liveness: str
    leaseable: bool
    registered_at: str
    last_heartbeat: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "PodRecord":
        return cls(
            pod_id=str(data.get("pod_id", "")),
            host_identity=str(data.get("host_identity", "")),
            provider=str(data.get("provider", "local")),
            runtime_capabilities=list(data.get("runtime_capabilities", [])),
            trust_tier=str(data.get("trust_tier", "untrusted")),
            privacy_floor=str(data.get("privacy_floor", "public")),
            gpu_inventory=list(data.get("gpu_inventory", [])),
            liveness=str(data.get("liveness", "offline")),
            leaseable=bool(data.get("leaseable", False)),
            registered_at=str(data.get("registered_at", "")),
            last_heartbeat=str(data.get("last_heartbeat", "")),
        )

    def to_dict(self) -> dict[str, Any]:
        from dataclasses import asdict
        return asdict(self)
