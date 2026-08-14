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
    let registry_provider = registry_provider_for(drex_provider)?;
    registry
        .accounts
        .iter()
        .find(|account| {
            account.provider == registry_provider
                && matches!(account.credential, Credential::ApiKey)
                && AccountHealth::project(account).routable
        })
        .cloned()
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
    if let Some(account) = routable_api_key_account(registry, provider) {
        let models: Vec<String> = account
            .models
            .iter()
            .map(|model| model.provider_model_id.clone())
            .collect();
        let account_id = account.account_id.clone();

        match canonical_provider_id(provider) {
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
            accounts: vec![api_key_account("anthropic", AccountStatus::Connected)],
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

    #[test]
    fn supported_providers_are_recognized_through_their_aliases() {
        assert!(is_supported("claude-code"));
        assert!(is_supported("anthropic"));
        assert!(is_supported("ollama"));
        assert!(!is_supported("mystery-provider"));
    }
}
