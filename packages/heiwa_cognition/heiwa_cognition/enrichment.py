from __future__ import annotations

from pathlib import Path

from heiwa_cognition.identity import Identity, IdentitySelector
from heiwa_cognition.router import ComputeRouter
from heiwa_cognition.intent import IntentNormalizer
from heiwa_cognition.risk import RiskScorer
from heiwa_protocol.routing import BrokerRouteRequest, BrokerRouteResult


# Map identity tool preferences to registry worker keys.
# These are the primary workers each tool targets in ai_router.json.
_TOOL_TO_WORKER: dict[str, str] = {
    "codex": "class_3_build",
    "gemini_cli": "class_3_research",
    "antigravity": "class_3_strategy",
    "claude": "class_3_review",
}


def _identity_worker_hint(identity: Identity) -> str | None:
    """Extract a preferred worker key from an identity's tool assignment."""
    tool = str(identity.tool or "").strip().lower()
    return _TOOL_TO_WORKER.get(tool)


class BrokerEnrichmentService:
    """Typed broker enrichment pipeline shared by broker, spine, and MCP."""

    def __init__(self, identity_selector: IdentitySelector | None = None) -> None:
        self.normalizer = IntentNormalizer()
        self.scorer = RiskScorer()
        self.router = ComputeRouter()
        self._selector = identity_selector

    @property
    def identity_selector(self) -> IdentitySelector:
        if self._selector is None:
            self._selector = IdentitySelector()
        return self._selector

    def enrich(self, request: BrokerRouteRequest) -> BrokerRouteResult:
        profile = self.normalizer.normalize(request.raw_text)
        assessment = self.scorer.score(
            intent_class=profile.intent_class,
            raw_text=request.raw_text,
            source_surface=request.source_surface,
        )

        # Select identity and extract worker hint
        identity = self.identity_selector.select(profile.intent_class, request.raw_text)
        worker_hint = _identity_worker_hint(identity)

        route = self.router.route(
            intent_class=profile.intent_class,
            risk_level=assessment.risk_level,
            raw_text=request.raw_text,
            privacy_level=request.privacy_level,
            identity_worker_hint=worker_hint,
        )

        normalization = profile.to_dict()
        normalization["preferred_runtime"] = route.target_runtime
        normalization["preferred_tool"] = route.target_tool
        normalization["preferred_tier"] = route.target_tier
        normalization["risk_level"] = assessment.risk_level
        normalization["identity_id"] = identity.id

        return BrokerRouteResult(
            request_id=request.request_id,
            task_id=request.task_id,
            envelope_version=request.envelope_version,
            raw_text=request.raw_text,
            source_surface=request.source_surface,
            intent_class=profile.intent_class,
            risk_level=assessment.risk_level,
            privacy_level=route.privacy_level,
            compute_class=route.compute_class,
            assigned_worker=route.assigned_worker,
            target_tool=route.target_tool,
            target_model=route.target_model,
            target_runtime=route.target_runtime,
            target_tier=route.target_tier,
            requires_approval=assessment.requires_approval or profile.requires_approval,
            rationale=route.rationale,
            confidence=profile.confidence,
            escalation_reasons=assessment.escalation_reasons,
            assumptions=profile.assumptions,
            missing_details=profile.missing_details,
            normalization=normalization,
        )
