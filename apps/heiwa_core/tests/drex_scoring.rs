use heiwa_bindings::ModelTier;
use heiwa_core::drex::{
    default_policy, evaluate_drex, plan_route, preflight_execution, DrexIngress, DrexVector,
    ExecutionMode, ResolutionTier,
};

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

fn remote_model(model_id: &str, capability_class: u8, strengths: &[&str]) -> ModelTier {
    ModelTier {
        id: 0,
        model_id: model_id.to_string(),
        provider_model_id: model_id.to_string(),
        provider: "claude".to_string(),
        rate_group: "anthropic".to_string(),
        capability_class,
        effort_knob: "default".to_string(),
        effort_level: capability_class,
        cost_per_turn: 0.05,
        max_context_tokens: 128_000,
        vram_requirement_mb: 0,
        quantization_type: "none".to_string(),
        kv_cache_strategy: "standard".to_string(),
        strengths_json: serde_json::to_string(strengths).unwrap(),
        enabled: true,
        last_success_rate: 0.99,
        avg_latency_ms: 800,
        latency_p_95_ms: 1600,
        updated_at: "2026-05-08T00:00:00Z".to_string(),
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
    assert_eq!(route.execution_mode, ExecutionMode::LocalModel);
    assert_eq!(route.runtime_hint, "local");
    assert_eq!(selected.model_id, "ollama/qwen3.5:4b");
    assert_eq!(selected.kv_cache_strategy, "turboquant");
}

#[test]
fn tool_using_route_requires_tool_capable_model() {
    let ingress = DrexIngress {
        intent: "operate".to_string(),
        risk: "low".to_string(),
        raw_text: "use the GitHub MCP server to open an issue from this summary".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 8_192,
        required_context_tokens: 4_096,
    };

    let tiers = vec![
        remote_model("claude-high-no-tools", 5, &["chat", "research"]),
        remote_model("claude-tool-capable", 3, &["chat", "tool_use"]),
    ];

    let route = plan_route(&ingress, &tiers, &default_policy()).expect("route plan");
    let selected = route.selected_model.expect("selected model");

    assert_eq!(selected.model_id, "claude-tool-capable");
    assert!(route.routing_metadata.contains("tool_use"));
}

#[test]
fn api_design_prompt_does_not_require_tool_capability() {
    let ingress = DrexIngress {
        intent: "strategy".to_string(),
        risk: "low".to_string(),
        raw_text: "design the public API architecture for the cockpit".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 8_192,
        required_context_tokens: 4_096,
    };

    let tiers = vec![remote_model("claude-no-tools", 4, &["chat", "research"])];

    let route = plan_route(&ingress, &tiers, &default_policy()).expect("route plan");
    let selected = route.selected_model.expect("selected model");

    assert_eq!(selected.model_id, "claude-no-tools");
    assert!(route
        .routing_metadata
        .contains("\"required_capabilities\":[]"));
}

#[test]
fn preflight_handles_greeting_without_model_spend() {
    let ingress = DrexIngress {
        intent: "chat".to_string(),
        risk: "low".to_string(),
        raw_text: "hi".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 8_192,
        required_context_tokens: 256,
    };

    let result = preflight_execution(&ingress, &[], &default_policy());
    assert_eq!(result.execution_mode, ExecutionMode::Deterministic);
    assert!(
        result
            .response_text
            .as_deref()
            .unwrap_or_default()
            .contains("Ready"),
        "expected deterministic greeting response, got {:?}",
        result.response_text
    );
}

#[test]
fn preflight_asks_for_clarification_on_underspecified_prompt() {
    let ingress = DrexIngress {
        intent: "chat".to_string(),
        risk: "low".to_string(),
        raw_text: "help".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 8_192,
        required_context_tokens: 256,
    };

    let result = preflight_execution(&ingress, &[], &default_policy());
    assert_eq!(result.execution_mode, ExecutionMode::Clarify);
    assert!(
        result
            .response_text
            .as_deref()
            .unwrap_or_default()
            .contains("Tell me"),
        "expected clarify response, got {:?}",
        result.response_text
    );
}
