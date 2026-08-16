//! Adapter selection: which of a user's accounts serves a given provider.
//!
//! Selection lives with the adapters rather than in the shell binary so
//! every surface — CLI, desktop runtime, tests — resolves a provider the
//! same way, and so the fresh-install path is reachable from an integration
//! test.
//!
//! The rule is: a user's own API key takes the route when they have a
//! healthy one, and the CLI adapter is the fallback. Both stay available —
//! a subscription seat carries the provider's own auth, quota, and session
//! behavior, and a key works on a machine where no CLI is installed.

use crate::adapter::ProviderAdapter;
use crate::health::AccountHealth;
use crate::providers::{
    anthropic_api::AnthropicApiAdapter, claude_code::ClaudeCodeCliAdapter,
    codex_cli::CodexCliAdapter, gemini_api::GeminiApiAdapter, gemini_cli::GeminiCliAdapter,
    ollama::OllamaCliAdapter, openai_api::OpenAiApiAdapter, openrouter::OpenRouterAdapter,
};
use crate::registry::{AccountRegistry, Credential, ProviderAccount};
use std::sync::Arc;

/// DREX provider ids this crate can serve.
pub const SUPPORTED_PROVIDERS: &[&str] = &["ollama", "claude", "codex", "gemini", "openrouter"];

/// Normalize the aliases that reach routing from different surfaces.
pub fn canonical_provider_id(provider: &str) -> &str {
    match provider {
        "claude-code" => "claude",
        "google-gemini-cli" => "gemini",
        "anthropic" => "claude",
        "openai" => "codex",
        "google" => "gemini",
        other => other,
    }
}

/// Registry provider name for a DREX provider id.
///
/// DREX names routes after the surface the user knows (`claude`, `codex`,
/// `gemini`); the registry names them after the vendor whose credential it
/// holds, which is what a user's API key is registered under.
pub fn registry_provider_for(drex_provider: &str) -> Option<&'static str> {
    match canonical_provider_id(drex_provider) {
        "claude" => Some("anthropic"),
        "codex" => Some("openai"),
        "gemini" => Some("google"),
        "openrouter" => Some("openrouter"),
        "ollama" => Some("ollama"),
        _ => None,
    }
}

pub fn is_supported(provider: &str) -> bool {
    SUPPORTED_PROVIDERS.contains(&canonical_provider_id(provider))
}

/// The user's routable direct-API account for a provider, if any.
///
/// Health is consulted here so an expired or rate-limited key does not win
/// the route over a working CLI seat.
pub fn routable_api_key_account(
    registry: &AccountRegistry,
    drex_provider: &str,
) -> Option<ProviderAccount> {
    routable_api_key_account_for(registry, drex_provider, "")
}

/// Pick the account that can actually serve `model_id`.
///
/// A user may hold several keys for one vendor with different inventories —
/// a work seat and a personal seat, or a key whose org has no Opus access.
/// Taking the first routable account sends the turn to a seat that may not
/// serve the routed model. An account that lists the model wins; otherwise
/// any healthy account does, because an empty inventory means "not probed",
/// not "cannot serve".
pub fn routable_api_key_account_for(
    registry: &AccountRegistry,
    drex_provider: &str,
    model_id: &str,
) -> Option<ProviderAccount> {
    let registry_provider = registry_provider_for(drex_provider)?;
    let candidates: Vec<&ProviderAccount> = registry
        .accounts
        .iter()
        .filter(|account| {
            account.provider == registry_provider
                && matches!(account.credential, Credential::ApiKey)
                && AccountHealth::project(account).routable
        })
        .collect();

    if let Some(serving) = candidates.iter().find(|account| {
        account
            .models
            .iter()
            .any(|model| model.provider_model_id == model_id || model.model_id == model_id)
    }) {
        return Some((*serving).clone());
    }

    // No API-key account lists this model. If some *other* account does — a
    // subscription seat, say — that account owns the model, and falling back
    // to a metered key would move the charge to the user's card while quota
    // still debits the seat. Only fall back when nothing claims the model.
    let claimed_elsewhere = !model_id.is_empty()
        && registry.accounts.iter().any(|account| {
            !matches!(account.credential, Credential::ApiKey)
                && account
                    .models
                    .iter()
                    .any(|model| model.provider_model_id == model_id || model.model_id == model_id)
        });
    if claimed_elsewhere {
        return None;
    }

    candidates.first().map(|account| (*account).clone())
}

/// Environment variable that retargets a provider's API base URL.
///
/// The provider's own published name, so a machine already pointed at a
/// gateway, proxy, or self-hosted endpoint for that vendor's SDK works
/// without extra Heiwa configuration.
fn base_url_env_var(canonical_provider: &str) -> Option<&'static str> {
    match canonical_provider {
        "claude" => Some("ANTHROPIC_BASE_URL"),
        "codex" => Some("OPENAI_BASE_URL"),
        "gemini" => Some("GEMINI_BASE_URL"),
        _ => None,
    }
}

/// The base URL a provider's calls should use.
///
/// Every call for a provider must agree on this. Verification used to
/// hardcode the vendor default while turns honored the override, so a user
/// on a gateway had their gateway credential transmitted to the vendor,
/// rejected, and their account permanently marked invalid.
pub fn api_base_url(provider: &str, default: &str) -> String {
    env_base_url(canonical_provider_id(provider)).unwrap_or_else(|| default.to_string())
}

/// Base URL override for a provider, from the environment.
fn env_base_url(canonical_provider: &str) -> Option<String> {
    let name = base_url_env_var(canonical_provider)?;
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Resolve an adapter for a provider using the user's own registry.
pub fn resolve_adapter(provider: &str, model_id: &str) -> Result<Arc<dyn ProviderAdapter>, String> {
    resolve_adapter_with(&AccountRegistry::load(), provider, model_id, None)
}

/// Resolve against an explicit registry.
///
/// `base_url_override` retargets direct-API adapters, which is how the
/// fresh-install harness points them at a loopback mock without the
/// adapters knowing they are under test.
pub fn resolve_adapter_with(
    registry: &AccountRegistry,
    provider: &str,
    model_id: &str,
    base_url_override: Option<&str>,
) -> Result<Arc<dyn ProviderAdapter>, String> {
    if let Some(account) = routable_api_key_account_for(registry, provider, model_id) {
        let models: Vec<String> = account
            .models
            .iter()
            .map(|model| model.provider_model_id.clone())
            .collect();
        let account_id = account.account_id.clone();

        let canonical = canonical_provider_id(provider);
        let from_env = env_base_url(canonical);
        let override_url = base_url_override.map(str::to_string).or(from_env);
        let base_url_override = override_url.as_deref();

        match canonical {
            "claude" => {
                let base =
                    base_url_override.unwrap_or(crate::providers::anthropic_api::DEFAULT_BASE_URL);
                return Ok(Arc::new(
                    AnthropicApiAdapter::new(account_id, base).with_models(models),
                ));
            }
            "codex" => {
                let base =
                    base_url_override.unwrap_or(crate::providers::openai_api::DEFAULT_BASE_URL);
                return Ok(Arc::new(
                    OpenAiApiAdapter::new(account_id, base).with_models(models),
                ));
            }
            "gemini" => {
                let base =
                    base_url_override.unwrap_or(crate::providers::gemini_api::DEFAULT_BASE_URL);
                return Ok(Arc::new(
                    GeminiApiAdapter::new(account_id, base).with_models(models),
                ));
            }
            _ => {}
        }
    }

    match canonical_provider_id(provider) {
        "ollama" => Ok(Arc::new(OllamaCliAdapter::with_model(model_id))),
        "claude" => Ok(Arc::new(ClaudeCodeCliAdapter::new())),
        "codex" => Ok(Arc::new(CodexCliAdapter::new())),
        "gemini" => Ok(Arc::new(GeminiCliAdapter::new())),
        "openrouter" => OpenRouterAdapter::from_registry()
            .map(|adapter| Arc::new(adapter) as Arc<dyn ProviderAdapter>)
            .ok_or_else(|| {
                "No OpenRouter account registered (heiwa auth add-key openrouter <key>)."
                    .to_string()
            }),
        _ => Err(format!("No adapter for provider '{provider}' yet.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AccountStatus;

    fn api_key_account(provider: &str, status: AccountStatus) -> ProviderAccount {
        ProviderAccount {
            account_id: format!("{provider}-api-1"),
            provider: provider.to_string(),
            credential: Credential::ApiKey,
            rate_group: format!("{provider}_api"),
            status,
            models: Vec::new(),
        }
    }

    #[test]
    fn maps_drex_route_names_onto_registry_vendor_names() {
        assert_eq!(registry_provider_for("claude"), Some("anthropic"));
        assert_eq!(registry_provider_for("claude-code"), Some("anthropic"));
        assert_eq!(registry_provider_for("codex"), Some("openai"));
        assert_eq!(registry_provider_for("gemini"), Some("google"));
        assert_eq!(registry_provider_for("nope"), None);
    }

    #[test]
    fn picks_a_healthy_api_key_account_for_the_route() {
        let registry = AccountRegistry {
            accounts: vec![with_models(
                api_key_account("anthropic", AccountStatus::Connected),
                &["claude-opus-5"],
            )],
        };
        let account = routable_api_key_account(&registry, "claude").expect("account");
        assert_eq!(account.account_id, "anthropic-api-1");
    }

    #[test]
    fn skips_an_unhealthy_api_key_account_so_a_cli_seat_can_serve() {
        let registry = AccountRegistry {
            accounts: vec![api_key_account(
                "anthropic",
                AccountStatus::Error("Invalid API key".to_string()),
            )],
        };
        assert!(routable_api_key_account(&registry, "claude").is_none());
    }

    #[test]
    fn an_empty_registry_yields_no_direct_api_account() {
        let registry = AccountRegistry::default();
        for provider in ["claude", "codex", "gemini"] {
            assert!(routable_api_key_account(&registry, provider).is_none());
        }
    }

    fn with_models(mut account: ProviderAccount, model_ids: &[&str]) -> ProviderAccount {
        account.models = model_ids
            .iter()
            .map(|id| crate::registry::DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: account.provider.clone(),
                account_id: account.account_id.clone(),
                rate_group: account.rate_group.clone(),
                capability_class: 4,
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: false,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: crate::registry::InventoryTruth::Verified,
            })
            .collect();
        account
    }

    fn named(account_id: &str, provider: &str) -> ProviderAccount {
        let mut account = api_key_account(provider, AccountStatus::Connected);
        account.account_id = account_id.to_string();
        account
    }

    #[test]
    fn the_account_that_serves_the_model_wins_over_registry_order() {
        // Two keys for the same vendor with different inventories — a work key
        // and a personal key. Taking the first would send an Opus turn to a
        // seat that cannot serve Opus.
        let registry = AccountRegistry {
            accounts: vec![
                with_models(named("anthropic-work", "anthropic"), &["claude-haiku-4-5"]),
                with_models(named("anthropic-personal", "anthropic"), &["claude-opus-5"]),
            ],
        };

        let account =
            routable_api_key_account_for(&registry, "claude", "claude-opus-5").expect("account");

        assert_eq!(account.account_id, "anthropic-personal");
    }

    #[test]
    fn an_unknown_model_still_routes_to_a_healthy_account() {
        // Inventory may be empty (never probed) or the caller may pass a model
        // this crate has not seen. Refusing to route would turn a working key
        // into a dead end.
        let registry = AccountRegistry {
            accounts: vec![with_models(
                named("anthropic-work", "anthropic"),
                &["claude-haiku-4-5"],
            )],
        };

        let account = routable_api_key_account_for(&registry, "claude", "some-unlisted-model")
            .expect("account");

        assert_eq!(account.account_id, "anthropic-work");
    }

    #[test]
    fn a_subscription_seats_model_does_not_get_billed_to_a_metered_key() {
        // A user with both a Claude Code seat and an Anthropic key. Routing
        // picks a model the seat serves; taking "any healthy API-key account"
        // would run it on the metered key while quota still debits the seat.
        let mut seat = named("anthropic-cli", "anthropic");
        seat.credential = Credential::OauthCli {
            binary: "claude".to_string(),
        };
        seat.rate_group = "claude_code".to_string();
        let seat = with_models(seat, &["claude-fable-5"]);
        let key = with_models(named("anthropic-api-1", "anthropic"), &["claude-opus-5"]);
        let registry = AccountRegistry {
            accounts: vec![seat, key],
        };

        assert!(
            routable_api_key_account_for(&registry, "claude", "claude-fable-5").is_none(),
            "a seat's model must not fall back to a metered key"
        );
        // The key's own model still routes to the key.
        assert_eq!(
            routable_api_key_account_for(&registry, "claude", "claude-opus-5")
                .expect("account")
                .account_id,
            "anthropic-api-1"
        );
    }

    #[test]
    fn a_model_served_only_by_an_unhealthy_account_does_not_resurrect_it() {
        let mut broken = with_models(named("anthropic-work", "anthropic"), &["claude-opus-5"]);
        broken.status = AccountStatus::Error("Invalid API key".to_string());
        let registry = AccountRegistry {
            accounts: vec![broken],
        };

        assert!(routable_api_key_account_for(&registry, "claude", "claude-opus-5").is_none());
    }

    #[test]
    fn supported_providers_are_recognized_through_their_aliases() {
        assert!(is_supported("claude-code"));
        assert!(is_supported("anthropic"));
        assert!(is_supported("ollama"));
        assert!(!is_supported("mystery-provider"));
    }
}
