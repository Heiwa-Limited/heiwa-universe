use std::cmp::Ordering;

use anyhow::{anyhow, Result};
use heiwa_protocol::ModelTier;

use super::policy::{DrexDecision, DrexPolicy, DEFAULT_POLICY_VERSION};
use super::router::{build_drex_vector, is_local_provider, tier_has_strength};
use super::scorer::evaluate_drex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostTruth {
    LocalZeroCost,
    TargetOnly,
    ProxyEstimate,
    ExactProviderReport,
    CannotConfirm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub intent: String,
    pub stage: String,
    pub raw_text: String,
    pub privacy: String,
    pub required_capabilities: Vec<String>,
    pub required_context_tokens: u32,
    pub minimum_quality_class: u8,
    pub minimum_success_rate: f64,
    pub maximum_marginal_cost_usd: Option<f64>,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub allowed_models: Vec<String>,
    pub excluded_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallCandidate {
    pub tier: ModelTier,
    pub connected: bool,
    pub adapter_capable: bool,
    pub quota_available: bool,
    pub marginal_cost_usd: Option<f64>,
    pub cost_truth: CostTruth,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CandidateRejection {
    pub candidate_id: u64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallPlan {
    pub selected: Option<ModelCallCandidate>,
    pub selected_id: Option<u64>,
    pub selected_cost_truth: Option<CostTruth>,
    pub admitted_ids: Vec<u64>,
    pub rejected: Vec<CandidateRejection>,
    pub policy_version: String,
    pub selection_reason: String,
    pub decision: DrexDecision,
}

pub fn plan_model_call(
    request: &ModelCallRequest,
    candidates: &[ModelCallCandidate],
    policy: &DrexPolicy,
) -> Result<ModelCallPlan> {
    validate_request(request)?;

    let vector = build_drex_vector(
        &request.intent,
        "standard",
        &request.raw_text,
        &request.privacy,
        if request.privacy == "sovereign" {
            "local"
        } else {
            "any"
        },
    );
    let decision = evaluate_drex(&vector, policy, 0.95, 1.0, 0.65);
    let mut admitted = Vec::new();
    let mut rejected = Vec::new();

    for candidate in candidates {
        if let Some(reason) = rejection_reason(request, candidate) {
            rejected.push(CandidateRejection {
                candidate_id: candidate.tier.id,
                reasons: vec![reason.to_string()],
            });
        } else {
            admitted.push(candidate.clone());
        }
    }

    admitted.sort_by(compare_candidates);
    let admitted_ids = admitted.iter().map(|candidate| candidate.tier.id).collect();
    let selected = admitted.into_iter().next();
    let Some(selected) = selected else {
        return Err(anyhow!("no admitted model call candidates"));
    };

    Ok(ModelCallPlan {
        selected_id: Some(selected.tier.id),
        selected_cost_truth: Some(selected.cost_truth.clone()),
        selected: Some(selected),
        admitted_ids,
        rejected,
        policy_version: DEFAULT_POLICY_VERSION.to_string(),
        selection_reason: "lowest_known_marginal_cost_then_quality_latency_success".to_string(),
        decision,
    })
}

pub fn compare_candidates(left: &ModelCallCandidate, right: &ModelCallCandidate) -> Ordering {
    match (left.marginal_cost_usd, right.marginal_cost_usd) {
        (Some(left_cost), Some(right_cost)) => {
            let ordering = left_cost.total_cmp(&right_cost);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        (None, None) => {}
    }

    let quality = right.tier.capability_class.cmp(&left.tier.capability_class);
    if quality != Ordering::Equal {
        return quality;
    }

    let latency = left.tier.latency_p_95_ms.cmp(&right.tier.latency_p_95_ms);
    if latency != Ordering::Equal {
        return latency;
    }

    let success = right
        .tier
        .last_success_rate
        .total_cmp(&left.tier.last_success_rate);
    if success != Ordering::Equal {
        return success;
    }

    left.tier.id.cmp(&right.tier.id)
}

fn validate_request(request: &ModelCallRequest) -> Result<()> {
    if !valid_probability(request.minimum_success_rate) {
        return Err(anyhow!(
            "minimum_success_rate must be finite and within 0.0..=1.0"
        ));
    }
    if let Some(cost) = request.maximum_marginal_cost_usd {
        if !cost.is_finite() || cost < 0.0 {
            return Err(anyhow!(
                "maximum_marginal_cost_usd must be finite and non-negative"
            ));
        }
    }
    Ok(())
}

fn rejection_reason<'a>(
    request: &'a ModelCallRequest,
    candidate: &'a ModelCallCandidate,
) -> Option<&'static str> {
    if !candidate.tier.enabled {
        return Some("disabled_model");
    }
    if !valid_cost(candidate.marginal_cost_usd) {
        return Some("invalid_marginal_cost_usd");
    }
    if !valid_probability(candidate.tier.last_success_rate) {
        return Some("invalid_success_rate");
    }
    if !candidate.connected {
        return Some("disconnected");
    }
    if !candidate.adapter_capable {
        return Some("adapter_incapable");
    }
    if !candidate.quota_available {
        return Some("quota_exhausted");
    }
    if model_matches_any(candidate, &request.excluded_models) {
        return Some("excluded_model");
    }
    if !request.allowed_models.is_empty() && !model_matches_any(candidate, &request.allowed_models)
    {
        return Some("not_allowed_model");
    }
    if request
        .preferred_provider
        .as_deref()
        .is_some_and(|provider| provider != candidate.tier.provider)
    {
        return Some("preferred_provider_mismatch");
    }
    if request
        .preferred_model
        .as_deref()
        .is_some_and(|model| !model_matches(candidate, model))
    {
        return Some("preferred_model_mismatch");
    }
    if request.privacy == "sovereign" && !is_local_provider(&candidate.tier.provider) {
        return Some("sovereign_remote");
    }
    if candidate.tier.max_context_tokens < request.required_context_tokens {
        return Some("insufficient_context");
    }
    if !request
        .required_capabilities
        .iter()
        .all(|capability| tier_has_strength(&candidate.tier, capability))
    {
        return Some("missing_required_capability");
    }
    if candidate.tier.capability_class < request.minimum_quality_class {
        return Some("minimum_quality_class");
    }
    if candidate.tier.last_success_rate < request.minimum_success_rate {
        return Some("minimum_success_rate");
    }
    if let Some(maximum) = request.maximum_marginal_cost_usd {
        match candidate.marginal_cost_usd {
            Some(cost) if cost > maximum => return Some("maximum_marginal_cost_usd"),
            None => return Some("unknown_marginal_cost_usd"),
            _ => {}
        }
    }
    None
}

fn model_matches_any(candidate: &ModelCallCandidate, models: &[String]) -> bool {
    models.iter().any(|model| model_matches(candidate, model))
}

fn model_matches(candidate: &ModelCallCandidate, model: &str) -> bool {
    model == candidate.tier.model_id
        || model == candidate.tier.provider_model_id
        || model == format!("{}/{}", candidate.tier.provider, candidate.tier.model_id)
}

fn valid_cost(cost: Option<f64>) -> bool {
    cost.is_none_or(|cost| cost.is_finite() && cost >= 0.0)
}

fn valid_probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
