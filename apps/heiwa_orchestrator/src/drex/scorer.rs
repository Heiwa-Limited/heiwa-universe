use super::policy::{
    DrexAuthorityGate, DrexDecision, DrexPolicy, DrexScoreCard, ResolutionTier,
};
use super::vector::DrexVector;

pub fn evaluate_drex(
    vector: &DrexVector,
    policy: &DrexPolicy,
    observability: f64,
    runtime_fit: f64,
    history_confidence: f64,
) -> DrexDecision {
    let macro_score = dot(policy.macro_weights, vector.axes())
        + policy.macro_bias
        + (history_confidence * policy.history_confidence_multiplier);
    let meso_score = dot(policy.meso_weights, vector.axes())
        + policy.meso_bias
        + (((runtime_fit + history_confidence) / 2.0) * (policy.history_confidence_multiplier / 2.0));
    let micro_score = dot(policy.micro_weights, vector.axes())
        + policy.micro_bias
        + (runtime_fit * policy.runtime_fit_multiplier)
        + (history_confidence * (policy.history_confidence_multiplier / 2.0))
        - ((1.0 - observability) * policy.observability_micro_multiplier);

    let gate = authority_gate(vector);
    let (active_tier, confidence) = choose_tier(
        macro_score,
        meso_score,
        micro_score,
        gate.requires_approval,
        policy.tie_margin,
    );

    DrexDecision {
        vector: vector.clone(),
        scorecard: DrexScoreCard {
            macro_score,
            meso_score,
            micro_score,
            confidence,
        },
        active_tier,
        gate,
    }
}

fn dot(weights: [f64; 7], axes: [f64; 7]) -> f64 {
    weights
        .into_iter()
        .zip(axes)
        .map(|(weight, axis)| weight * axis)
        .sum()
}

fn authority_gate(vector: &DrexVector) -> DrexAuthorityGate {
    if vector.blast_radius >= 0.75 {
        DrexAuthorityGate {
            authority_required: "approved_write".to_string(),
            requires_approval: true,
            reasons: vec!["blast_radius".to_string()],
        }
    } else if vector.scope >= 0.80 && vector.coordination_load >= 0.70 {
        DrexAuthorityGate {
            authority_required: "operator_review".to_string(),
            requires_approval: true,
            reasons: vec!["enterprise_scope".to_string(), "coordination_load".to_string()],
        }
    } else {
        DrexAuthorityGate {
            authority_required: "none".to_string(),
            requires_approval: false,
            reasons: Vec::new(),
        }
    }
}

fn choose_tier(
    macro_score: f64,
    meso_score: f64,
    micro_score: f64,
    approval_required: bool,
    tie_margin: f64,
) -> (ResolutionTier, f64) {
    let mut ranked = [
        (ResolutionTier::Macro, macro_score),
        (ResolutionTier::Meso, meso_score),
        (ResolutionTier::Micro, micro_score),
    ];
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

    let confidence = ranked[0].1 - ranked[1].1;
    if confidence < tie_margin {
        if approval_required && macro_score >= meso_score {
            return (ResolutionTier::Macro, confidence);
        }
        return (ResolutionTier::Meso, confidence);
    }

    (ranked[0].0, confidence)
}
