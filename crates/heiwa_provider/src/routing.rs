//! Registry-backed, callers-first admission and witness-bound adapter factory.
//! No implicit CLI/local fallback exists. This transitional stage does not
//! establish fresh inference proof; all witnesses name LegacyCurrentTruth.

use crate::adapter::{Message, ProviderAdapter, StreamEvent};
use crate::admission::{admit_registry_lane, ProviderAdmissionDenial, RegistryAdmittedLane};
use crate::health::AccountHealth;
use crate::providers::{
    anthropic_api::AnthropicApiAdapter, claude_code::ClaudeCodeCliAdapter,
    codex_cli::CodexCliAdapter, gemini_api::GeminiApiAdapter, gemini_cli::GeminiCliAdapter,
    ollama::OllamaCliAdapter, openai_api::OpenAiApiAdapter, openrouter::OpenRouterAdapter,
};
use crate::registry::{AccountRegistry, Credential, ProviderAccount};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

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

/// Pick an exact verified API inventory entry. Empty requests support passive
/// account queries only; adapter resolution always requires a named model.
pub fn routable_api_key_account_for(
    registry: &AccountRegistry,
    drex_provider: &str,
    model_id: &str,
) -> Option<ProviderAccount> {
    let provider = registry_provider_for(drex_provider)?;
    registry
        .accounts
        .iter()
        .find(|account| {
            canonical_provider_id(&account.provider) == canonical_provider_id(provider)
                && matches!(account.credential, Credential::ApiKey)
                && AccountHealth::project(account).routable
                && account.models.iter().any(|model| {
                    crate::admission::model_is_admissible(account, model)
                        && admit_registry_lane(
                            registry,
                            &account.account_id,
                            &model.provider_model_id,
                        )
                        .is_ok()
                        && (model_id.is_empty()
                            || model.provider_model_id == model_id
                            || model.model_id == model_id)
                })
        })
        .cloned()
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

/// Resolve an explicit model using the current registry. Unknown/unavailable
/// accounts return actionable denial before any subprocess or credential read.
pub fn resolve_adapter(provider: &str, model_id: &str) -> Result<Arc<dyn ProviderAdapter>, String> {
    let registry = AccountRegistry::load_strict().map_err(|_| {
        "Provider account configuration could not be read; inspect heiwa doctor".to_string()
    })?;
    resolve_adapter_from_registry(&registry, provider, model_id, None, true)
}

pub fn resolve_adapter_with(
    registry: &AccountRegistry,
    provider: &str,
    model_id: &str,
    base_url_override: Option<&str>,
) -> Result<Arc<dyn ProviderAdapter>, String> {
    resolve_adapter_from_registry(registry, provider, model_id, base_url_override, false)
}

fn resolve_adapter_from_registry(
    registry: &AccountRegistry,
    provider: &str,
    model_id: &str,
    base_url_override: Option<&str>,
    current_registry: bool,
) -> Result<Arc<dyn ProviderAdapter>, String> {
    let vendor = registry_provider_for(provider)
        .ok_or_else(|| ProviderAdmissionDenial::UnsupportedProvider.to_string())?;
    if model_id.trim().is_empty() {
        return Err(ProviderAdmissionDenial::ModelUnavailable.to_string());
    }
    // A key wins only when THIS account admits THIS model. A CLI seat does
    // not lend inventory to a key, and neither invents a default model.
    let mut accounts: Vec<_> = registry
        .accounts
        .iter()
        .filter(|a| canonical_provider_id(&a.provider) == canonical_provider_id(vendor))
        .collect();
    accounts.sort_by_key(|a| !matches!(a.credential, Credential::ApiKey));
    let mut denial = ProviderAdmissionDenial::AccountMissing;
    for account in accounts {
        match admit_registry_lane(registry, &account.account_id, model_id) {
            Ok(lane) => {
                return resolve_admitted_adapter_with(
                    Arc::new(lane),
                    base_url_override,
                    current_registry,
                )
                .map_err(|e| e.to_string())
            }
            Err(error) => denial = error,
        }
    }
    Err(format!("{provider}: {denial}"))
}

/// Construct only from the original admission witness; no registry reload or
/// provider-name account selection occurs inside a transport constructor.
pub fn resolve_admitted_adapter(
    lane: Arc<RegistryAdmittedLane>,
) -> Result<Arc<dyn ProviderAdapter>, ProviderAdmissionDenial> {
    resolve_admitted_adapter_with(lane, None, true)
}

fn resolve_admitted_adapter_with(
    lane: Arc<RegistryAdmittedLane>,
    override_url: Option<&str>,
    current_registry: bool,
) -> Result<Arc<dyn ProviderAdapter>, ProviderAdmissionDenial> {
    crate::admission::admit_account(lane.account(), lane.provider_model_id(), |binary| {
        crate::resolve_command(binary).is_some()
    })?;
    let base = override_url
        .map(str::to_string)
        .or_else(|| env_base_url(lane.provider()));
    let transport: Arc<dyn ProviderAdapter> = match (&lane.account().credential, lane.provider()) {
        (Credential::ApiKey, "claude") => Arc::new(AnthropicApiAdapter::from_admitted_lane(
            &lane,
            base.as_deref()
                .unwrap_or(crate::providers::anthropic_api::DEFAULT_BASE_URL),
        )),
        (Credential::ApiKey, "codex") => Arc::new(OpenAiApiAdapter::from_admitted_lane(
            &lane,
            base.as_deref()
                .unwrap_or(crate::providers::openai_api::DEFAULT_BASE_URL),
        )),
        (Credential::ApiKey, "gemini") => Arc::new(GeminiApiAdapter::from_admitted_lane(
            &lane,
            base.as_deref()
                .unwrap_or(crate::providers::gemini_api::DEFAULT_BASE_URL),
        )),
        (Credential::ApiKey, "openrouter") => {
            Arc::new(OpenRouterAdapter::from_admitted_lane(&lane))
        }
        (Credential::OauthCli { .. }, "claude") => {
            Arc::new(ClaudeCodeCliAdapter::from_admitted_lane(&lane))
        }
        (Credential::OauthCli { .. }, "codex") => {
            Arc::new(CodexCliAdapter::from_admitted_lane(&lane))
        }
        (Credential::OauthCli { .. }, "gemini") => {
            Arc::new(GeminiCliAdapter::from_admitted_lane(&lane))
        }
        (Credential::LocalRuntime { .. }, "ollama") => {
            Arc::new(OllamaCliAdapter::from_admitted_lane(&lane))
        }
        _ => return Err(ProviderAdmissionDenial::UnsupportedProvider),
    };
    Ok(Arc::new(AdmittedAdapter {
        lane,
        transport,
        current_registry,
    }))
}

struct AdmittedAdapter {
    lane: Arc<RegistryAdmittedLane>,
    transport: Arc<dyn ProviderAdapter>,
    current_registry: bool,
}

#[async_trait]
impl ProviderAdapter for AdmittedAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let current_denial = if self.current_registry {
            match AccountRegistry::load_strict() {
                Ok(registry) => self.lane.validate_current(&registry).err(),
                Err(_) => Some(ProviderAdmissionDenial::AccountUnavailable),
            }
        } else {
            None
        };
        let denial = if let Some(denial) = current_denial {
            Some(denial)
        } else if model != self.lane.provider_model_id() && model != self.lane.model_id() {
            Some(ProviderAdmissionDenial::LaneMismatch)
        } else {
            crate::admission::admit_account(
                self.lane.account(),
                self.lane.provider_model_id(),
                |binary| crate::resolve_command(binary).is_some(),
            )
            .err()
        };
        if let Some(denial) = denial {
            let _ = tx.send(StreamEvent::Error(denial.to_string())).await;
            return Err(denial.into());
        }
        self.transport
            .send(self.lane.provider_model_id(), messages, tx)
            .await
    }
    async fn interrupt(&self) -> anyhow::Result<()> {
        self.transport.interrupt().await
    }
    fn supported_models(&self) -> Vec<String> {
        vec![self.lane.provider_model_id().to_string()]
    }
    fn execution_channel(&self) -> crate::adapter::ExecutionChannel {
        self.transport.execution_channel()
    }
}

/// Stable presentation identity includes the exact account and quota group.
/// It is never an authorization token; execution also matches the witness.
pub fn registry_model_identity(model: &crate::DetectedModel) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        canonical_provider_id(&model.provider),
        model.model_id,
        model.provider_model_id,
        model.account_id,
        model.rate_group
    )
}

pub fn registry_model_candidate_id(model: &crate::DetectedModel) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in registry_model_identity(model).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 {
        1
    } else {
        hash
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
        let registry = AccountRegistry::from_accounts(vec![with_models(
            api_key_account("anthropic", AccountStatus::Connected),
            &["claude-opus-5"],
        )]);
        let account = routable_api_key_account(&registry, "claude").expect("account");
        assert_eq!(account.account_id, "anthropic-api-1");
    }

    #[test]
    fn skips_an_unhealthy_api_key_account_so_a_cli_seat_can_serve() {
        let registry = AccountRegistry::from_accounts(vec![api_key_account(
            "anthropic",
            AccountStatus::Error("Invalid API key".to_string()),
        )]);
        assert!(routable_api_key_account(&registry, "claude").is_none());
    }

    #[test]
    fn the_resolved_adapter_reports_the_channel_that_actually_runs() {
        // AccountHealth checks no binary for API keys, so this is hermetic.
        let registry = AccountRegistry::from_accounts(vec![with_models(
            api_key_account("google", AccountStatus::Connected),
            &["gemini-3-flash"],
        )]);
        let api = resolve_adapter_with(&registry, "gemini", "gemini-3-flash", None).unwrap();
        assert_eq!(
            api.execution_channel(),
            crate::adapter::ExecutionChannel::api_key("google-api-1")
        );

        let empty = AccountRegistry::default();
        for provider in ["gemini", "claude", "codex", "ollama"] {
            assert!(resolve_adapter_with(&empty, provider, "m", None).is_err());
        }
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
                price_truth: crate::registry::PriceTruth::Known,
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
        let registry = AccountRegistry::from_accounts(vec![
            with_models(named("anthropic-work", "anthropic"), &["claude-haiku-4-5"]),
            with_models(named("anthropic-personal", "anthropic"), &["claude-opus-5"]),
        ]);

        let account =
            routable_api_key_account_for(&registry, "claude", "claude-opus-5").expect("account");

        assert_eq!(account.account_id, "anthropic-personal");
    }

    #[test]
    fn an_unknown_model_is_denied_even_with_a_healthy_account() {
        // A real account does not authorize an unlisted model.
        let registry = AccountRegistry::from_accounts(vec![with_models(
            named("anthropic-work", "anthropic"),
            &["claude-haiku-4-5"],
        )]);

        assert!(routable_api_key_account_for(&registry, "claude", "some-unlisted-model").is_none());
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
        let registry = AccountRegistry::from_accounts(vec![seat, key]);

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
        let registry = AccountRegistry::from_accounts(vec![broken]);

        assert!(routable_api_key_account_for(&registry, "claude", "claude-opus-5").is_none());
    }

    #[test]
    fn supported_providers_are_recognized_through_their_aliases() {
        assert!(is_supported("claude-code"));
        assert!(is_supported("anthropic"));
        assert!(is_supported("ollama"));
        assert!(!is_supported("mystery-provider"));
    }
    #[test]
    fn missing_registry_never_constructs_implicit_cli_or_local_routes() {
        for provider in ["claude", "codex", "gemini", "ollama", "openrouter"] {
            assert!(
                resolve_adapter_with(&AccountRegistry::default(), provider, "unproven", None)
                    .is_err(),
                "{provider} bypassed admission"
            );
        }
    }

    #[test]
    fn unlisted_model_never_borrows_a_working_api_account() {
        let registry = AccountRegistry::from_accounts(vec![with_models(
            named("anthropic-work", "anthropic"),
            &["allowed"],
        )]);
        assert!(resolve_adapter_with(&registry, "claude", "unlisted", None).is_err());
    }

    #[test]
    fn inferred_inventory_never_constructs_a_metered_adapter() {
        let mut account = with_models(named("anthropic-work", "anthropic"), &["allowed"]);
        account.models[0].inventory_truth = crate::registry::InventoryTruth::Inferred;
        assert!(resolve_adapter_with(
            &AccountRegistry::from_accounts(vec![account]),
            "claude",
            "allowed",
            None
        )
        .is_err());
    }

    #[test]
    fn another_accounts_inventory_never_admits_a_lane() {
        let mut account = with_models(named("anthropic-work", "anthropic"), &["allowed"]);
        account.models[0].account_id = "anthropic-personal".into();
        assert!(resolve_adapter_with(
            &AccountRegistry::from_accounts(vec![account]),
            "claude",
            "allowed",
            None
        )
        .is_err());
    }

    #[test]
    fn disconnected_cli_never_constructs_a_route() {
        let mut account = with_models(named("anthropic-cli", "anthropic"), &["allowed"]);
        account.credential = Credential::OauthCli {
            binary: "claude".into(),
        };
        account.status = AccountStatus::Disconnected;
        assert!(resolve_adapter_with(
            &AccountRegistry::from_accounts(vec![account]),
            "claude",
            "allowed",
            None
        )
        .is_err());
    }

    #[test]
    fn empty_model_request_does_not_select_an_implicit_provider_default() {
        let registry = AccountRegistry::from_accounts(vec![with_models(
            named("anthropic-work", "anthropic"),
            &["allowed"],
        )]);
        assert!(resolve_adapter_with(&registry, "claude", "", None).is_err());
    }

    #[tokio::test]
    async fn changing_the_model_after_admission_denies_before_transport_or_credential_access() {
        struct CountingTransport(std::sync::atomic::AtomicUsize);
        #[async_trait]
        impl ProviderAdapter for CountingTransport {
            async fn send(
                &self,
                _model: &str,
                _messages: &[Message],
                _tx: mpsc::Sender<StreamEvent>,
            ) -> anyhow::Result<()> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
            async fn interrupt(&self) -> anyhow::Result<()> {
                Ok(())
            }
            fn supported_models(&self) -> Vec<String> {
                vec![]
            }
        }
        let account = with_models(named("anthropic-work", "anthropic"), &["allowed"]);
        let registry = AccountRegistry::from_accounts(vec![account]);
        let lane = Arc::new(admit_registry_lane(&registry, "anthropic-work", "allowed").unwrap());
        let transport = Arc::new(CountingTransport(std::sync::atomic::AtomicUsize::new(0)));
        let adapter = AdmittedAdapter {
            lane,
            transport: transport.clone(),
            current_registry: false,
        };
        let (tx, mut rx) = mpsc::channel(4);
        assert!(adapter.send("unlisted", &[], tx).await.is_err());
        assert_eq!(transport.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(matches!(rx.recv().await, Some(StreamEvent::Error(_))));
        assert!(rx.recv().await.is_none());
    }
}
