//! L1 fresh-install contract.
//!
//! The roadmap's acceptance criterion for L1: *a test harness with no
//! provider CLI on `PATH` and a single API key completes a turn end to end.*
//!
//! The contract test drives the **shipped `heiwa` binary** as a subprocess.
//! That matters more than convenience: a test that constructs an adapter
//! directly proves the adapter works, not that a user's turn reaches it. An
//! earlier version of this file did exactly that, and missed a defect where
//! the shell dropped every direct-API model before routing ever ran — the
//! adapter was fine and the product was broken.
//!
//! A child process also gives real isolation: its own `PATH`, `HOME`, and
//! state root, set at spawn rather than mutated into this process's global
//! environment where parallel tests would race.
//!
//! Everything is hermetic — a temp state root, an emptied `PATH`, no system
//! bin directories, a dead local-runtime endpoint, and a loopback mock
//! speaking the Anthropic Messages API wire format. No network, no reachable
//! CLI, and an account id that matches nothing in the developer's keychain.
//!
//! Scope, stated plainly: this drives the Anthropic wire format. The OpenAI
//! and Google adapters have unit-level wire coverage but are not driven
//! through the binary.

mod support;

use heiwa_provider::health::{FleetHealth, HealthState};
use heiwa_provider::registry::{AccountStatus, InventoryTruth};
use heiwa_provider::routing::{resolve_adapter_with, routable_api_key_account};
use support::{
    assert_no_provider_cli_reachable, single_api_key_registry, FreshInstall, MockProvider,
    HARNESS_API_KEY,
};

/// THE L1 CONTRACT: no provider CLI, one API key, a turn completes —
/// through the shipped binary's own turn path.
#[test]
fn fresh_install_with_one_api_key_and_no_cli_completes_a_turn() {
    let mock = MockProvider::start();
    let install = FreshInstall::new(&single_api_key_registry());
    assert_no_provider_cli_reachable(&install);

    let output = install.run(&mock, &["ask", "Say hello."]);

    let stdout = output.stdout.clone();
    let stderr = output.stderr.clone();
    assert!(
        output.status.success(),
        "heiwa ask failed ({}):\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );

    // The model's text reached the user through the real turn pipeline.
    assert!(
        stdout.contains("Heiwa is running."),
        "the assistant's text did not reach stdout:\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Discovery must not have found a provider CLI. Asserting on the state
    // the child actually wrote is the only version of this check that can
    // fail: the earlier one resolved with empty search lists, which is true
    // for any input, while the child went on registering the real
    // /usr/local/bin/claude it found through the built-in system directories.
    let written: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(install.state_dir.join("accounts.json")).expect("registry"),
    )
    .expect("registry parses");
    let minted: Vec<&str> = written["accounts"]
        .as_array()
        .expect("accounts array")
        .iter()
        .filter(|account| account["credential"]["kind"] == "oauth_cli")
        .filter_map(|account| account["account_id"].as_str())
        .collect();
    assert!(
        minted.is_empty(),
        "a fresh install found provider CLIs: {minted:?}"
    );

    // And the turn really went to the Messages API with the supplied key.
    let turn = mock
        .request_matching("POST /v1/messages")
        .unwrap_or_else(|| {
            panic!(
                "no Messages API request was made; saw {:?}",
                mock.all_requests()
            )
        });
    assert_eq!(
        turn.headers.get("x-api-key").map(String::as_str),
        Some(HARNESS_API_KEY),
        "the request did not carry the user's key"
    );
    assert!(turn.body.contains("\"stream\":true"));
    // Count, not contains: the prompt was being sent twice, and a presence
    // check passed on the doubled body while the user was billed for both.
    assert_eq!(
        turn.body.matches("Say hello.").count(),
        1,
        "the prompt was sent {} times: {}",
        turn.body.matches("Say hello.").count(),
        turn.body
    );
}

/// The turn must reach the provider *because* routing chose the API key,
/// not because a CLI happened to be installed.
#[test]
fn the_completed_turn_routed_through_the_users_api_key() {
    let mock = MockProvider::start();
    let install = FreshInstall::new(&single_api_key_registry());

    let output = install.run(&mock, &["route", "preview", "Say hello."]);
    let stdout = output.stdout.clone();

    assert!(
        output.status.success(),
        "route preview failed: {}",
        output.stderr
    );
    assert!(
        stdout.contains("claude-opus-5"),
        "the direct-API model was dropped before routing:\n{stdout}"
    );
}

/// Model inventory is discovered from the provider, not carried in-tree.
#[test]
fn fresh_install_discovers_model_inventory_from_the_provider() {
    let mock = MockProvider::start();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    let models = runtime
        .block_on(heiwa_provider::providers::anthropic_api::discover_models(
            HARNESS_API_KEY,
            &mock.base_url,
            "anthropic-api-harness",
            "anthropic_api",
        ))
        .expect("discovery succeeds");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider_model_id, "claude-opus-5");
    assert_eq!(models[0].inventory_truth, InventoryTruth::Verified);

    let probe = mock.next_request();
    assert!(probe.line.starts_with("GET /v1/models"));
    assert_eq!(
        probe.headers.get("x-api-key").map(String::as_str),
        Some(HARNESS_API_KEY)
    );
}

/// Zero providers must be an actionable state, not a crash.
#[test]
fn a_fresh_install_with_no_accounts_is_actionable_rather_than_broken() {
    let fleet = FleetHealth::project(&[]);
    assert!(!fleet.has_routable_account());

    let guidance = fleet.guidance();
    assert!(guidance.contains("Connect a provider"));
    // The guidance must name paths a user without any CLI can actually take.
    assert!(guidance.contains("API key"));
}

/// An unusable credential is a routing constraint that names its reason.
#[test]
fn an_expired_credential_is_skipped_with_a_reason_rather_than_failing_the_app() {
    let mut registry = single_api_key_registry();
    registry.accounts[0].status = AccountStatus::Error("Invalid API key".to_string());

    assert!(routable_api_key_account(&registry, "claude").is_none());

    let fleet = FleetHealth::project(&registry.accounts);
    assert!(!fleet.has_routable_account());
    let report = &fleet.reports[0];
    assert_eq!(report.state, HealthState::Unauthenticated);
    assert!(report.detail.contains("Invalid API key"));
    assert!(fleet.guidance().contains("anthropic-api-harness"));
}

/// A rate-limited account is temporary, and says so.
#[test]
fn a_rate_limited_account_is_a_temporary_constraint() {
    let mut registry = single_api_key_registry();
    registry.accounts[0].status = AccountStatus::Error("HTTP 429 rate limit".to_string());

    let fleet = FleetHealth::project(&registry.accounts);
    assert_eq!(fleet.reports[0].state, HealthState::RateLimited);
    assert!(!fleet.reports[0].routable);
}

/// With no CLI installed, resolution still yields an adapter for the key.
#[test]
fn adapter_resolution_prefers_the_users_key_when_no_cli_exists() {
    let mock = MockProvider::start();
    let registry = single_api_key_registry();

    let adapter = resolve_adapter_with(&registry, "claude", "claude-opus-5", Some(&mock.base_url))
        .expect("an adapter resolves from the API key alone");
    assert_eq!(
        adapter.supported_models(),
        vec!["claude-opus-5".to_string()]
    );
}
