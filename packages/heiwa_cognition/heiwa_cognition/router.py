from __future__ import annotations

import logging
from dataclasses import dataclass
import json
import os
from pathlib import Path
from typing import Any

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
    effort_knob: str = ""  # provider-specific effort setting
    requires_human_oversight: bool = False


@dataclass(slots=True)
class InferenceTarget:
    model_id: str
    provider_model_id: str
    provider: str
    rate_group: str
    transport: str
    effort_knob: str
    capability_class: int


@dataclass(slots=True)
class RoutedPlan:
    primary: InferenceTarget
    fallbacks: list[InferenceTarget]
    intent: str
    risk: str
    privacy: str
    reason: str
    retry_policy: str


@dataclass(slots=True)
class InferenceResult:
    text: str
    provider: str
    model: str
    attempts: int
    rerouted: bool


class ComputeRouter:
    """
    Heiwa routing core.

    Converts the normalized intent/risk/privacy profile into a control-plane
    route that the broker, spine, executor, and MCP surface can all share.

    Rate-group-aware: when a premium route's provider is throttled, cascades
    to the next available provider in the rotation list from ai_router.json.
    """

    def __init__(self, router_path: Path | None = None, stdb: Any | None = None) -> None:
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

        # STDB model tier support (falls back to JSON config when None)
        self._stdb = stdb
        self._model_tiers: list[dict] | None = None
        if stdb:
            try:
                self._model_tiers = stdb.get_model_tiers()
                logger.info("ComputeRouter: loaded %d model tiers from STDB", len(self._model_tiers))
            except Exception as e:
                logger.warning("ComputeRouter: STDB unavailable (%s), falling back to JSON", e)

    def _is_local_provider(self, provider: str) -> bool:
        return provider in {"ollama", "local", "vllm", "litellm"}

    def _get_available_providers(self, owner_id: str) -> set[str]:
        # Always allow local providers
        available = {"ollama", "local", "vllm", "litellm"}

        # Check UserVault if STDB is available
        if self._stdb:
            try:
                # STDB client has get_provider_credentials(user_id)
                creds = self._stdb.get_provider_credentials(owner_id)
                for c in creds:
                    available.add(c["provider_id"])
            except Exception:
                pass

        # Operator fallback (Railway/Local system keys)
        if owner_id == "operator":
            if os.getenv("GEMINI_API_KEY"): available.add("google")
            if os.getenv("ANTHROPIC_API_KEY"): available.add("anthropic")
            if os.getenv("OPENAI_API_KEY"): available.add("openai")

        return available

    def _select_model_from_tiers(
        self,
        intent_class: str,
        risk_level: str,
        *,
        owner_id: str = "operator",
        target_runtime: str,
        privacy_level: str,
        min_capability_override: int | None = None,
    ) -> tuple[str, str] | None:
        """Select cheapest capable model from STDB tiers. Returns (model_id, effort_knob) or None."""
        candidates = self._ranked_tier_candidates(
            intent_class,
            risk_level,
            owner_id=owner_id,
            target_runtime=target_runtime,
            privacy_level=privacy_level,
            min_capability_override=min_capability_override,
        )
        if not candidates:
            return None

        best = candidates[0]
        return best["model_id"], best["effort_knob"]

    def _ranked_tier_candidates(
        self,
        intent_class: str,
        risk_level: str,
        *,
        owner_id: str = "operator",
        target_runtime: str,
        privacy_level: str,
        min_capability_override: int | None = None,
    ) -> list[dict[str, Any]]:
        if self._model_tiers is None:
            try:
                db = self._stdb
                if not db:
                    import os
                    from heiwa_sdk.spacetimedb import SpacetimeDB
                    identity = os.environ.get("STDB_IDENTITY", "heiwaproductiondb")
                    server = os.environ.get("STDB_SERVER", "local")
                    db = SpacetimeDB(db_identity=identity, server=server)
                self._model_tiers = db.get_model_tiers()
            except Exception:
                # Leave self._model_tiers as None to allow transient retry attempts
                pass

        if not self._model_tiers:
            return []

        min_class = min_capability_override
        if min_class is None:
            min_class = {"low": 1, "medium": 2, "high": 3, "critical": 3}.get(risk_level, 2)

        runtime = str(target_runtime or "").strip().lower()
        privacy = str(privacy_level or "").strip().lower()

        try:
            db = self._stdb
            if not db:
                import os
                from heiwa_sdk.spacetimedb import SpacetimeDB
                identity = os.environ.get("STDB_IDENTITY", "heiwaproductiondb")
                server = os.environ.get("STDB_SERVER", "local")
                db = SpacetimeDB(db_identity=identity, server=server)
            slots = db.get_gpu_slots()
            pods = db.get_pods()
        except Exception:
            slots = []
            pods = []

        active_loaded_models = set()
        for s in slots:
            if s.get("available_slots", 0) > 0:
                models = s.get("loaded_models")
                if isinstance(models, list) and models:
                    active_loaded_models.add(models[0])
                elif isinstance(models, str) and models:
                    # Support legacy or CLI bridge-flattened formats
                    active_loaded_models.add(models)

        max_available_vram = max([s.get("vram_gb", 0) for s in slots if s.get("available_slots", 0) > 0], default=0)
        available_providers = self._get_available_providers(owner_id)

        candidates: list[tuple[dict[str, Any], bool, bool, bool, bool]] = []
        for tier in self._model_tiers:
            if tier["capability_class"] < min_class:
                continue
            if not tier.get("enabled", True):
                continue

            provider = str(tier.get("provider") or "")
            if provider not in available_providers:
                continue

            is_local_provider = self._is_local_provider(provider)
            
            # Pod trust_tier alignment mappings:
            # - sovereign requires local or trust_tier = local
            # - sensitive requires trust_tier in [trusted, local] 
            if privacy == "sovereign":
                 if not is_local_provider:
                     continue
            elif privacy == "sensitive" and runtime not in {"boost", "macbook"}:
                 # Stricter privacy bounds when not routing locally natively
                 if not is_local_provider:
                      continue

            if runtime == "railway" and is_local_provider:
                continue

            strengths_json = tier.get("strengths_json") or "[]"
            if isinstance(strengths_json, str):
                try:
                    strengths = json.loads(strengths_json)
                except json.JSONDecodeError:
                    strengths = []
            else:
                strengths = strengths_json

            exact_intent_match = intent_class in strengths
            intent_match = exact_intent_match or "general" in strengths
            
            # Capacity-aware constraint:
            # Provider "ollama" natively refers to local GPU capacity constraints. We extract exact models avoiding prefix splits if they align perfectly.
            model_key = tier["model_id"].split("/", 1)[1] if "/" in tier["model_id"] else tier["model_id"]
            is_hot = model_key in active_loaded_models
            has_capacity = is_hot or (max_available_vram > 0)
            
            # If it's a local model that requires VRAM but we have none, technically reject it unless we can't be sure
            if is_local_provider and not has_capacity and runtime != "railway":
                pass # Usually we still fallback but we penalize it severely in sort
                
            candidates.append((tier, exact_intent_match, intent_match, is_hot, has_capacity))

        candidates.sort(key=lambda c: (
            not c[3], # is_hot (Loaded already)
            not c[1], # exact intent match
            not c[2], # general intent match
            not c[4], # has capacity 
            c[0].get("last_success_rate", 1.0) < 0.5,
            -c[0].get("last_success_rate", 1.0),
            c[0]["cost_per_turn"],
            c[0]["effort_level"],
        ))
        return [item[0] for item in candidates]

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
                    effort_knob=route.effort_knob,
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

    def _cloud_lane_for_intent(self, intent_class: str) -> dict | None:
        """Return cloud lane config for an intent from single_node_mac, if any."""
        return (
            self.router_config
            .get("routing_policy", {})
            .get("single_node_mac", {})
            .get("cloud_lanes", {})
            .get(intent_class)
        )

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

        # Guard: Railway runtime must never get a local-only model.
        # Escalate to the cloud lane or a known Railway-safe provider.
        if runtime == "railway" and model_id:
            provider_prefix = model_id.split("/", 1)[0] if "/" in model_id else ""
            if self._is_local_provider(provider_prefix):
                cloud_lane = self._cloud_lane_for_intent(intent)
                if cloud_lane and cloud_lane.get("worker"):
                    worker = cloud_lane["worker"]
                    model_id = self._model_for_worker(worker)
                    compute_class = max(compute_class, 3)
                    logger.info(
                        "Railway guard: escalated local model to cloud lane %s (%s)",
                        worker, model_id,
                    )
                else:
                    # Fallback: use Gemini Flash via LLMEngine (heiwa_claw handles this)
                    model_id = "gemini/gemini-2.5-flash"
                    logger.info(
                        "Railway guard: replaced local model with Gemini Flash fallback",
                    )

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
        owner_id: str = "operator",
        identity_worker_hint: str | None = None,
    ) -> ComputeRoute:
        result = self._route_inner(intent_class, risk_level, raw_text, privacy_level)

        # Policy Gate: Scan for REQUIRES_HUMAN_OVERSIGHT in raw_text or metadata
        if "REQUIRES_HUMAN_OVERSIGHT" in raw_text:
            result.requires_human_oversight = True

        # STDB-first model selection stays advisory, but it must respect the
        # already-chosen runtime lane. Otherwise Railway can select boost-only
        # local models and create invalid dispatches.
        if result.target_tool != "heiwa_ops":
            # For Class 3 routes, we MUST preserve Class 3 reasoning during fallback
            min_cap = 3 if result.compute_class >= 3 else None
            
            tier_result = self._select_model_from_tiers(
                result.intent_class or intent_class,
                risk_level,
                owner_id=owner_id,
                target_runtime=result.target_runtime,
                privacy_level=result.privacy_level,
                min_capability_override=min_cap,
            )
            if tier_result:
                result.target_model = tier_result[0]
                result.effort_knob = tier_result[1]

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
                    effort_knob=result.effort_knob,
                )

        # Apply feedback-based model preference within the same provider lane
        result = self._apply_feedback_preference(result)

        return self._maybe_cascade(result)

    def route_inference(
        self,
        intent: str,
        risk: str,
        privacy: str | None = None,
        runtime: str | None = None,
        owner_id: str = "operator",
    ) -> RoutedPlan:
        route = self.route(
            intent_class=intent,
            risk_level=risk,
            raw_text="",
            privacy_level=privacy,
            owner_id=owner_id,
        )
        runtime_value = str(runtime or route.target_runtime or "auto").strip().lower() or "auto"
        privacy_value = str(route.privacy_level or privacy or "").strip().lower()

        try:
            from heiwa_sdk.provider_registry import ProviderRegistry
            registry = ProviderRegistry()
        except Exception:
            registry = None

        def _make_target(model_id: str, effort_knob: str, capability_class: int) -> InferenceTarget:
            provider = (
                registry.provider_for_model(model_id)
                if registry is not None
                else (model_id.split("/", 1)[0] if "/" in model_id else model_id)
            )
            provider_cfg = registry.resolve(provider) if registry is not None else None
            provider_model_id = model_id.split("/", 1)[1] if "/" in model_id else model_id
            return InferenceTarget(
                model_id=model_id,
                provider_model_id=provider_model_id,
                provider=provider,
                rate_group=(provider_cfg.rate_group if provider_cfg else provider),
                transport=(provider_cfg.transport if provider_cfg else ("local_http" if self._is_local_provider(provider) else "api_http")),
                effort_knob=effort_knob,
                capability_class=capability_class,
            )

        capability_class = route.compute_class
        primary = _make_target(route.target_model, route.effort_knob, capability_class)

        fallbacks: list[InferenceTarget] = []
        for tier in self._ranked_tier_candidates(
            intent,
            risk,
            owner_id=owner_id,
            target_runtime=runtime_value,
            privacy_level=privacy_value,
            min_capability_override=capability_class,
        ):
            if tier["model_id"] == primary.model_id:
                continue
            fallback_provider = (
                registry.provider_for_model(tier["model_id"])
                if registry is not None
                else (tier["model_id"].split("/", 1)[0] if "/" in tier["model_id"] else tier["model_id"])
            )
            fallback_cfg = registry.resolve(fallback_provider) if registry is not None else None
            fallbacks.append(
                InferenceTarget(
                    model_id=tier["model_id"],
                    provider_model_id=tier["model_id"].split("/", 1)[1] if "/" in tier["model_id"] else tier["model_id"],
                    provider=fallback_provider,
                    rate_group=(fallback_cfg.rate_group if fallback_cfg else fallback_provider),
                    transport=(fallback_cfg.transport if fallback_cfg else ("local_http" if self._is_local_provider(fallback_provider) else "api_http")),
                    effort_knob=str(tier.get("effort_knob") or ""),
                    capability_class=int(tier.get("capability_class") or capability_class),
                )
            )
            if len(fallbacks) >= 2:
                break

        retry_policy = "exhaust_then_reroute" if fallbacks else "reroute_immediately"
        return RoutedPlan(
            primary=primary,
            fallbacks=fallbacks,
            intent=intent,
            risk=risk,
            privacy=privacy_value or "unspecified",
            reason=route.rationale,
            retry_policy=retry_policy,
        )

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
                effort_knob=route.effort_knob,
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
                effort_knob="",
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
