use anyhow::{anyhow, Result};
use heiwa_bindings::ModelTier;

use super::policy::{DrexDecision, DrexPolicy, ResolutionTier};
use super::scorer::evaluate_drex;
use super::vector::DrexVector;

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DrexIngress {
    pub intent: String,
    pub risk: String,
    pub raw_text: String,
    pub privacy: String,
    pub runtime: String,
    pub available_vram_mb: u32,
    pub required_context_tokens: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutePlan {
    pub decision: DrexDecision,
    pub runtime_hint: String,
    pub selected_model: Option<ModelTier>,
}

pub fn plan_route(
    ingress: &DrexIngress,
    model_tiers: &[ModelTier],
    policy: &DrexPolicy,
) -> Result<RoutePlan> {
    let vector = build_drex_vector(
        &ingress.intent,
        &ingress.risk,
        &ingress.raw_text,
        &ingress.privacy,
        &ingress.runtime,
    );
    let runtime_hint = runtime_hint(ingress, &vector);
    let runtime_fit = if is_local_runtime(&runtime_hint) { 1.0 } else { 0.8 };
    let decision = evaluate_drex(&vector, policy, 0.95, runtime_fit, 0.65);
    let selected_model = select_model_tier(ingress, &runtime_hint, &decision, model_tiers);

    if selected_model.is_none() {
        return Err(anyhow!("no compatible model tier found for route"));
    }

    Ok(RoutePlan {
        decision,
        runtime_hint,
        selected_model,
    })
}

fn build_drex_vector(
    intent: &str,
    risk: &str,
    raw_text: &str,
    privacy: &str,
    runtime: &str,
) -> DrexVector {
    let mut vector = match intent {
        "build" | "files" => DrexVector {
            scope: 0.35,
            abstraction: 0.35,
            context_span: 0.55,
            execution_proximity: 0.90,
            blast_radius: 0.55,
            coordination_load: 0.30,
            latency_pressure: 0.75,
        },
        "audit" | "status_check" => DrexVector {
            scope: 0.45,
            abstraction: 0.40,
            context_span: 0.60,
            execution_proximity: 0.30,
            blast_radius: 0.45,
            coordination_load: 0.25,
            latency_pressure: 0.60,
        },
        "research" => DrexVector {
            scope: 0.70,
            abstraction: 0.80,
            context_span: 0.85,
            execution_proximity: 0.20,
            blast_radius: 0.40,
            coordination_load: 0.65,
            latency_pressure: 0.30,
        },
        "strategy" => DrexVector {
            scope: 0.90,
            abstraction: 0.95,
            context_span: 0.80,
            execution_proximity: 0.10,
            blast_radius: 0.75,
            coordination_load: 0.85,
            latency_pressure: 0.20,
        },
        "deploy" | "operate" | "automate" => DrexVector {
            scope: 0.75,
            abstraction: 0.60,
            context_span: 0.65,
            execution_proximity: 0.65,
            blast_radius: 0.90,
            coordination_load: 0.70,
            latency_pressure: 0.70,
        },
        _ => DrexVector {
            scope: 0.50,
            abstraction: 0.50,
            context_span: 0.50,
            execution_proximity: 0.50,
            blast_radius: 0.50,
            coordination_load: 0.50,
            latency_pressure: 0.50,
        },
    };

    if matches!(risk, "high" | "critical") {
        vector.scope = clamp(vector.scope + 0.15);
        vector.blast_radius = clamp(vector.blast_radius + 0.15);
    }

    if privacy == "sovereign" {
        vector.scope = clamp(vector.scope - 0.05);
        vector.execution_proximity = clamp(vector.execution_proximity + 0.10);
    }

    if matches!(runtime, "boost" | "macbook" | "local") {
        vector.execution_proximity = clamp(vector.execution_proximity + 0.10);
    }

    if raw_text.len() > 500 {
        vector.context_span = clamp(vector.context_span + 0.10);
    }

    let lowercase = raw_text.to_ascii_lowercase();
    if ["patch", "edit", "write", "run", "pytest", "bash", "shell"]
        .iter()
        .any(|needle| lowercase.contains(needle))
    {
        vector.execution_proximity = clamp(vector.execution_proximity + 0.10);
    }

    if ["portfolio", "enterprise", "roadmap", "priority", "governance"]
        .iter()
        .any(|needle| lowercase.contains(needle))
    {
        vector.scope = clamp(vector.scope + 0.10);
        vector.abstraction = clamp(vector.abstraction + 0.10);
    }

    vector
}

fn runtime_hint(ingress: &DrexIngress, vector: &DrexVector) -> String {
    if ingress.privacy == "sovereign" && is_local_runtime(&ingress.runtime) {
        return ingress.runtime.clone();
    }

    if vector.execution_proximity >= 0.80 && is_local_runtime(&ingress.runtime) {
        return ingress.runtime.clone();
    }

    "railway".to_string()
}

fn select_model_tier(
    ingress: &DrexIngress,
    runtime_hint: &str,
    decision: &DrexDecision,
    model_tiers: &[ModelTier],
) -> Option<ModelTier> {
    let min_capability_class = match ingress.risk.as_str() {
        "critical" | "high" => 3,
        "medium" => 2,
        _ => 1,
    };
    let local_only = ingress.privacy == "sovereign" || is_local_runtime(runtime_hint);

    model_tiers
        .iter()
        .filter(|tier| tier.enabled)
        .filter(|tier| tier.capability_class >= min_capability_class)
        .filter(|tier| {
            tier.max_context_tokens >= ingress.required_context_tokens
                || ingress.required_context_tokens == 0
        })
        .filter(|tier| tier.vram_requirement_mb <= ingress.available_vram_mb || !local_only)
        .filter(|tier| !local_only || is_local_provider(&tier.provider))
        .max_by(|left, right| {
            model_score(left, ingress, decision)
                .total_cmp(&model_score(right, ingress, decision))
        })
        .cloned()
}

fn model_score(tier: &ModelTier, ingress: &DrexIngress, decision: &DrexDecision) -> f64 {
    let mut score = tier.last_success_rate;

    if tier.vram_requirement_mb <= ingress.available_vram_mb {
        score += 1.5;
    }
    if tier.max_context_tokens >= ingress.required_context_tokens {
        score += 1.5;
    }
    if tier.kv_cache_strategy == "turboquant" && ingress.required_context_tokens >= 16_384 {
        score += 0.75;
    }
    if tier.cost_per_turn == 0.0 {
        score += 0.5;
    }
    if tier.capability_class == 2 && decision.active_tier == ResolutionTier::Micro {
        score += 0.25;
    }

    score - (tier.cost_per_turn * 0.1) - ((tier.vram_requirement_mb as f64) / 32_768.0)
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "local" | "vllm" | "litellm")
}

fn is_local_runtime(runtime: &str) -> bool {
    matches!(runtime, "macbook" | "boost" | "local")
}

fn clamp(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}
