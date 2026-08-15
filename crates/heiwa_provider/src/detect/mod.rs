pub mod ollama;

use crate::registry::{add_local_runtime_account, AccountRegistry, AccountStatus, ProviderAccount};

/// A model-discovery failure that kept its HTTP status.
#[derive(Debug)]
pub struct DiscoveryError {
    pub status: u16,
    pub detail: String,
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "models endpoint returned HTTP {}", self.status)
    }
}

impl std::error::Error for DiscoveryError {}

/// Turn a discovery failure into the account status routing reads.
///
/// Status-driven, not text-driven: 401/403 means the credential is bad and
/// the user must act; 429 means try later and must stay distinguishable from
/// both; anything else is transient and leaves the account usable rather
/// than disabling a key over one bad response.
fn status_after_discovery_failure(error: &anyhow::Error) -> AccountStatus {
    let Some(discovery) = error.downcast_ref::<DiscoveryError>() else {
        // Transport failure with no HTTP status: the key may still be good.
        return AccountStatus::Connected;
    };
    match discovery.status {
        401 | 403 => AccountStatus::Error("Invalid API key".to_string()),
        429 => AccountStatus::Error(format!("HTTP 429 rate limit: {}", discovery.detail)),
        _ => AccountStatus::Connected,
    }
}

/// Apply a discovery result to an account.
fn apply_discovery(
    account: &mut ProviderAccount,
    provider_label: &str,
    result: anyhow::Result<Vec<crate::registry::DetectedModel>>,
) -> anyhow::Result<()> {
    match result {
        Ok(models) => {
            account.models = models;
            account.status = AccountStatus::Connected;
            Ok(())
        }
        Err(error) => {
            let status = status_after_discovery_failure(&error);
            let invalid =
                matches!(&status, AccountStatus::Error(message) if message == "Invalid API key");
            account.status = status;
            if invalid {
                account.models.clear();
                return Err(anyhow::anyhow!("Invalid {provider_label} API key"));
            }
            Ok(())
        }
    }
}

/// Provider CLIs Heiwa can drive when the user happens to have them.
///
/// `(provider, binary, rate_group)`. These are optional account kinds: a
/// direct API key is the primary path, and nothing here is required for a
/// turn to run. The CLI owns its own auth, so no secret is stored.
const PROVIDER_CLIS: &[(&str, &str, &str)] = &[
    ("anthropic", "claude", "anthropic_sub"),
    ("openai", "codex", "openai_sub"),
    // `google`, not `google_sub`: heiwa_drex knows budgets for
    // anthropic_sub/openai_sub/google, and an unknown group silently falls
    // through to a conservative 200k ceiling.
    ("google", "gemini", "google"),
];

/// Register an account for each provider CLI present on this machine.
///
/// Returns the account ids newly added. Existing accounts are left alone —
/// re-running discovery must not reset a status an earlier probe established,
/// and must not resurrect an account the user removed on purpose within the
/// same run.
///
/// The installed-probe is a parameter so this is testable without depending
/// on what happens to be installed on the machine running the tests.
fn register_installed_clis(
    registry: &mut AccountRegistry,
    installed: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut added = Vec::new();
    for (provider, binary, rate_group) in PROVIDER_CLIS {
        let account_id = format!("{provider}-cli");
        if registry.get(&account_id).is_some() || !installed(binary) {
            continue;
        }
        registry.upsert(ProviderAccount {
            account_id: account_id.clone(),
            provider: provider.to_string(),
            credential: crate::registry::Credential::OauthCli {
                binary: binary.to_string(),
            },
            rate_group: rate_group.to_string(),
            // Present on PATH is not the same as signed in; the health
            // projection reads `Disconnected` + OauthCli as "installed but
            // not signed in", which is exactly what is known at this point.
            status: AccountStatus::Disconnected,
            models: Vec::new(),
        });
        added.push(account_id);
    }
    added
}

/// Auto-discover local providers and refresh model inventories for all
/// connected accounts.
///
/// Discovers:
/// - Ollama on localhost:11434
/// - Provider CLIs present on PATH, as optional account kinds
pub async fn auto_discover(registry: &mut AccountRegistry) -> Vec<String> {
    let mut changes = Vec::new();

    // --- Ollama auto-discovery ---
    // Ensure an Ollama local account exists if none is registered
    let has_ollama = registry.accounts.iter().any(|a| a.provider == "ollama");
    if !has_ollama {
        if let Ok(endpoint) = ollama::resolve_configured_endpoint(None) {
            if let Ok(id) = add_local_runtime_account(registry, "ollama", endpoint.as_str()) {
                changes.push(format!("Registered Ollama account: {}", id));
            }
        }
    }

    // --- Provider CLI discovery ---
    // Optional account kinds: found because they are installed, never
    // required. A CLI that later disappears projects as NotInstalled rather
    // than failing a turn.
    for account_id in
        register_installed_clis(registry, |binary| crate::resolve_command(binary).is_some())
    {
        changes.push(format!("Registered provider CLI account: {account_id}"));
    }

    // Probe all Ollama accounts
    for account in registry
        .accounts
        .iter_mut()
        .filter(|a| a.provider == "ollama")
    {
        match ollama::detect_models(account).await {
            Ok(()) => {
                changes.push(format!(
                    "{}: detected {} models",
                    account.account_id,
                    account.models.len()
                ));
            }
            Err(_) => {
                // Ollama not running — leave status as Disconnected, clear stale models
            }
        }
    }

    // Save after all discovery
    if !changes.is_empty() {
        registry.save().ok();
    }

    changes
}

/// Verify an API key by probing the provider's model list endpoint.
///
/// On success, updates the account's status to Connected and populates
/// the model inventory.  On failure, sets status to Error.
pub async fn verify_api_key(account: &mut ProviderAccount) -> anyhow::Result<()> {
    let api_key = crate::registry::resolve_secret(&account.account_id).ok_or_else(|| {
        anyhow::anyhow!("No API key found in Keychain for {}", account.account_id)
    })?;

    match account.provider.as_str() {
        "anthropic" => verify_anthropic(account, &api_key).await,
        "openai" => verify_openai(account, &api_key).await,
        "google" => verify_google(account, &api_key).await,
        "openrouter" => verify_openrouter(account, &api_key).await,
        _ => {
            // For unknown providers, just mark as connected (key stored, unverified models)
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify an Anthropic API key by listing models.
///
/// Anthropic publishes `GET /v1/models`, so the key check and the inventory
/// are one call and the inventory is `Verified` rather than a list this
/// crate carries. The previous implementation probed the Messages API with a
/// hardcoded model id, which both invented inventory and pinned a snapshot
/// that goes stale on every model release.
async fn verify_anthropic(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    // Same base URL the turn will use: verifying against the vendor while
    // turns go to a gateway sends the gateway's credential to the vendor.
    let base_url = crate::routing::api_base_url(
        "anthropic",
        crate::providers::anthropic_api::DEFAULT_BASE_URL,
    );
    let result = crate::providers::anthropic_api::discover_models(
        api_key,
        &base_url,
        &account.account_id,
        &account.rate_group,
    )
    .await;
    apply_discovery(account, "Anthropic", result)
}

/// Verify a Google API key by listing models.
async fn verify_google(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    // Same base URL the turn will use: verifying against the vendor while
    // turns go to a gateway sends the gateway's credential to the vendor.
    let base_url =
        crate::routing::api_base_url("google", crate::providers::gemini_api::DEFAULT_BASE_URL);
    let result = crate::providers::gemini_api::discover_models(
        api_key,
        &base_url,
        &account.account_id,
        &account.rate_group,
    )
    .await;
    apply_discovery(account, "Google", result)
}

/// Verify an OpenAI API key by listing models.
async fn verify_openai(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    // Same base URL the turn will use: verifying against the vendor while
    // turns go to a gateway sends the gateway's credential to the vendor.
    let base_url =
        crate::routing::api_base_url("openai", crate::providers::openai_api::DEFAULT_BASE_URL);
    let result = crate::providers::openai_api::discover_models(
        api_key,
        &base_url,
        &account.account_id,
        &account.rate_group,
    )
    .await;
    apply_discovery(account, "OpenAI", result)
}

/// Verify an OpenRouter API key and detect the free-tier model inventory.
///
/// Calls `GET https://openrouter.ai/api/v1/models` and keeps only models
/// whose id carries the `:free` suffix — the zero-cost overflow tier that
/// replaced the dead gemini-cli free seat.  Note the models endpoint is
/// public, so a syntactically-stored but revoked key is only truly proven
/// on the first completion call.
async fn verify_openrouter(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if resp.status().as_u16() == 401 {
        account.status = AccountStatus::Error("Invalid API key".to_string());
        account.models.clear();
        return Err(anyhow::anyhow!("Invalid OpenRouter API key"));
    }

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        account.models = openrouter_free_models(&body, &account.account_id, &account.rate_group);
        account.status = AccountStatus::Connected;
    } else {
        // Transient upstream error — key may still be valid.
        account.status = AccountStatus::Connected;
    }

    Ok(())
}

/// Map an OpenRouter `/models` response to free-tier `DetectedModel`s.
fn openrouter_free_models(
    body: &serde_json::Value,
    account_id: &str,
    rate_group: &str,
) -> Vec<crate::registry::DetectedModel> {
    use crate::registry::{DetectedModel, InventoryTruth};

    let Some(data) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };

    data.iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?;
            if !id.ends_with(":free") {
                return None;
            }
            let input_modalities: Vec<&str> = m
                .pointer("/architecture/input_modalities")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let supported_parameters: Vec<&str> = m
                .get("supported_parameters")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            Some(DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: "openrouter".to_string(),
                account_id: account_id.to_string(),
                rate_group: rate_group.to_string(),
                capability_class: openrouter_capability_class(id),
                context_window: m
                    .get("context_length")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8_192) as u32,
                supports_streaming: true,
                supports_tools: supported_parameters.contains(&"tools"),
                supports_vision: input_modalities.contains(&"image"),
                supports_audio: input_modalities.contains(&"audio"),
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            })
        })
        .collect()
}

/// Capability class for a free-tier OpenRouter model.
///
/// Hard cap at 3: free models absorb bulk/overflow work but must never
/// outrank the OAuth seats (claude/codex, class 4-5) on quality-sensitive
/// intents — class >= 4 is what earns the `advanced_coding` strength.
fn openrouter_capability_class(id: &str) -> u8 {
    let lower = id.to_lowercase();
    let big = [
        "70b",
        "72b",
        "235b",
        "405b",
        "671b",
        "deepseek-r1",
        "deepseek-v3",
    ]
    .iter()
    .any(|hint| lower.contains(hint));
    if big {
        3
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InventoryTruth;

    #[test]
    fn cli_discovery_registers_only_the_binaries_that_are_actually_present() {
        let mut registry = AccountRegistry::default();
        let installed = |binary: &str| binary == "claude";

        let added = register_installed_clis(&mut registry, installed);

        assert_eq!(added, vec!["anthropic-cli".to_string()]);
        let account = registry
            .get("anthropic-cli")
            .expect("the Claude CLI registers under its vendor, not its binary name");
        assert!(matches!(
            &account.credential,
            crate::registry::Credential::OauthCli { binary } if binary == "claude"
        ));
        assert!(
            registry.get("openai-cli").is_none(),
            "an absent binary must not become an account"
        );
    }

    #[test]
    fn cli_discovery_does_not_disturb_an_already_registered_cli_account() {
        // Re-running discovery must not reset a CLI account that a previous
        // probe already marked usable.
        let mut registry = AccountRegistry::default();
        register_installed_clis(&mut registry, |binary| binary == "claude");
        registry.get_mut("anthropic-cli").unwrap().status = AccountStatus::Connected;

        let added = register_installed_clis(&mut registry, |binary| binary == "claude");

        assert!(added.is_empty(), "already-known CLI is not re-announced");
        assert_eq!(
            registry.get("anthropic-cli").unwrap().status,
            AccountStatus::Connected
        );
    }

    #[test]
    fn a_registered_cli_whose_binary_disappears_becomes_not_installed() {
        // The state exists because this transition happens: a user uninstalls
        // the CLI and the account must read as NotInstalled, not as healthy.
        let mut registry = AccountRegistry::default();
        register_installed_clis(&mut registry, |_| true);
        let account = registry.get_mut("openai-cli").unwrap();
        account.status = AccountStatus::Connected;
        account.credential = crate::registry::Credential::OauthCli {
            binary: "heiwa-no-such-binary-exists".to_string(),
        };

        let report = crate::health::AccountHealth::project(registry.get("openai-cli").unwrap());

        assert_eq!(report.state, crate::health::HealthState::NotInstalled);
        assert!(!report.routable);
    }

    fn account_with_verified_inventory() -> ProviderAccount {
        ProviderAccount {
            account_id: "anthropic-api-1".to_string(),
            provider: "anthropic".to_string(),
            credential: crate::registry::Credential::ApiKey,
            rate_group: "anthropic_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![crate::registry::DetectedModel {
                model_id: "claude-opus-5".to_string(),
                provider_model_id: "claude-opus-5".to_string(),
                provider: "anthropic".to_string(),
                account_id: "anthropic-api-1".to_string(),
                rate_group: "anthropic_api".to_string(),
                capability_class: 5,
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            }],
        }
    }

    fn discovery_failure(status: u16, detail: &str) -> anyhow::Error {
        DiscoveryError {
            status,
            detail: detail.to_string(),
        }
        .into()
    }

    #[test]
    fn a_server_error_whose_body_mentions_401_does_not_invalidate_the_key() {
        // Anthropic 5xx bodies carry a request id, and request ids contain
        // arbitrary digits: req_011CS401Nk reads as "401" to a substring match.
        // Classifying on the body once cleared a working key's inventory.
        let mut account = account_with_verified_inventory();
        let result = apply_discovery(
            &mut account,
            "Anthropic",
            Err(discovery_failure(
                500,
                r#"{"type":"error","request_id":"req_011CS401Nk403x"}"#,
            )),
        );

        assert!(
            result.is_ok(),
            "a transient 500 is not a credential verdict"
        );
        assert_eq!(account.status, AccountStatus::Connected);
        assert_eq!(
            account.models.len(),
            1,
            "a 500 must not clear a verified inventory"
        );
    }

    #[test]
    fn an_actual_401_invalidates_the_key_and_clears_inventory() {
        let mut account = account_with_verified_inventory();
        let result = apply_discovery(
            &mut account,
            "Anthropic",
            Err(discovery_failure(401, "authentication_error")),
        );

        assert!(result.is_err());
        assert_eq!(
            account.status,
            AccountStatus::Error("Invalid API key".to_string())
        );
        assert!(account.models.is_empty());
    }

    #[test]
    fn a_429_is_distinguishable_from_both_healthy_and_invalid() {
        let mut account = account_with_verified_inventory();
        let result = apply_discovery(
            &mut account,
            "Anthropic",
            Err(discovery_failure(429, "slow down")),
        );

        assert!(
            result.is_ok(),
            "rate limiting is a wait, not a credential failure"
        );
        let AccountStatus::Error(message) = &account.status else {
            panic!("expected a rate-limit status, got {:?}", account.status);
        };
        assert!(
            message.contains("429"),
            "status must name the rate limit: {message}"
        );
        assert!(
            !account.models.is_empty(),
            "a rate limit must not clear inventory"
        );
    }

    #[test]
    fn a_transport_failure_with_no_http_status_leaves_the_account_usable() {
        // Offline, DNS failure, TLS reset: says nothing about the credential.
        let mut account = account_with_verified_inventory();
        let result = apply_discovery(
            &mut account,
            "Anthropic",
            Err(anyhow::anyhow!("error sending request: connection refused")),
        );

        assert!(result.is_ok());
        assert_eq!(account.status, AccountStatus::Connected);
        assert!(!account.models.is_empty());
    }

    fn sample_models_body() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "id": "meta-llama/llama-3.3-70b-instruct:free",
                    "context_length": 131072,
                    "architecture": { "input_modalities": ["text"] },
                    "supported_parameters": ["tools", "temperature"]
                },
                {
                    "id": "openai/gpt-4o",
                    "context_length": 128000,
                    "architecture": { "input_modalities": ["text", "image"] },
                    "supported_parameters": ["tools"]
                },
                {
                    "id": "qwen/qwen2.5-vl-7b-instruct:free",
                    "context_length": 32768,
                    "architecture": { "input_modalities": ["text", "image"] },
                    "supported_parameters": ["temperature"]
                }
            ]
        })
    }

    #[test]
    fn openrouter_keeps_only_free_suffix_models() {
        let models =
            openrouter_free_models(&sample_models_body(), "openrouter-api-3", "openrouter");
        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .all(|m| m.provider_model_id.ends_with(":free")));
        assert!(models.iter().all(|m| m.provider == "openrouter"));
    }

    #[test]
    fn openrouter_models_are_zero_cost_verified_in_account_rate_group() {
        let models =
            openrouter_free_models(&sample_models_body(), "openrouter-api-3", "openrouter");
        for m in &models {
            assert_eq!(m.account_id, "openrouter-api-3");
            assert_eq!(m.rate_group, "openrouter");
            assert_eq!(m.cost_per_1k_input, 0.0);
            assert_eq!(m.cost_per_1k_output, 0.0);
            assert_eq!(m.inventory_truth, InventoryTruth::Verified);
        }
    }

    #[test]
    fn openrouter_capability_caps_below_oauth_seats() {
        // Free-tier models must never outrank OAuth seats (class 4-5) on
        // quality-sensitive intents: cap at 3 even for large models.
        let models = openrouter_free_models(&sample_models_body(), "a", "openrouter");
        let llama70b = &models[0];
        let qwen7b = &models[1];
        assert_eq!(llama70b.capability_class, 3);
        assert_eq!(qwen7b.capability_class, 2);
        assert!(models.iter().all(|m| m.capability_class <= 3));
    }

    #[test]
    fn openrouter_maps_context_window_and_modalities() {
        let models = openrouter_free_models(&sample_models_body(), "a", "openrouter");
        let llama70b = &models[0];
        assert_eq!(llama70b.context_window, 131_072);
        assert!(llama70b.supports_tools);
        assert!(!llama70b.supports_vision);

        let qwen_vl = &models[1];
        assert_eq!(qwen_vl.context_window, 32_768);
        assert!(!qwen_vl.supports_tools);
        assert!(qwen_vl.supports_vision);
    }
}
