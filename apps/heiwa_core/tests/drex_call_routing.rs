use heiwa_core::drex::{
    default_policy, plan_model_call, plan_route, CandidateRejection, CostTruth, DrexIngress,
    ModelCallCandidate, ModelCallRequest,
};
use heiwa_protocol::ModelTier;

fn request() -> ModelCallRequest {
    ModelCallRequest {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        intent: "code".to_string(),
        stage: "execution".to_string(),
        raw_text: "edit the routing implementation".to_string(),
        privacy: "standard".to_string(),
        required_capabilities: vec!["advanced_coding".to_string()],
        required_context_tokens: 8_192,
        minimum_quality_class: 3,
        minimum_success_rate: 0.90,
        maximum_marginal_cost_usd: None,
        preferred_provider: None,
        preferred_model: None,
        allowed_models: Vec::new(),
        excluded_models: Vec::new(),
    }
}

fn candidate(
    id: u64,
    model_id: &str,
    provider: &str,
    quality: u8,
    cost: Option<f64>,
    cost_truth: CostTruth,
) -> ModelCallCandidate {
    ModelCallCandidate {
        tier: ModelTier {
            id,
            model_id: model_id.to_string(),
            provider_model_id: model_id.to_string(),
            provider: provider.to_string(),
            rate_group: provider.to_string(),
            capability_class: quality,
            effort_knob: "default".to_string(),
            effort_level: quality,
            cost_per_turn: cost.unwrap_or(0.0),
            max_context_tokens: 128_000,
            vram_requirement_mb: 0,
            quantization_type: "none".to_string(),
            kv_cache_strategy: "standard".to_string(),
            strengths_json: "[\"advanced_coding\",\"tool_use\"]".to_string(),
            enabled: true,
            last_success_rate: 0.98,
            avg_latency_ms: 500,
            latency_p_95_ms: 750,
            updated_at: "2026-07-19T00:00:00Z".to_string(),
        },
        connected: true,
        adapter_capable: true,
        quota_available: true,
        marginal_cost_usd: cost,
        cost_truth,
    }
}

fn rejection(plan: &heiwa_core::drex::ModelCallPlan, id: u64) -> &CandidateRejection {
    plan.rejected
        .iter()
        .find(|rejection| rejection.candidate_id == id)
        .unwrap_or_else(|| panic!("missing rejection for {id}"))
}

#[test]
fn minimum_quality_admits_subscription_route_then_uses_marginal_cost() {
    let free_low_quality = candidate(
        1,
        "local-free",
        "ollama",
        1,
        Some(0.0),
        CostTruth::LocalZeroCost,
    );
    let subscription = candidate(
        2,
        "codex-subscription",
        "openai",
        3,
        Some(0.0),
        CostTruth::TargetOnly,
    );
    let direct_api = candidate(
        3,
        "direct-api",
        "openai",
        3,
        Some(0.08),
        CostTruth::ExactProviderReport,
    );

    let plan = plan_model_call(
        &request(),
        &[free_low_quality, subscription.clone(), direct_api],
        &default_policy(),
    )
    .expect("call plan");

    assert_eq!(plan.selected.as_ref(), Some(&subscription));
    assert_eq!(plan.selected_cost_truth, Some(CostTruth::TargetOnly));
    assert_eq!(rejection(&plan, 1).reasons, vec!["minimum_quality_class"]);
    assert_eq!(plan.admitted_ids, vec![2, 3]);
}

#[test]
fn hard_gates_reject_before_cost_and_pins_do_not_bypass_them() {
    let mut request = request();
    request.privacy = "sovereign".to_string();
    request.required_context_tokens = 200_000;
    request.required_capabilities = vec!["tool_use".to_string()];
    request.preferred_provider = Some("remote".to_string());
    request.preferred_model = Some("pinned-remote".to_string());

    let pinned_remote = candidate(
        1,
        "pinned-remote",
        "remote",
        3,
        Some(0.0),
        CostTruth::TargetOnly,
    );
    let mut disconnected = candidate(
        2,
        "disconnected",
        "ollama",
        3,
        Some(0.0),
        CostTruth::LocalZeroCost,
    );
    disconnected.connected = false;
    let mut quota_exhausted =
        candidate(3, "quota", "ollama", 3, Some(0.0), CostTruth::LocalZeroCost);
    quota_exhausted.quota_available = false;
    let mut context_short = candidate(
        4,
        "context",
        "ollama",
        3,
        Some(0.0),
        CostTruth::LocalZeroCost,
    );
    context_short.tier.max_context_tokens = 100;
    let mut missing_capability = candidate(
        5,
        "capability",
        "ollama",
        3,
        Some(0.0),
        CostTruth::LocalZeroCost,
    );
    missing_capability.tier.strengths_json = "[\"advanced_coding\"]".to_string();

    let plan = plan_model_call(
        &request,
        &[
            pinned_remote,
            disconnected,
            quota_exhausted,
            context_short,
            missing_capability,
        ],
        &default_policy(),
    )
    .expect_err("all candidates fail hard gates");

    assert!(plan
        .to_string()
        .contains("no admitted model call candidates"));
}

#[test]
fn explicit_allow_and_exclude_and_budget_are_hard_gates() {
    let mut request = request();
    request.allowed_models = vec!["allowed".to_string()];
    request.excluded_models = vec!["excluded".to_string()];
    request.maximum_marginal_cost_usd = Some(0.01);
    let allowed = candidate(
        1,
        "allowed",
        "provider",
        3,
        Some(0.01),
        CostTruth::ExactProviderReport,
    );
    let excluded = candidate(
        2,
        "excluded",
        "provider",
        3,
        Some(0.0),
        CostTruth::ExactProviderReport,
    );
    let unlisted = candidate(
        3,
        "unlisted",
        "provider",
        3,
        Some(0.0),
        CostTruth::ExactProviderReport,
    );
    let expensive = candidate(
        4,
        "allowed",
        "expensive",
        3,
        Some(0.02),
        CostTruth::ExactProviderReport,
    );

    let plan = plan_model_call(
        &request,
        &[allowed.clone(), excluded, unlisted, expensive],
        &default_policy(),
    )
    .expect("allowed candidate remains");
    assert_eq!(plan.selected, Some(allowed));
    assert_eq!(rejection(&plan, 2).reasons, vec!["excluded_model"]);
    assert_eq!(rejection(&plan, 3).reasons, vec!["not_allowed_model"]);
    assert_eq!(
        rejection(&plan, 4).reasons,
        vec!["maximum_marginal_cost_usd"]
    );
}

#[test]
fn invalid_metrics_are_rejected_deterministically() {
    let mut request = request();
    request.minimum_success_rate = 0.95;
    let valid = candidate(
        1,
        "valid",
        "provider",
        3,
        Some(0.01),
        CostTruth::ExactProviderReport,
    );
    let nan_cost = candidate(
        2,
        "nan-cost",
        "provider",
        3,
        Some(f64::NAN),
        CostTruth::ProxyEstimate,
    );
    let negative_cost = candidate(
        3,
        "negative-cost",
        "provider",
        3,
        Some(-0.01),
        CostTruth::ProxyEstimate,
    );
    let infinite_cost = candidate(
        4,
        "infinite-cost",
        "provider",
        3,
        Some(f64::INFINITY),
        CostTruth::ProxyEstimate,
    );
    let mut nan_success = candidate(
        5,
        "nan-success",
        "provider",
        3,
        Some(0.0),
        CostTruth::ProxyEstimate,
    );
    nan_success.tier.last_success_rate = f64::NAN;
    let mut negative_success = candidate(
        6,
        "negative-success",
        "provider",
        3,
        Some(0.0),
        CostTruth::ProxyEstimate,
    );
    negative_success.tier.last_success_rate = -0.1;

    let plan = plan_model_call(
        &request,
        &[
            valid.clone(),
            nan_cost,
            negative_cost,
            infinite_cost,
            nan_success,
            negative_success,
        ],
        &default_policy(),
    )
    .expect("valid candidate selected");

    assert_eq!(plan.selected, Some(valid));
    for id in 2..=4 {
        assert_eq!(
            rejection(&plan, id).reasons,
            vec!["invalid_marginal_cost_usd"]
        );
    }
    for id in 5..=6 {
        assert_eq!(rejection(&plan, id).reasons, vec!["invalid_success_rate"]);
    }
}

#[test]
fn legacy_route_wrapper_delegates_cost_selection_to_per_call_planner() {
    let mut local = candidate(
        1,
        "expensive-local",
        "ollama",
        3,
        Some(0.0),
        CostTruth::LocalZeroCost,
    )
    .tier;
    local.vram_requirement_mb = 32_768;
    local.last_success_rate = 0.90;
    let remote = candidate(
        2,
        "cheap-remote",
        "remote",
        3,
        Some(0.08),
        CostTruth::ExactProviderReport,
    )
    .tier;
    let ingress = DrexIngress {
        intent: "research".to_string(),
        risk: "low".to_string(),
        raw_text: "research routing doctrine".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        available_vram_mb: 32_768,
        required_context_tokens: 8_192,
    };

    let route = plan_route(&ingress, &[local, remote], &default_policy()).expect("route plan");

    assert_eq!(
        route.selected_model.expect("selected").model_id,
        "expensive-local"
    );
    assert!(route
        .routing_metadata
        .contains("lowest_known_marginal_cost"));
}
