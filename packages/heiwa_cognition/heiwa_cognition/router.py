from __future__ import annotations

import logging
from dataclasses import dataclass
import json
from pathlib import Path

from heiwa_protocol.routing import normalize_privacy_level

logger = logging.getLogger("Cognition.Router")


@dataclass(slots=True)
class ComputeRoute:
    compute_class: int
    assigned_worker: str
    target_tool: str
    target_model: str
    target_runtime: str
    target_tier: str
    privacy_level: str
    rationale: str
    intent_class: str = ""


class ComputeRouter:
    """
    Heiwa routing core.

    Converts the normalized intent/risk/privacy profile into a control-plane
    route that the broker, spine, executor, and MCP surface can all share.

    Rate-group-aware: when a premium route's provider is throttled, cascades
    to the next available provider in the rotation list from ai_router.json.
    """

    def __init__(self, router_path: Path | None = None) -> None:
        root = Path(__file__).resolve().parents[3]
        self.router_path = router_path or root / "config" / "swarm" / "ai_router.json"
        self.router_config = self._load_router()
        self.registry = self.router_config.get("models", {}).get("registry", {})
        self._rotation = (
            self.router_config
            .get("routing_policy", {})
            .get("provider_rotation", {})
            .get("premium_remote", [])
        )
        self._intent_rotations = (
            self.router_config
            .get("routing_policy", {})
            .get("provider_rotation", {})
            .get("by_intent", {})
        )
        self._providers = self.router_config.get("providers", {})

    def _load_router(self) -> dict:
        try:
            return json.loads(self.router_path.read_text(encoding="utf-8"))
        except Exception:
            return {}

    def _rate_group_for_worker(self, worker: str) -> str:
        """Resolve worker → provider → rate_group."""
        entry = self.registry.get(worker, {})
        provider = str(entry.get("provider") or "")
        provider_cfg = self._providers.get(provider, {})
        return str(provider_cfg.get("rate_group", provider))

    def _model_for_worker(self, worker: str) -> str:
        return str(self.registry.get(worker, {}).get("id") or "")

    def _worker_for_model_id(self, model_id: str) -> str | None:
        """Find the registry worker key for a given model id."""
        for worker_key, entry in self.registry.items():
            if entry.get("id") == model_id:
                return worker_key
        return None

    def _rotation_for_intent(self, intent_class: str) -> list[str]:
        intent = str(intent_class or "").strip().lower()
        candidates = self._intent_rotations.get(intent)
        if isinstance(candidates, list) and candidates:
            return [str(item) for item in candidates if str(item).strip()]
        return [str(item) for item in self._rotation if str(item).strip()]

    def _maybe_cascade(self, route: ComputeRoute) -> ComputeRoute:
        """If the chosen route's rate group is throttled, cascade to the next
        available provider in the premium_remote rotation."""
        rotation = self._rotation_for_intent(route.intent_class)
        if route.compute_class < 3 or not rotation:
            return route
        if route.target_tool == "heiwa_ops":
            return route

        try:
            from heiwa_sdk.rate_ledger import get_rate_ledger
            ledger = get_rate_ledger(self.router_path)
        except Exception:
            return route

        current_group = self._rate_group_for_worker(route.assigned_worker)
        if ledger.has_capacity(current_group):
            return route

        # Walk the rotation list and find the first group with capacity
        for model_id in rotation:
            worker_key = self._worker_for_model_id(model_id)
            if not worker_key or worker_key == route.assigned_worker:
                continue
            alt_group = self._rate_group_for_worker(worker_key)
            if alt_group == current_group:
                continue
            if ledger.has_capacity(alt_group):
                logger.info(
                    "Rate cascade: %s (%s) exhausted → %s (%s)",
                    route.assigned_worker, current_group, worker_key, alt_group,
                )
                return ComputeRoute(
                    compute_class=route.compute_class,
                    assigned_worker=worker_key,
                    target_tool=route.target_tool,
                    target_model=self._model_for_worker(worker_key),
                    target_runtime=route.target_runtime,
                    target_tier=route.target_tier,
                    privacy_level=route.privacy_level,
                    rationale=f"Rate cascade from {current_group} (exhausted) to {alt_group}.",
                    intent_class=route.intent_class,
                )

        logger.warning(
            "All premium rate groups exhausted for task. Proceeding with original: %s",
            current_group,
        )
        return route

    def _local_worker_for_intent(self, intent_class: str) -> str:
        # Use single_node_mac lane mappings if available
        snm = (
            self.router_config
            .get("routing_policy", {})
            .get("single_node_mac", {})
            .get("local_lanes", {})
        )
        lane = snm.get(intent_class, {})
        if lane.get("worker"):
            return lane["worker"]
        if intent_class in {"build", "files", "self_buff"}:
            return "node_a_codegen"
        return "node_a_orchestrator"

    def _local_model_for_intent(self, intent_class: str) -> str | None:
        """Return the preferred local model for an intent from single_node_mac config."""
        snm = (
            self.router_config
            .get("routing_policy", {})
            .get("single_node_mac", {})
            .get("local_lanes", {})
        )
        lane = snm.get(intent_class, {})
        return lane.get("model") or None

    def _route_from_worker(
        self,
        *,
        intent: str,
        compute_class: int,
        worker: str,
        runtime: str,
        tier: str,
        privacy_level: str,
        rationale: str,
        direct_tool: str | None = None,
    ) -> ComputeRoute:
        model_id = self._model_for_worker(worker)
        # For local routes, prefer the single_node_mac lane model if configured
        if compute_class <= 2:
            lane_model = self._local_model_for_intent(intent)
            if lane_model:
                model_id = lane_model
        target_tool = direct_tool or "heiwa_claw"
        return ComputeRoute(
            compute_class=compute_class,
            assigned_worker=worker,
            target_tool=target_tool,
            target_model=model_id,
            target_runtime=runtime,
            target_tier=tier,
            privacy_level=privacy_level,
            rationale=rationale,
            intent_class=intent,
        )

    def route(
        self,
        intent_class: str,
        risk_level: str,
        raw_text: str = "",
        privacy_level: str | None = None,
        identity_worker_hint: str | None = None,
    ) -> ComputeRoute:
        result = self._route_inner(intent_class, risk_level, raw_text, privacy_level)
        # Apply identity worker preference for premium (class 3+) routes
        if identity_worker_hint and result.compute_class >= 3 and result.target_tool != "heiwa_ops":
            hint_model = self._model_for_worker(identity_worker_hint)
            if hint_model:
                result = ComputeRoute(
                    compute_class=result.compute_class,
                    assigned_worker=identity_worker_hint,
                    target_tool=result.target_tool,
                    target_model=hint_model,
                    target_runtime=result.target_runtime,
                    target_tier=result.target_tier,
                    privacy_level=result.privacy_level,
                    rationale=f"Identity-preferred worker: {identity_worker_hint}.",
                    intent_class=result.intent_class,
                )

        # Apply feedback-based model preference within the same provider lane
        result = self._apply_feedback_preference(result)

        return self._maybe_cascade(result)

    def _apply_feedback_preference(self, route: ComputeRoute) -> ComputeRoute:
        """Adjust model based on accumulated /feedback signals.

        Only reorders within the same provider — never overrides mode,
        privacy, or provider eligibility.
        """
        if route.target_tool == "heiwa_ops":
            return route
        try:
            from heiwa_sdk.memory import preferred_model_for
            preferred = preferred_model_for(route.intent_class)
        except Exception:
            return route
        if not preferred:
            return route

        # Only apply if the preferred model is from the same provider
        current_provider = route.target_model.split("/", 1)[0] if "/" in route.target_model else ""
        preferred_provider = preferred.split("/", 1)[0] if "/" in preferred else ""
        if current_provider and preferred_provider and current_provider != preferred_provider:
            return route

        if preferred != route.target_model:
            logger.info(
                "Feedback preference: %s → %s for intent %s",
                route.target_model, preferred, route.intent_class,
            )
            return ComputeRoute(
                compute_class=route.compute_class,
                assigned_worker=route.assigned_worker,
                target_tool=route.target_tool,
                target_model=preferred,
                target_runtime=route.target_runtime,
                target_tier=route.target_tier,
                privacy_level=route.privacy_level,
                rationale=f"Feedback-preferred model: {preferred}.",
                intent_class=route.intent_class,
            )
        return route

    def _route_inner(
        self,
        intent_class: str,
        risk_level: str,
        raw_text: str = "",
        privacy_level: str | None = None,
    ) -> ComputeRoute:
        privacy = normalize_privacy_level(privacy_level, raw_text)
        intent = str(intent_class or "general").strip().lower() or "general"
        risk = str(risk_level or "low").strip().lower() or "low"

        # Sovereign clamp: everything remains local and off-cloud.
        if privacy == "sovereign":
            worker = self._local_worker_for_intent(intent)
            if intent in {"audit", "status_check", "chat", "files"}:
                compute_class = 1
                direct_tool = "heiwa_ops" if intent != "chat" else "heiwa_claw"
                tier = "tier1_local"
            else:
                compute_class = 2
                direct_tool = "heiwa_ops" if intent in {"deploy", "operate", "automate", "automation", "mesh_ops"} else None
                tier = "tier5_heavy_code" if intent in {"build", "self_buff"} else "tier3_orchestrator"
            return self._route_from_worker(
                intent=intent,
                compute_class=compute_class,
                worker=worker,
                runtime="macbook",
                tier=tier,
                privacy_level=privacy,
                rationale="Sovereign clamp keeps execution on local trusted infrastructure.",
                direct_tool=direct_tool,
            )

        if intent in {"audit", "status_check"}:
            return self._route_from_worker(
                intent=intent,
                compute_class=1,
                worker="node_a_orchestrator",
                runtime="both" if intent == "audit" else "railway",
                tier="tier1_local",
                privacy_level=privacy,
                rationale="Deterministic audit/status work stays on the fast local ops path.",
                direct_tool="heiwa_ops",
            )

        if intent == "files":
            return self._route_from_worker(
                intent=intent,
                compute_class=1,
                worker="node_a_codegen",
                runtime="boost",
                tier="tier1_local",
                privacy_level=privacy,
                rationale="Filesystem mutations prefer boost node (local files); Railway uses git clone.",
                direct_tool="heiwa_ops",
            )

        if intent in {"deploy", "operate", "automate", "automation"}:
            return ComputeRoute(
                compute_class=4,
                assigned_worker="railway_control_plane",
                target_tool="heiwa_ops",
                target_model="",
                target_runtime="railway",
                target_tier="tier3_orchestrator",
                privacy_level=privacy,
                rationale="Operational control-plane work stays on Railway via bounded deterministic tools.",
                intent_class=intent,
            )

        if intent == "build":
            if risk in {"high", "critical"}:
                return self._route_from_worker(
                    intent=intent,
                    compute_class=3,
                    worker="class_3_build",
                    runtime="railway",
                    tier="tier5_heavy_code",
                    privacy_level=privacy,
                    rationale="High-risk build escalates to Railway-hosted premium CLI session.",
                )
            return self._route_from_worker(
                intent=intent,
                compute_class=2,
                worker="node_a_codegen",
                runtime="railway",
                tier="tier5_heavy_code",
                privacy_level=privacy,
                rationale="Build work executes on Railway; boost node accelerates when available.",
            )

        if intent == "media":
            return self._route_from_worker(
                intent=intent,
                compute_class=2,
                worker="node_b_media",
                runtime="boost",
                tier="tier2_fast_context",
                privacy_level=privacy,
                rationale="Media work requires GPU — queued for boost node (MacBook/WSL).",
            )

        if intent == "research":
            return self._route_from_worker(
                intent=intent,
                compute_class=3,
                worker="class_3_research",
                runtime="railway",
                tier="tier6_premium_context",
                privacy_level=privacy,
                rationale="Research executes on Railway via premium long-context CLI session.",
            )

        if intent == "strategy":
            return self._route_from_worker(
                intent=intent,
                compute_class=3,
                worker="class_3_strategy",
                runtime="railway",
                tier="tier7_supreme_court",
                privacy_level=privacy,
                rationale="Strategy executes on Railway via adversarial-review CLI session.",
            )

        if intent in {"mesh_ops", "self_buff"}:
            return self._route_from_worker(
                intent=intent,
                compute_class=2,
                worker="node_a_orchestrator",
                runtime="railway",
                tier="tier3_orchestrator",
                privacy_level=privacy,
                rationale="Heiwa maintenance runs on Railway; boost node assists when online.",
                direct_tool="heiwa_ops" if intent == "mesh_ops" else None,
            )

        if intent == "chat":
            return self._route_from_worker(
                intent=intent,
                compute_class=1,
                worker="node_a_orchestrator",
                runtime="railway",
                tier="tier1_local",
                privacy_level=privacy,
                rationale="Chat handled by Railway via free-tier API inference.",
            )

        return self._route_from_worker(
            intent=intent,
            compute_class=2,
            worker="node_a_orchestrator",
            runtime="railway",
            tier="tier3_orchestrator",
            privacy_level=privacy,
            rationale="Default route executes on Railway primary plane.",
        )
