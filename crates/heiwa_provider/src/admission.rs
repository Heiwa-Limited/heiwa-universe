//! Callers-first provider admission. Registry metadata is transitional current
//! truth; it is never fresh inference proof. The later observation projection
//! replaces this predicate without reopening adapter construction.
use crate::health::AccountHealth;
use crate::registry::{
    AccountRegistry, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};
use crate::routing::{canonical_provider_id, registry_provider_for};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionSource {
    LegacyCurrentTruth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProviderAdmissionDenial {
    #[error("provider is unsupported")]
    UnsupportedProvider,
    #[error("no configured account; connect this provider in Resources or heiwa auth")]
    AccountMissing,
    #[error("account is unavailable; verify its connection before retrying")]
    AccountUnavailable,
    #[error("the configured provider executable is unavailable")]
    ExecutionSubstrateUnavailable,
    #[error("model inventory is unverified; refresh this account's inventory")]
    ModelInventoryUnverified,
    #[error("the exact requested model is absent from this account's verified inventory")]
    ModelUnavailable,
    #[error("account/model identity is inconsistent or duplicated")]
    SnapshotAmbiguous,
    #[error("the selected account configuration changed; refresh provider routes")]
    AccountChanged,
    #[error("the selected route does not match its admitted account and model")]
    LaneMismatch,
}

/// Not constructible or deserializable by consumers. A witness binds one exact
/// configured account and provider model, never a provider-name fallback.
#[derive(Debug)]
pub struct RegistryAdmittedLane {
    account: ProviderAccount,
    model: DetectedModel,
}

impl RegistryAdmittedLane {
    pub fn account_id(&self) -> &str {
        &self.account.account_id
    }
    pub fn provider(&self) -> &str {
        canonical_provider_id(&self.account.provider)
    }
    pub fn provider_model_id(&self) -> &str {
        &self.model.provider_model_id
    }
    pub fn model_id(&self) -> &str {
        &self.model.model_id
    }
    pub fn rate_group(&self) -> &str {
        &self.account.rate_group
    }
    pub fn source(&self) -> AdmissionSource {
        AdmissionSource::LegacyCurrentTruth
    }
    pub fn source_version(&self) -> u32 {
        1
    }
    pub(crate) fn account(&self) -> &ProviderAccount {
        &self.account
    }

    /// Recheck this exact lane, with no account selection or fallthrough. This
    /// is also used after ranking so removal or changed configuration denies
    /// before a transport can read a credential or start a process.
    pub fn validate_current(
        &self,
        registry: &AccountRegistry,
    ) -> Result<(), ProviderAdmissionDenial> {
        let mut matching = registry
            .accounts
            .iter()
            .filter(|a| a.account_id == self.account_id());
        let account = matching
            .next()
            .ok_or(ProviderAdmissionDenial::AccountMissing)?;
        if matching.next().is_some() {
            return Err(ProviderAdmissionDenial::SnapshotAmbiguous);
        }
        if account.provider != self.account.provider
            || account.credential != self.account.credential
            || account.rate_group != self.account.rate_group
        {
            return Err(ProviderAdmissionDenial::AccountChanged);
        }
        let current = admit_account(account, self.provider_model_id(), |binary| {
            crate::resolve_command(binary).is_some()
        })?;
        if current.model != self.model {
            return Err(ProviderAdmissionDenial::AccountChanged);
        }
        Ok(())
    }
}

pub struct RegistryAdmittedCandidate {
    pub model: DetectedModel,
    pub lane: Arc<RegistryAdmittedLane>,
}

/// Local runtimes retain explicit operator model configuration. A cloud model
/// must come from verified inventory; subscription guesses never authorize it.
pub(crate) fn model_is_admissible(account: &ProviderAccount, model: &DetectedModel) -> bool {
    model.account_id == account.account_id
        && canonical_provider_id(&model.provider) == canonical_provider_id(&account.provider)
        && model.rate_group == account.rate_group
        && !model.model_id.trim().is_empty()
        && !model.provider_model_id.trim().is_empty()
        && (model.inventory_truth == InventoryTruth::Verified
            || (matches!(account.credential, Credential::LocalRuntime { .. })
                && model.inventory_truth == InventoryTruth::UserConfigured))
}

fn expected_binary(provider: &str) -> Option<&'static str> {
    match canonical_provider_id(provider) {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "ollama" => Some("ollama"),
        _ => None,
    }
}

pub(crate) fn configured_channel_supported(account: &ProviderAccount) -> bool {
    match &account.credential {
        Credential::ApiKey => matches!(
            canonical_provider_id(&account.provider),
            "claude" | "codex" | "gemini" | "openrouter"
        ),
        Credential::OauthCli { binary } => {
            matches!(
                canonical_provider_id(&account.provider),
                "claude" | "codex" | "gemini"
            ) && expected_binary(&account.provider) == Some(binary.as_str())
        }
        Credential::LocalRuntime { .. } => canonical_provider_id(&account.provider) == "ollama",
        _ => false,
    }
}

pub(crate) fn admit_account(
    account: &ProviderAccount,
    model_id: &str,
    installed: impl Fn(&str) -> bool,
) -> Result<RegistryAdmittedLane, ProviderAdmissionDenial> {
    if registry_provider_for(&account.provider).is_none() {
        return Err(ProviderAdmissionDenial::UnsupportedProvider);
    }
    if !configured_channel_supported(account) {
        return Err(ProviderAdmissionDenial::AccountUnavailable);
    }
    let health = AccountHealth::project_with(account, installed);
    if health.state == crate::health::HealthState::NotInstalled {
        return Err(ProviderAdmissionDenial::ExecutionSubstrateUnavailable);
    }
    if !health.routable {
        return Err(ProviderAdmissionDenial::AccountUnavailable);
    }
    if model_id.trim().is_empty() {
        return Err(ProviderAdmissionDenial::ModelUnavailable);
    }
    let mut matching = account
        .models
        .iter()
        .filter(|m| m.provider_model_id == model_id || m.model_id == model_id);
    let model = matching
        .next()
        .ok_or(ProviderAdmissionDenial::ModelUnavailable)?;
    if matching.next().is_some() {
        return Err(ProviderAdmissionDenial::SnapshotAmbiguous);
    }
    if !model_is_admissible(account, model) {
        return Err(ProviderAdmissionDenial::ModelInventoryUnverified);
    }
    Ok(RegistryAdmittedLane {
        account: account.clone(),
        model: model.clone(),
    })
}

pub fn admit_registry_lane(
    registry: &AccountRegistry,
    account_id: &str,
    model_id: &str,
) -> Result<RegistryAdmittedLane, ProviderAdmissionDenial> {
    admit_registry_lane_with(registry, account_id, model_id, |binary| {
        crate::resolve_command(binary).is_some()
    })
}

pub fn admit_registry_lane_with(
    registry: &AccountRegistry,
    account_id: &str,
    model_id: &str,
    installed: impl Fn(&str) -> bool,
) -> Result<RegistryAdmittedLane, ProviderAdmissionDenial> {
    let mut matching = registry
        .accounts
        .iter()
        .filter(|a| a.account_id == account_id);
    let account = matching
        .next()
        .ok_or(ProviderAdmissionDenial::AccountMissing)?;
    if matching.next().is_some() {
        return Err(ProviderAdmissionDenial::SnapshotAmbiguous);
    }
    admit_account(account, model_id, installed)
}

pub fn admitted_candidates_with(
    registry: &AccountRegistry,
    installed: impl Fn(&str) -> bool,
) -> Vec<RegistryAdmittedCandidate> {
    let mut ids = HashSet::new();
    let mut duplicate_ids = HashSet::new();
    for account in &registry.accounts {
        if !ids.insert(&account.account_id) {
            duplicate_ids.insert(&account.account_id);
        }
    }
    registry
        .accounts
        .iter()
        .filter(|a| !duplicate_ids.contains(&a.account_id))
        .flat_map(|account| {
            account.models.iter().filter_map(|model| {
                admit_account(account, &model.provider_model_id, &installed)
                    .ok()
                    .map(|lane| RegistryAdmittedCandidate {
                        model: model.clone(),
                        lane: Arc::new(lane),
                    })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountStatus, PriceTruth};

    fn fixture(credential: Credential, truth: InventoryTruth) -> ProviderAccount {
        ProviderAccount {
            account_id: "account-1".into(),
            provider: "openai".into(),
            rate_group: "openai_api".into(),
            credential,
            status: AccountStatus::Connected,
            models: vec![DetectedModel {
                model_id: "short-name".into(),
                provider_model_id: "vendor-model".into(),
                provider: "openai".into(),
                account_id: "account-1".into(),
                rate_group: "openai_api".into(),
                capability_class: 4,
                context_window: 32_000,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: false,
                supports_audio: false,
                cost_per_1k_input: 0.01,
                cost_per_1k_output: 0.01,
                price_truth: PriceTruth::Known,
                inventory_truth: truth,
            }],
        }
    }

    #[test]
    fn witness_names_legacy_source_and_exact_account_model_not_inference_proof() {
        let account = fixture(Credential::ApiKey, InventoryTruth::Verified);
        let registry = AccountRegistry::from_accounts(vec![account]);
        let lane =
            admit_registry_lane_with(&registry, "account-1", "short-name", |_| false).unwrap();
        assert_eq!(lane.account_id(), "account-1");
        assert_eq!(lane.provider_model_id(), "vendor-model");
        assert_eq!(lane.source(), AdmissionSource::LegacyCurrentTruth);
        assert_eq!(lane.source_version(), 1);
    }

    #[test]
    fn cloud_inventory_inferred_or_operator_configured_is_not_admission() {
        for truth in [InventoryTruth::Inferred, InventoryTruth::UserConfigured] {
            let registry = AccountRegistry::from_accounts(vec![fixture(Credential::ApiKey, truth)]);
            assert!(
                admit_registry_lane_with(&registry, "account-1", "vendor-model", |_| true).is_err()
            );
            assert!(admitted_candidates_with(&registry, |_| true).is_empty());
        }
    }

    #[test]
    fn explicit_local_configuration_keeps_its_endpoint_and_inventory_policy() {
        let mut account = fixture(
            Credential::LocalRuntime {
                endpoint: "http://127.0.0.1:19999".into(),
            },
            InventoryTruth::UserConfigured,
        );
        account.provider = "ollama".into();
        account.models[0].provider = "ollama".into();
        let registry = AccountRegistry::from_accounts(vec![account]);
        let lane =
            admit_registry_lane_with(&registry, "account-1", "vendor-model", |_| true).unwrap();
        assert!(
            matches!(&lane.account().credential, Credential::LocalRuntime { endpoint } if endpoint.ends_with(":19999"))
        );
        assert_eq!(admitted_candidates_with(&registry, |_| true).len(), 1);
        assert!(
            admit_registry_lane_with(&registry, "account-1", "vendor-model", |_| false).is_err()
        );
    }

    #[test]
    fn model_identity_and_ranking_metadata_changes_revoke_the_original_witness() {
        let account = fixture(Credential::ApiKey, InventoryTruth::Verified);
        let registry = AccountRegistry::from_accounts(vec![account.clone()]);
        let lane =
            admit_registry_lane_with(&registry, "account-1", "vendor-model", |_| true).unwrap();
        for changed in [
            {
                let mut a = account.clone();
                a.models[0].context_window += 1;
                a
            },
            {
                let mut a = account.clone();
                a.models[0].cost_per_1k_input += 0.1;
                a
            },
            {
                let mut a = account.clone();
                a.models[0].capability_class = 1;
                a
            },
            {
                let mut a = account.clone();
                a.models[0].model_id = "renamed".into();
                a
            },
        ] {
            assert_eq!(
                lane.validate_current(&AccountRegistry::from_accounts(vec![changed])),
                Err(ProviderAdmissionDenial::AccountChanged)
            );
        }
        assert!(lane.validate_current(&registry).is_ok());
    }

    #[test]
    fn account_removal_does_not_fall_through_to_another_account_with_the_same_model() {
        let account = fixture(Credential::ApiKey, InventoryTruth::Verified);
        let lane = admit_registry_lane_with(
            &AccountRegistry::from_accounts(vec![account.clone()]),
            "account-1",
            "vendor-model",
            |_| true,
        )
        .unwrap();
        let mut neighbor = account;
        neighbor.account_id = "account-2".into();
        neighbor.models[0].account_id = "account-2".into();
        assert_eq!(
            lane.validate_current(&AccountRegistry::from_accounts(vec![neighbor])),
            Err(ProviderAdmissionDenial::AccountMissing)
        );
    }

    #[test]
    fn duplicate_account_or_inventory_identity_is_never_an_ordinary_candidate() {
        let account = fixture(Credential::ApiKey, InventoryTruth::Verified);
        let duplicate_accounts =
            AccountRegistry::from_accounts(vec![account.clone(), account.clone()]);
        assert_eq!(
            admit_registry_lane_with(&duplicate_accounts, "account-1", "vendor-model", |_| true)
                .unwrap_err(),
            ProviderAdmissionDenial::SnapshotAmbiguous
        );
        assert!(admitted_candidates_with(&duplicate_accounts, |_| true).is_empty());
        let mut duplicate_models = account;
        duplicate_models
            .models
            .push(duplicate_models.models[0].clone());
        assert!(admitted_candidates_with(
            &AccountRegistry::from_accounts(vec![duplicate_models]),
            |_| true
        )
        .is_empty());
    }
}
