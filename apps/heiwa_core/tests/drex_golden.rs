//! E3 DREX golden eval suite — L2 routing layer.
//!
//! Data-driven goldens over [`plan_route`]. Fixtures live in
//! `tests/fixtures/drex_golden/l2/*.json`. The suite is hermetic: no providers,
//! network, STDB, or `heiwa-route`. The model registry is controlled test
//! infrastructure (see [`registry`]); only the *cases* are data.
//!
//! A failing fixture fails `cargo test` — that is E3's acceptance criterion
//! ("golden evals prove intent, risk, and route decisions and fail on
//! regressions").
//!
//! Layers:
//!   - L1 intent classification -> `crates/heiwa_protocol/tests/drex_golden.rs`
//!   - L2 routing decision       -> this file
//!   - L3 CLI smoke              -> `apps/heiwa_shell/tests/smoke.rs`

use std::fs;
use std::path::{Path, PathBuf};

use heiwa_core::drex::{default_policy, plan_route, DrexIngress};
use heiwa_protocol::ModelTier;
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenCase {
    id: String,
    #[allow(dead_code)]
    description: String,
    #[serde(default)]
    status: Status,
    registry: String,
    ingress: IngressSpec,
    expect: Expect,
}

#[derive(Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Status {
    #[default]
    Active,
    /// Documents intended-but-unbuilt behavior. Loaded and counted, never
    /// asserted, so the target is visible without faking implementation.
    Pending,
}

#[derive(Deserialize)]
struct IngressSpec {
    intent: String,
    risk: String,
    privacy: String,
    runtime: String,
    available_vram_mb: u32,
    required_context_tokens: u32,
    #[serde(default)]
    raw_text: String,
}

#[derive(Deserialize)]
struct Expect {
    /// "routed" | "error"
    outcome: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    rate_group: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
    /// "local_model" | "remote_model" | "deterministic"
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    provider_is_local: Option<bool>,
    #[serde(default)]
    min_capability_class: Option<u8>,
    #[serde(default)]
    error_contains: Option<String>,
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "local" | "vllm" | "litellm")
}

/// Controlled model registries. Named sets keep fixtures terse and the
/// environment deterministic across machines (no live `heiwa models` read).
fn registry(name: &str) -> Vec<ModelTier> {
    match name {
        "default" => vec![
            tier(
                "qwen3.5:9b",
                "ollama",
                "local",
                3,
                8192,
                16384,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gemma4:latest",
                "ollama",
                "local",
                2,
                4096,
                8192,
                r#"["chat"]"#,
            ),
            tier(
                "claude-sonnet-4-6",
                "anthropic",
                "anthropic",
                3,
                0,
                200000,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gpt-5-codex",
                "openai",
                "openai",
                3,
                0,
                200000,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gemini-3.1-pro",
                "google",
                "google",
                3,
                0,
                1000000,
                r#"["chat","advanced_coding"]"#,
            ),
        ],
        "cloud_only" => vec![
            tier(
                "claude-sonnet-4-6",
                "anthropic",
                "anthropic",
                3,
                0,
                200000,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gpt-5-codex",
                "openai",
                "openai",
                3,
                0,
                200000,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gemini-3.1-pro",
                "google",
                "google",
                3,
                0,
                1000000,
                r#"["chat","advanced_coding"]"#,
            ),
        ],
        "local_only" => vec![
            tier(
                "qwen3.5:9b",
                "ollama",
                "local",
                3,
                8192,
                16384,
                r#"["chat","advanced_coding"]"#,
            ),
            tier(
                "gemma4:latest",
                "ollama",
                "local",
                2,
                4096,
                8192,
                r#"["chat"]"#,
            ),
        ],
        // No local tier reaches capability_class 3.
        "local_weak_only" => vec![
            tier(
                "gemma4:latest",
                "ollama",
                "local",
                2,
                4096,
                8192,
                r#"["chat"]"#,
            ),
            tier(
                "qwen3.5:4b",
                "ollama",
                "local",
                1,
                2048,
                8192,
                r#"["chat"]"#,
            ),
        ],
        "empty" => vec![],
        other => panic!("unknown registry '{other}' referenced by fixture"),
    }
}

fn tier(
    model_id: &str,
    provider: &str,
    rate_group: &str,
    capability_class: u8,
    vram_requirement_mb: u32,
    max_context_tokens: u32,
    strengths_json: &str,
) -> ModelTier {
    ModelTier {
        id: 0,
        model_id: model_id.to_string(),
        provider_model_id: model_id.to_string(),
        provider: provider.to_string(),
        rate_group: rate_group.to_string(),
        capability_class,
        effort_knob: "default".to_string(),
        effort_level: 1,
        cost_per_turn: if is_local_provider(provider) {
            0.0
        } else {
            0.01
        },
        max_context_tokens,
        vram_requirement_mb,
        quantization_type: "q4_K_M".to_string(),
        kv_cache_strategy: "standard".to_string(),
        strengths_json: strengths_json.to_string(),
        enabled: true,
        last_success_rate: 1.0,
        avg_latency_ms: 100,
        latency_p_95_ms: 150,
        updated_at: "2026-06-06T00:00:00Z".to_string(),
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drex_golden/l2")
}

fn parse_mode(routing_metadata: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(routing_metadata)
        .ok()
        .and_then(|v| v.get("mode").and_then(|m| m.as_str()).map(String::from))
}

#[test]
fn drex_golden_l2_route() {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    entries.sort();

    let mut failures: Vec<String> = Vec::new();
    let mut active = 0usize;
    let mut pending = 0usize;

    for path in entries {
        let text = fs::read_to_string(&path).expect("read fixture");
        let case: GoldenCase =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

        if case.status == Status::Pending {
            pending += 1;
            continue;
        }
        active += 1;

        let ingress = DrexIngress {
            intent: case.ingress.intent.clone(),
            risk: case.ingress.risk.clone(),
            raw_text: case.ingress.raw_text.clone(),
            privacy: case.ingress.privacy.clone(),
            runtime: case.ingress.runtime.clone(),
            available_vram_mb: case.ingress.available_vram_mb,
            required_context_tokens: case.ingress.required_context_tokens,
        };
        let tiers = registry(&case.registry);
        let policy = default_policy();
        let result = plan_route(&ingress, &tiers, &policy);

        let mut errs: Vec<String> = Vec::new();
        match case.expect.outcome.as_str() {
            // Refusal semantics: the kernel expresses "no route" as
            // Ok(RoutePlan { selected_model: None, .. }) with the reason in
            // routing_metadata (legacy_no_route / no_admitted_model_call_candidates).
            // Err remains reserved for harness-level faults.
            "error" => match &result {
                Ok(plan) if plan.selected_model.is_some() => errs.push(format!(
                    "expected refusal, got routed (provider={:?})",
                    plan.selected_model.as_ref().map(|m| m.provider.clone())
                )),
                Ok(plan) => {
                    if let Some(sub) = &case.expect.error_contains {
                        if !plan.routing_metadata.contains(sub) {
                            errs.push(format!(
                                "refusal metadata does not contain '{sub}': {}",
                                plan.routing_metadata
                            ));
                        }
                    }
                }
                Err(e) => {
                    if let Some(sub) = &case.expect.error_contains {
                        if !e.to_string().contains(sub) {
                            errs.push(format!("error '{e}' does not contain '{sub}'"));
                        }
                    }
                }
            },
            "routed" => match &result {
                Err(e) => errs.push(format!("expected routed, got error '{e}'")),
                Ok(plan) if plan.selected_model.is_none() => errs.push(format!(
                    "expected routed, got no-model refusal (metadata={})",
                    plan.routing_metadata
                )),
                Ok(plan) => {
                    let model = plan
                        .selected_model
                        .as_ref()
                        .expect("selected_model present per guard above");
                    if let Some(p) = &case.expect.provider {
                        if &model.provider != p {
                            errs.push(format!("provider: expected {p}, got {}", model.provider));
                        }
                    }
                    if let Some(rg) = &case.expect.rate_group {
                        if &model.rate_group != rg {
                            errs.push(format!(
                                "rate_group: expected {rg}, got {}",
                                model.rate_group
                            ));
                        }
                    }
                    if let Some(mid) = &case.expect.model_id {
                        if &model.model_id != mid {
                            errs.push(format!("model_id: expected {mid}, got {}", model.model_id));
                        }
                    }
                    if let Some(local) = case.expect.provider_is_local {
                        if is_local_provider(&model.provider) != local {
                            errs.push(format!(
                                "provider_is_local: expected {local}, got provider={}",
                                model.provider
                            ));
                        }
                    }
                    if let Some(min) = case.expect.min_capability_class {
                        if model.capability_class < min {
                            errs.push(format!(
                                "min_capability_class: expected >= {min}, got {}",
                                model.capability_class
                            ));
                        }
                    }
                    if let Some(mode) = &case.expect.mode {
                        let actual = parse_mode(&plan.routing_metadata);
                        if actual.as_deref() != Some(mode.as_str()) {
                            errs.push(format!("mode: expected {mode}, got {actual:?}"));
                        }
                    }
                }
            },
            other => errs.push(format!("unknown expect.outcome '{other}'")),
        }

        if !errs.is_empty() {
            failures.push(format!("[{}] {}", case.id, errs.join("; ")));
        }
    }

    println!("drex_golden L2: {active} active, {pending} pending fixture(s)");
    assert!(
        active > 0,
        "no active L2 fixtures found in {}",
        dir.display()
    );
    assert!(
        failures.is_empty(),
        "{} L2 golden failure(s) ({} active, {} pending):\n{}",
        failures.len(),
        active,
        pending,
        failures.join("\n")
    );
}
