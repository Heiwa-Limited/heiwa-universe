//! Tests for the `runtime` convenience helpers.

use heiwa_receipts::runtime::{
    compute_or_zero, default_rates, env_for_provider, estimate_tokens, load_rates_or_default,
};
use heiwa_receipts::Env;
use tempfile::TempDir;

#[test]
fn default_rates_cover_marketing_demo_providers() {
    let rates = default_rates();
    for (env, provider, model) in [
        (Env::Local, "ollama", "qwen3.5:9b"),
        (Env::Local, "ollama", "qwen3.5:4b"),
        (Env::Oauth, "claude-code", "claude-sonnet-4-6"),
        (Env::Oauth, "codex", "gpt-5-codex"),
        (Env::Api, "openrouter", "claude-3.7-sonnet"),
    ] {
        let costs = rates
            .compute(env, provider, model, 1_000_000, 1_000_000)
            .unwrap_or_else(|_| {
                panic!("default rates missing entry for {env:?} / {provider} / {model}")
            });
        // OAuth + local entries are zero actual; counterfactual is non-zero
        if matches!(env, Env::Local | Env::Oauth) {
            assert!(costs.actual_cad.abs() < 1e-9);
            assert!(costs.counterfactual_cad > 0.0);
        } else {
            assert!(costs.actual_cad > 0.0);
        }
    }
}

#[test]
fn env_mapping_is_stable() {
    assert_eq!(env_for_provider("ollama"), Env::Local);
    assert_eq!(env_for_provider("claude-code"), Env::Oauth);
    assert_eq!(env_for_provider("codex"), Env::Oauth);
    assert_eq!(env_for_provider("gemini-cli"), Env::Oauth);
    assert_eq!(env_for_provider("antigravity"), Env::Oauth);
    assert_eq!(env_for_provider("openrouter"), Env::Api);
    // Unknown providers default to Api so cost is never silently underreported.
    assert_eq!(env_for_provider("hypothetical_new_lane"), Env::Api);
}

#[test]
fn token_estimator_is_monotonic_and_handles_empty() {
    assert_eq!(estimate_tokens(""), 0);
    let short = estimate_tokens("hello");
    let long = estimate_tokens("hello world, this is a longer string of text");
    assert!(long > short);
    // Sanity-check the rough ratio — 100 chars should land in [25, 30] tokens.
    let hundred = estimate_tokens(&"x".repeat(100));
    assert!(
        (25..=30).contains(&hundred),
        "100 chars -> {hundred} tokens"
    );
}

#[test]
fn load_rates_falls_back_to_default_when_missing() {
    let dir = TempDir::new().unwrap();
    let rates = load_rates_or_default(dir.path());
    // Default table covers ollama qwen3.5:9b — proves we fell back, not panicked.
    let costs = rates
        .compute(Env::Local, "ollama", "qwen3.5:9b", 1_000_000, 0)
        .unwrap();
    assert!(costs.actual_cad.abs() < 1e-9);
    assert!(costs.counterfactual_cad > 0.0);
}

#[test]
fn load_rates_falls_back_when_corrupt() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("rates.toml"), "this is not valid toml = =").unwrap();
    // Should not panic — falls back to defaults.
    let rates = load_rates_or_default(dir.path());
    assert!(rates
        .compute(Env::Local, "ollama", "qwen3.5:9b", 1, 1)
        .is_ok());
}

#[test]
fn load_rates_uses_operator_override() {
    let dir = TempDir::new().unwrap();
    let toml = r#"
[rates.local.ollama."custom-model"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
"#;
    std::fs::write(dir.path().join("rates.toml"), toml).unwrap();
    let rates = load_rates_or_default(dir.path());
    let costs = rates
        .compute(Env::Local, "ollama", "custom-model", 100, 100)
        .unwrap();
    assert_eq!(costs.actual_cad, 0.0);
}

#[test]
fn compute_or_zero_falls_back_on_missing_rate() {
    let rates = default_rates();
    // openrouter::no-such-model isn't in defaults
    let (costs, found) = compute_or_zero(
        &rates,
        Env::Api,
        "openrouter",
        "no-such-model-12345",
        1_000_000,
        1_000_000,
    );
    assert!(!found, "should report missing rate");
    assert_eq!(costs.actual_cad, 0.0);
    assert_eq!(costs.counterfactual_cad, 0.0);
}
