pub mod call;
pub mod policy;
pub mod router;
pub mod scorer;
pub mod vector;

pub use call::{
    compare_candidates, plan_model_call, CallRisk, CandidateRejection, CandidateRejectionReason,
    CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallIdentity, ModelCallPlan,
    ModelCallRequest, ModelCallStage, PrivacyClass, SafetyClass,
};
pub use policy::{
    default_policy, DrexAuthorityGate, DrexDecision, DrexPolicy, DrexScoreCard, ExecutionMode,
    ResolutionTier, DEFAULT_POLICY_VERSION,
};
pub use router::{
    plan_route, plan_route_for_call, preflight_execution, DrexIngress, PreflightDecision, RoutePlan,
};
pub use scorer::evaluate_drex;
pub use vector::DrexVector;
