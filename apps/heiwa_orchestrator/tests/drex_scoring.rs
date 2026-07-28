use heiwa_orchestrator::drex::{
    default_policy, evaluate_drex, plan_route, DrexIngress, DrexVector, ResolutionTier,
};
use heiwa_protocol::ModelTier;

fn local_model(
    model_id: &str,
    capability_class: u8,
    max_context_tokens: u32,
    vram_requirement_mb: u32,
    quantization_type: &str,
    kv_cache_strategy: &str,
) -> ModelTier {
    ModelTier {
        id: 0,
        model_id: model_id.to_string(),
        provider_model_id: model_id.split('/').nth(1).unwrap_or(model_id).to_string(),
        provider: "ollama".to_string(),
        rate_group: "local_ollama".to_string(),
        capability_class,
        effort_knob: "thinking:on".to_string(),
        effort_level: capability_class,
        cost_per_turn: 0.0,
        max_context_tokens,
        vram_requirement_mb,
        quantization_type: quantization_type.to_string(),
        kv_cache_strategy: kv_cache_strategy.to_string(),
        strengths_json: "[\"build\",\"research\",\"general\"]".to_string(),
        enabled: true,
        last_success_rate: 0.99,
        avg_latency_ms: 700,
        latency_p_95_ms: 1400,
        updated_at: "2026-04-01T00:00:00Z".to_string(),
    }
}

#[test]
fn code_edit_task_scores_micro_highest() {
    let vector = DrexVector {
        scope: 0.30,
        abstraction: 0.20,
        context_span: 0.55,
        execution_proximity: 0.95,
        blast_radius: 0.45,
        coordination_load: 0.25,
        latency_pressure: 0.80,
    };

    let result = evaluate_drex(&vector, &default_policy(), 1.0, 1.0, 0.6);
    assert_eq!(result.active_tier, ResolutionTier::Micro);
}

#[test]
fn strategic_task_scores_macro_highest() {
    let vector = DrexVector {
        scope: 0.95,
        abstraction: 0.95,
        context_span: 0.80,
        execution_proximity: 0.10,
        blast_radius: 0.85,
        coordination_load: 0.90,
        latency_pressure: 0.20,
    };

    let result = evaluate_drex(&vector, &default_policy(), 0.9, 1.0, 0.7);
    assert_eq!(result.active_tier, ResolutionTier::Macro);
    assert!(result.gate.requires_approval);
}

#[test]
fn route_plan_prefers_vram_fit_and_kv_strategy_for_local_execution() {
    let ingress = DrexIngress {
        intent: "build".to_string(),
        risk: "medium".to_string(),
        raw_text: "edit two files, run pytest, and patch the failing route".to_string(),
        privacy: "sovereign".to_string(),
        runtime: "macbook".to_string(),
        available_vram_mb: 6_144,
        required_context_tokens: 32_768,
    };

    let tiers = vec![
        local_model(
            "ollama/qwen3.5:4b",
            2,
            32_768,
            4_096,
            "q4_k_m",
            "turboquant",
        ),
        local_model("ollama/qwen3.5:14b", 3, 32_768, 12_288, "q4_k_m", "q8_0"),
    ];

    let route = plan_route(&ingress, &tiers, &default_policy()).expect("route plan");
    let selected = route.selected_model.expect("selected model");

    assert_eq!(route.decision.active_tier, ResolutionTier::Micro);
    assert_eq!(route.runtime_hint, "macbook");
    assert_eq!(selected.model_id, "ollama/qwen3.5:4b");
    assert_eq!(selected.kv_cache_strategy, "turboquant");
}
