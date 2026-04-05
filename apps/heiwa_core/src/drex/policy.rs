use super::vector::DrexVector;

pub const DEFAULT_POLICY_VERSION: &str = "2026-04-01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionTier {
    Macro,
    Meso,
    Micro,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    Deterministic,
    LocalModel,
    RemoteModel,
    Clarify,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrexPolicy {
    pub macro_weights: [f64; 7],
    pub meso_weights: [f64; 7],
    pub micro_weights: [f64; 7],
    pub macro_bias: f64,
    pub meso_bias: f64,
    pub micro_bias: f64,
    pub observability_micro_multiplier: f64,
    pub runtime_fit_multiplier: f64,
    pub history_confidence_multiplier: f64,
    pub tie_margin: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrexScoreCard {
    pub macro_score: f64,
    pub meso_score: f64,
    pub micro_score: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrexAuthorityGate {
    pub authority_required: String,
    pub requires_approval: bool,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrexDecision {
    pub vector: DrexVector,
    pub scorecard: DrexScoreCard,
    pub active_tier: ResolutionTier,
    pub gate: DrexAuthorityGate,
}

pub fn default_policy() -> DrexPolicy {
    DrexPolicy {
        macro_weights: [1.2, 1.1, 0.9, -0.9, 0.8, 0.7, -0.3],
        meso_weights: [0.6, 0.5, 0.7, 0.1, 0.6, 1.0, 0.2],
        micro_weights: [-0.4, -0.6, -0.3, 1.3, 0.4, -0.2, 1.1],
        macro_bias: 0.0,
        meso_bias: 0.05,
        micro_bias: 0.0,
        observability_micro_multiplier: 0.25,
        runtime_fit_multiplier: 0.20,
        history_confidence_multiplier: 0.15,
        tie_margin: 0.08,
    }
}
