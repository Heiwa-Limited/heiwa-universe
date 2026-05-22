use anyhow::{anyhow, Result};
use heiwa_bindings::ModelTier;

use super::policy::{DrexDecision, DrexPolicy, ExecutionMode, ResolutionTier};
use super::scorer::evaluate_drex;
use super::vector::DrexVector;

use serde::Deserialize;
use serde_json::json;

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
    pub execution_mode: ExecutionMode,
    pub runtime_hint: String,
    pub selected_model: Option<ModelTier>,
    pub routing_metadata: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreflightDecision {
    pub execution_mode: ExecutionMode,
    pub response_text: Option<String>,
    pub reason: String,
}

pub fn preflight_execution(
    ingress: &DrexIngress,
    model_tiers: &[ModelTier],
    policy: &DrexPolicy,
) -> PreflightDecision {
    let trimmed = ingress.raw_text.trim();
    if trimmed.is_empty() {
        return PreflightDecision {
            execution_mode: ExecutionMode::Clarify,
            response_text: Some(
                "Tell me the outcome you want, or use /status, /providers, or /models.".to_string(),
            ),
            reason: "empty_input".to_string(),
        };
    }

    let lowercase = trimmed.to_ascii_lowercase();
    if is_greeting(&lowercase) {
        return PreflightDecision {
            execution_mode: ExecutionMode::Deterministic,
            response_text: Some(
                "Ready. Tell me what you want to do, or use /status, /providers, or /models."
                    .to_string(),
            ),
            reason: "greeting".to_string(),
        };
    }

    if is_underspecified(&lowercase) {
        return PreflightDecision {
            execution_mode: ExecutionMode::Clarify,
            response_text: Some(
                "Tell me the outcome you want, the repo or file target, or the command context."
                    .to_string(),
            ),
            reason: "underspecified".to_string(),
        };
    }

    let vector = build_drex_vector(
        &ingress.intent,
        &ingress.risk,
        &ingress.raw_text,
        &ingress.privacy,
        &ingress.runtime,
    );
    let runtime_hint = runtime_hint(ingress, &vector);
    let runtime_fit = if is_local_runtime(&runtime_hint) {
        1.0
    } else {
        0.8
    };
    let decision = evaluate_drex(&vector, policy, 0.95, runtime_fit, 0.65);
    let local_candidates: Vec<&ModelTier> = model_tiers
        .iter()
        .filter(|tier| tier.enabled && is_local_provider(&tier.provider))
        .collect();
    let remote_candidates: Vec<&ModelTier> = model_tiers
        .iter()
        .filter(|tier| tier.enabled && !is_local_provider(&tier.provider))
        .collect();

    let execution_mode = if should_prefer_local(ingress, &decision, &local_candidates)
        || remote_candidates.is_empty()
    {
        ExecutionMode::LocalModel
    } else {
        ExecutionMode::RemoteModel
    };

    PreflightDecision {
        execution_mode,
        response_text: None,
        reason: match execution_mode {
            ExecutionMode::LocalModel => "local_first".to_string(),
            ExecutionMode::RemoteModel => "remote_escalation".to_string(),
            ExecutionMode::Deterministic => "deterministic".to_string(),
            ExecutionMode::Clarify => "clarify".to_string(),
        },
    }
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
    let initial_runtime_hint = runtime_hint(ingress, &vector);
    let runtime_fit = if is_local_runtime(&initial_runtime_hint) {
        1.0
    } else {
        0.8
    };
    let decision = evaluate_drex(&vector, policy, 0.95, runtime_fit, 0.65);
    let selected_model = select_model_tier(ingress, &initial_runtime_hint, &decision, model_tiers);

    let required_capabilities = required_model_capabilities(ingress);
    let (execution_mode, runtime_hint, routing_metadata) = if let Some(ref tier) = selected_model {
        let execution_mode = execution_mode_for_tier(tier);
        let runtime_hint = if matches!(execution_mode, ExecutionMode::LocalModel) {
            "local".to_string()
        } else {
            initial_runtime_hint.clone()
        };
        (
            execution_mode,
            runtime_hint,
            json!({
                "reason": "best_score",
                "mode": execution_mode_label(execution_mode),
                "model_id": tier.model_id,
                "provider": tier.provider,
                "required_capabilities": required_capabilities,
            })
            .to_string(),
        )
    } else {
        return Err(anyhow!("no compatible model tier found for route"));
    };

    Ok(RoutePlan {
        decision,
        execution_mode,
        runtime_hint,
        selected_model,
        routing_metadata,
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

    if [
        "portfolio",
        "enterprise",
        "roadmap",
        "priority",
        "governance",
    ]
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

    "cloud".to_string()
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
    let required_capabilities = required_model_capabilities(ingress);
    let local_only = ingress.privacy == "sovereign" || is_local_runtime(runtime_hint);
    let compatible: Vec<&ModelTier> = model_tiers
        .iter()
        .filter(|tier| tier.enabled)
        .filter(|tier| tier.capability_class >= min_capability_class)
        .filter(|tier| {
            tier.max_context_tokens >= ingress.required_context_tokens
                || ingress.required_context_tokens == 0
        })
        .filter(|tier| tier.vram_requirement_mb <= ingress.available_vram_mb || !local_only)
        .filter(|tier| !local_only || is_local_provider(&tier.provider))
        .filter(|tier| {
            required_capabilities
                .iter()
                .all(|capability| tier_has_strength(tier, capability))
        })
        .filter(|tier| {
            if ingress.intent == "code" {
                tier_has_strength(tier, "advanced_coding") || tier.capability_class >= 3
            } else {
                true
            }
        })
        .collect();

    let local_candidates: Vec<&ModelTier> = compatible
        .iter()
        .copied()
        .filter(|tier| is_local_provider(&tier.provider))
        .collect();

    if local_only {
        return best_tier(&local_candidates, ingress, decision);
    }

    if should_prefer_local(ingress, decision, &local_candidates) {
        if let Some(best_local) = best_tier(&local_candidates, ingress, decision) {
            return Some(best_local);
        }
    }

    best_tier(&compatible, ingress, decision)
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
    score += (tier.capability_class as f64) * 2.0;

    // Strength bonuses
    if ingress.intent == "code" && tier_has_strength(tier, "advanced_coding") {
        score += 2.0;
    }

    score - (tier.cost_per_turn * 0.1) - ((tier.vram_requirement_mb as f64) / 32_768.0)
}

fn required_model_capabilities(ingress: &DrexIngress) -> Vec<&'static str> {
    let lowercase = ingress.raw_text.to_ascii_lowercase();
    let phrase_needles = [
        "mcp server",
        "mcp tool",
        "tool use",
        "use github",
        "github",
        "pull request",
        "open an issue",
        "create an issue",
        "notion",
        "slack",
        "gmail",
        "send email",
        "calendar",
        "google drive",
        "google sheet",
        "browser",
        "webhook",
        "jira",
        "linear",
    ];
    let word_needles = ["mcp"];
    let needs_external_tool = phrase_needles
        .iter()
        .any(|needle| lowercase.contains(needle))
        || word_needles
            .iter()
            .any(|needle| contains_word(&lowercase, needle));

    if needs_external_tool {
        vec!["tool_use"]
    } else {
        Vec::new()
    }
}

fn tier_has_strength(tier: &ModelTier, capability: &str) -> bool {
    serde_json::from_str::<Vec<String>>(&tier.strengths_json)
        .map(|strengths| strengths.iter().any(|strength| strength == capability))
        .unwrap_or(false)
}

fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|token| token == word)
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "local" | "vllm" | "litellm")
}

fn is_local_runtime(runtime: &str) -> bool {
    matches!(runtime, "macbook" | "boost" | "local")
}

fn best_tier(
    candidates: &[&ModelTier],
    ingress: &DrexIngress,
    decision: &DrexDecision,
) -> Option<ModelTier> {
    candidates
        .iter()
        .max_by(|left, right| {
            model_score(left, ingress, decision).total_cmp(&model_score(right, ingress, decision))
        })
        .map(|tier| (*tier).clone())
}

fn should_prefer_local(
    ingress: &DrexIngress,
    decision: &DrexDecision,
    local_candidates: &[&ModelTier],
) -> bool {
    if local_candidates.is_empty() {
        return false;
    }

    if matches!(ingress.intent.as_str(), "chat" | "status_check") {
        return true;
    }

    if ingress.privacy == "sovereign" {
        return true;
    }

    if ingress.intent == "code" {
        let strongest_local = local_candidates
            .iter()
            .map(|tier| tier.capability_class)
            .max()
            .unwrap_or(0);
        return strongest_local >= 3
            && ingress.required_context_tokens <= 32_768
            && !matches!(ingress.risk.as_str(), "high" | "critical");
    }

    decision.active_tier == ResolutionTier::Micro
        && ingress.required_context_tokens <= 32_768
        && !matches!(ingress.risk.as_str(), "high" | "critical")
}

fn execution_mode_for_tier(tier: &ModelTier) -> ExecutionMode {
    if is_local_provider(&tier.provider) {
        ExecutionMode::LocalModel
    } else {
        ExecutionMode::RemoteModel
    }
}

fn execution_mode_label(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Deterministic => "deterministic",
        ExecutionMode::LocalModel => "local_model",
        ExecutionMode::RemoteModel => "remote_model",
        ExecutionMode::Clarify => "clarify",
    }
}

fn clamp(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn is_greeting(lowercase: &str) -> bool {
    matches!(
        lowercase.trim(),
        "hi" | "hello"
            | "hey"
            | "yo"
            | "sup"
            | "gm"
            | "good morning"
            | "good afternoon"
            | "good evening"
    )
}

fn is_underspecified(lowercase: &str) -> bool {
    let trimmed = lowercase.trim();
    let token_count = trimmed.split_whitespace().count();
    token_count <= 2
        && matches!(
            trimmed,
            "help" | "what" | "why" | "huh" | "okay" | "ok" | "sure" | "go" | "start" | "continue"
        )
}
