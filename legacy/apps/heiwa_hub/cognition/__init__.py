"""Cognition modules — re-exported from heiwa_cognition shared package."""

from heiwa_cognition.llm import (
    LocalLLMEngine,
    LLMPolicyError,
    llm_generate,
    llm_generate_async,
    llm_generate_json,
    llm_is_available,
    llm_generate_with_plan,
)
from heiwa_cognition.router import ComputeRouter, ComputeRoute, InferenceTarget, RoutedPlan, InferenceResult
from heiwa_cognition.planner import LocalTaskPlanner, TaskPlan, StepPlan
from heiwa_cognition.approval import ApprovalRegistry, ApprovalState
from heiwa_cognition.intent import IntentNormalizer, IntentProfile
from heiwa_cognition.risk import RiskScorer, RiskAssessment
from heiwa_cognition.enrichment import BrokerEnrichmentService
from heiwa_cognition.identity import IdentitySelector, Identity, Cell

__all__ = [
    "LocalLLMEngine", "LLMPolicyError",
    "llm_generate", "llm_generate_async", "llm_generate_json", "llm_is_available", "llm_generate_with_plan",
    "ComputeRouter", "ComputeRoute", "InferenceTarget", "RoutedPlan", "InferenceResult",
    "LocalTaskPlanner", "TaskPlan", "StepPlan",
    "ApprovalRegistry", "ApprovalState",
    "IntentNormalizer", "IntentProfile",
    "RiskScorer", "RiskAssessment",
    "BrokerEnrichmentService",
    "IdentitySelector", "Identity", "Cell",
]
