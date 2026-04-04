use heiwa_core::drex::{DrexIngress, plan_route, default_policy};
use heiwa_bindings::ModelTier;

fn get_mock_model_tiers() -> Vec<ModelTier> {
    vec![
        ModelTier {
            id: 1,
            model_id: "llama3".to_string(),
            provider_model_id: "llama3:latest".to_string(),
            provider: "ollama".to_string(),
            rate_group: "local_ollama".to_string(),
            capability_class: 1,
            effort_knob: "default".to_string(),
            effort_level: 1,
            cost_per_turn: 0.0,
            max_context_tokens: 8192,
            strengths_json: r#"["chat", "advanced_coding"]"#.to_string(),
            vram_requirement_mb: 4096,
            quantization_type: "q4_K_M".to_string(),
            kv_cache_strategy: "standard".to_string(),
            enabled: true,
            last_success_rate: 1.0,
            avg_latency_ms: 100,
            latency_p_95_ms: 150,
            updated_at: "2026-04-04T05:00:00Z".to_string(),
        },
        ModelTier {
            id: 2,
            model_id: "claude-3-5-sonnet".to_string(),
            provider_model_id: "claude-3-5-sonnet-20240620".to_string(),
            provider: "anthropic".to_string(),
            rate_group: "cloud_anthropic".to_string(),
            capability_class: 3,
            effort_knob: "high".to_string(),
            effort_level: 5,
            cost_per_turn: 0.01,
            max_context_tokens: 200000,
            strengths_json: r#"["chat", "advanced_coding"]"#.to_string(),
            vram_requirement_mb: 0,
            quantization_type: "fp16".to_string(),
            kv_cache_strategy: "remote".to_string(),
            enabled: true,
            last_success_rate: 1.0,
            avg_latency_ms: 500,
            latency_p_95_ms: 800,
            updated_at: "2026-04-04T05:00:00Z".to_string(),
        },
    ]
}

#[test]
fn test_drex_routing_allows_cloud_for_standard_privacy() {
    let ingress = DrexIngress {
        intent: "code".to_string(),
        risk: "low".to_string(),
        raw_text: "write a python script".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 8192,
        required_context_tokens: 1024,
    };
    
    let model_tiers = get_mock_model_tiers();
    let policy = default_policy();
    let plan = plan_route(&ingress, &model_tiers, &policy).expect("should plan route");
    
    let selected = plan.selected_model.expect("should select a model");
    assert_eq!(selected.model_id, "claude-3-5-sonnet");
}

#[test]
fn test_drex_routing_forces_local_for_sovereign_privacy() {
    let ingress = DrexIngress {
        intent: "code".to_string(),
        risk: "low".to_string(),
        raw_text: "secret code".to_string(),
        privacy: "sovereign".to_string(),
        runtime: "local".to_string(),
        available_vram_mb: 8192,
        required_context_tokens: 1024,
    };
    
    let model_tiers = get_mock_model_tiers();
    let policy = default_policy();
    let plan = plan_route(&ingress, &model_tiers, &policy).expect("should plan route");
    
    let selected = plan.selected_model.expect("should select a model");
    assert_eq!(selected.model_id, "llama3");
    assert_eq!(selected.provider, "ollama");
}
