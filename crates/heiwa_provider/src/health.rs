//! Account health projection (L1 interface).
//!
//! A provider that is unauthenticated, rate-limited, or unreachable is a
//! routing constraint, not an application failure. This module turns the
//! account registry into a projection routing can filter on and the AI
//! surface can render: which accounts are usable, and for each one that is
//! not, why it was skipped.
//!
//! Zero routable accounts is a first-class state, not an error — a fresh
//! install has no accounts at all and must still open with an actionable
//! next step.

use crate::registry::{AccountStatus, Credential, ProviderAccount};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// Credential present and last verification succeeded.
    Healthy,
    /// Credential missing, invalid, or expired. The user must act.
    Unauthenticated,
    /// Provider is refusing for now. Temporary; retry later.
    RateLimited,
    /// Endpoint or local runtime did not answer.
    Unreachable,
    /// A CLI-backed account whose binary is not on PATH.
    NotInstalled,
}

impl HealthState {
    /// Whether DREX may route a turn to this account right now.
    pub fn routable(self) -> bool {
        matches!(self, HealthState::Healthy)
    }
}

/// One account's health, with the reason it is unusable when it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHealth {
    pub account_id: String,
    pub provider: String,
    pub credential_kind: String,
    pub state: HealthState,
    /// Human-readable reason, shown verbatim in the AI surface.
    pub detail: String,
    pub routable: bool,
    pub model_count: usize,
}

impl AccountHealth {
    pub fn project(account: &ProviderAccount) -> Self {
        Self::project_with(account, |binary| crate::resolve_command(binary).is_some())
    }

    /// Project against an explicit "can this binary be run" probe.
    ///
    /// Production answers that from `PATH`. Tests must state the answer
    /// instead of inheriting it: whether the developer's machine happens to
    /// have `ollama` installed decides which branch of `classify` runs, so a
    /// test asserting the behaviour behind an *installed* binary passes on a
    /// machine that has it and fails on a CI runner that does not.
    pub fn project_with(account: &ProviderAccount, is_installed: impl Fn(&str) -> bool) -> Self {
        let (state, detail) = classify(account, is_installed);
        Self {
            account_id: account.account_id.clone(),
            provider: account.provider.clone(),
            credential_kind: account.credential.kind_label().to_string(),
            state,
            detail,
            routable: state.routable(),
            model_count: account.models.len(),
        }
    }
}

fn classify(
    account: &ProviderAccount,
    is_installed: impl Fn(&str) -> bool,
) -> (HealthState, String) {
    // An account is only as available as the thing that executes the turn,
    // whatever the stored status says — the binary can disappear between
    // runs, and for a local runtime the daemon that answers discovery is not
    // the same thing as the binary the adapter drives.
    if let Some(binary) = execution_binary(account) {
        if !is_installed(binary) {
            return (
                HealthState::NotInstalled,
                format!("`{binary}` is not installed or not on PATH"),
            );
        }
    }

    match &account.status {
        // Connected with nothing to offer is not usable. It happens on a
        // normal path — add a key while offline, or during a provider 5xx,
        // and discovery leaves the account Connected with no inventory —
        // and routing then finds no model and reports "no working adapters"
        // without ever naming the account or what to do about it.
        AccountStatus::Connected if account.models.is_empty() => (
            HealthState::Unreachable,
            "connected but no models have been detected yet; re-add the key \
             (heiwa auth add-key) to probe the provider's model list"
                .to_string(),
        ),
        AccountStatus::Connected => (HealthState::Healthy, String::new()),
        AccountStatus::NeedsAuth => (
            HealthState::Unauthenticated,
            "no verified credential yet".to_string(),
        ),
        AccountStatus::Disconnected => match &account.credential {
            Credential::LocalRuntime { endpoint } => (
                HealthState::Unreachable,
                format!("local runtime at {endpoint} did not respond"),
            ),
            Credential::OauthCli { binary } => (
                HealthState::Unauthenticated,
                format!("`{binary}` is installed but not signed in"),
            ),
            _ => (
                HealthState::Unreachable,
                "provider did not respond".to_string(),
            ),
        },
        AccountStatus::Error(message) => classify_error(message),
    }
}

/// The binary this account's adapter must be able to run, if any.
///
/// A direct API key needs no local executable. A CLI seat names its binary.
/// A local runtime is discovered over HTTP but driven as a subprocess, so its
/// provider name is the binary that has to exist.
fn execution_binary(account: &ProviderAccount) -> Option<&str> {
    match &account.credential {
        Credential::OauthCli { binary } => Some(binary),
        Credential::LocalRuntime { .. } => Some(&account.provider),
        _ => None,
    }
}

/// Map a stored error string onto a health state.
///
/// Matching on text is imprecise, but the registry records errors as strings
/// and the distinction that matters to routing is coarse: retry later
/// (rate-limited), ask the user (unauthenticated), or skip (unreachable).
fn classify_error(message: &str) -> (HealthState, String) {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("429") || lowered.contains("rate limit") || lowered.contains("quota") {
        return (
            HealthState::RateLimited,
            format!("rate-limited by the provider: {message}"),
        );
    }
    if lowered.contains("401")
        || lowered.contains("403")
        || lowered.contains("invalid api key")
        || lowered.contains("unauthorized")
        || lowered.contains("expired")
    {
        return (
            HealthState::Unauthenticated,
            format!("credential rejected: {message}"),
        );
    }
    (HealthState::Unreachable, message.to_string())
}

/// Health across every account the user holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetHealth {
    pub reports: Vec<AccountHealth>,
}

impl FleetHealth {
    pub fn project(accounts: &[ProviderAccount]) -> Self {
        Self {
            reports: accounts.iter().map(AccountHealth::project).collect(),
        }
    }

    /// Load and project the user's registry.
    pub fn load() -> Self {
        Self::project(&crate::AccountRegistry::load().accounts)
    }

    pub fn routable(&self) -> Vec<&AccountHealth> {
        self.reports
            .iter()
            .filter(|report| report.routable)
            .collect()
    }

    pub fn skipped(&self) -> Vec<&AccountHealth> {
        self.reports
            .iter()
            .filter(|report| !report.routable)
            .collect()
    }

    pub fn has_routable_account(&self) -> bool {
        self.reports.iter().any(|report| report.routable)
    }

    /// What the user should do next, or empty when nothing is wrong.
    ///
    /// This is the text a zero-provider install shows instead of failing.
    pub fn guidance(&self) -> String {
        if self.has_routable_account() {
            return String::new();
        }
        if self.reports.is_empty() {
            return "Connect a provider to start: add an API key for Anthropic, OpenAI, \
                    or Google, point Heiwa at a local runtime such as Ollama, or sign in \
                    to an installed provider CLI."
                .to_string();
        }
        let mut lines =
            vec!["Connect a provider — no account can serve a turn right now:".to_string()];
        for report in self.skipped() {
            lines.push(format!(
                "  {} ({}, {}): {}",
                report.account_id,
                report.provider,
                report.credential_kind,
                if report.detail.is_empty() {
                    "unavailable".to_string()
                } else {
                    report.detail.clone()
                }
            ));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{AccountStatus, Credential, ProviderAccount};

    fn account(id: &str, provider: &str, credential: Credential) -> ProviderAccount {
        ProviderAccount {
            account_id: id.to_string(),
            provider: provider.to_string(),
            credential,
            rate_group: format!("{provider}_api"),
            status: AccountStatus::Connected,
            models: Vec::new(),
        }
    }

    /// An account with something to offer. Health treats a Connected account
    /// with an empty inventory as unusable, so a fixture standing in for a
    /// working account has to carry a model.
    fn stocked(id: &str, provider: &str, credential: Credential) -> ProviderAccount {
        let mut account = account(id, provider, credential);
        account.models = vec![crate::registry::DetectedModel {
            model_id: "a-model".to_string(),
            provider_model_id: "a-model".to_string(),
            provider: provider.to_string(),
            account_id: id.to_string(),
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
        }];
        account
    }

    #[test]
    fn a_connected_api_key_account_is_routable() {
        let report =
            AccountHealth::project(&stocked("anthropic-api-1", "anthropic", Credential::ApiKey));
        assert_eq!(report.state, HealthState::Healthy);
        assert!(report.routable);
        assert_eq!(report.account_id, "anthropic-api-1");
        assert_eq!(report.credential_kind, "api_key");
    }

    #[test]
    fn a_connected_account_with_no_models_is_reported_rather_than_silently_dead() {
        // Reachable from a normal flow: add a valid key while offline, or
        // during a provider 5xx. Routing found no model and told the user
        // "no models with working adapters", naming neither the account nor
        // a way out, and nothing re-probed the inventory afterwards.
        let account = account("anthropic-api-1", "anthropic", Credential::ApiKey);
        assert!(account.models.is_empty());

        let report = AccountHealth::project(&account);

        assert!(!report.routable);
        assert!(
            report.detail.contains("no models"),
            "the reason must name the empty inventory: {}",
            report.detail
        );
        let fleet = FleetHealth::project(std::slice::from_ref(&account));
        assert!(fleet.guidance().contains("anthropic-api-1"));
    }

    #[test]
    fn a_needs_auth_account_is_unauthenticated_and_not_routable() {
        let mut account = account("openai-api-1", "openai", Credential::ApiKey);
        account.status = AccountStatus::NeedsAuth;
        let report = AccountHealth::project(&account);
        assert_eq!(report.state, HealthState::Unauthenticated);
        assert!(!report.routable);
        assert!(report.detail.contains("credential"));
    }

    #[test]
    fn a_rate_limit_error_is_classified_as_rate_limited() {
        let mut account = account("anthropic-api-1", "anthropic", Credential::ApiKey);
        account.status = AccountStatus::Error("HTTP 429: rate limit exceeded".to_string());
        let report = AccountHealth::project(&account);
        assert_eq!(report.state, HealthState::RateLimited);
        // Rate limiting is temporary, so the account stays in the pool and
        // routing steps over it rather than the app failing.
        assert!(!report.routable);
    }

    #[test]
    fn an_auth_error_is_classified_as_unauthenticated() {
        let mut account = account("openai-api-1", "openai", Credential::ApiKey);
        account.status = AccountStatus::Error("Invalid API key".to_string());
        assert_eq!(
            AccountHealth::project(&account).state,
            HealthState::Unauthenticated
        );
    }

    #[test]
    fn an_unclassified_error_is_unreachable_and_keeps_its_detail() {
        let mut account = account("openai-api-1", "openai", Credential::ApiKey);
        account.status = AccountStatus::Error("connection refused".to_string());
        let report = AccountHealth::project(&account);
        assert_eq!(report.state, HealthState::Unreachable);
        assert!(report.detail.contains("connection refused"));
    }

    /// The installed/missing answer is stated, never inherited from the host:
    /// `ollama` present on a developer machine and absent on a CI runner sends
    /// `classify` down different branches for the same account.
    const INSTALLED: fn(&str) -> bool = |_| true;
    const MISSING: fn(&str) -> bool = |_| false;

    #[test]
    fn a_disconnected_local_runtime_is_unreachable() {
        let mut account = account(
            "ollama-local",
            "ollama",
            Credential::LocalRuntime {
                endpoint: "http://127.0.0.1:11434".to_string(),
            },
        );
        account.status = AccountStatus::Disconnected;
        let report = AccountHealth::project_with(&account, INSTALLED);
        assert_eq!(report.state, HealthState::Unreachable);
        assert!(report.detail.contains("11434"));
    }

    #[test]
    fn a_cli_account_whose_binary_is_missing_reports_the_binary_name() {
        let mut account = account(
            "anthropic-cli",
            "anthropic",
            Credential::OauthCli {
                binary: "definitely-not-installed-xyz".to_string(),
            },
        );
        account.status = AccountStatus::Disconnected;
        let report = AccountHealth::project_with(&account, MISSING);
        assert_eq!(report.state, HealthState::NotInstalled);
        assert!(report.detail.contains("definitely-not-installed-xyz"));
        assert!(!report.routable);
    }

    #[test]
    fn a_local_runtime_that_answers_http_but_cannot_be_executed_is_not_routable() {
        // Ollama is discovered over HTTP and driven by shelling out to its
        // binary. Those can disagree — a daemon left running after the binary
        // was removed, or a sandbox with no PATH. Reporting Healthy there
        // sends the turn to an adapter that cannot start, and the whole turn
        // dies on an OS error instead of routing elsewhere.
        let mut account = account(
            "ollama-local",
            "definitely-not-installed-xyz",
            Credential::LocalRuntime {
                endpoint: "http://127.0.0.1:11434".to_string(),
            },
        );
        account.status = AccountStatus::Connected;

        let report = AccountHealth::project_with(&account, MISSING);

        assert_eq!(report.state, HealthState::NotInstalled);
        assert!(!report.routable);
        assert!(report.detail.contains("definitely-not-installed-xyz"));
    }

    #[test]
    fn the_fleet_projection_partitions_routable_from_skipped() {
        let healthy = stocked("anthropic-api-1", "anthropic", Credential::ApiKey);
        let mut broken = account("openai-api-1", "openai", Credential::ApiKey);
        broken.status = AccountStatus::Error("Invalid API key".to_string());

        let fleet = FleetHealth::project(&[healthy, broken]);
        assert_eq!(fleet.routable().len(), 1);
        assert_eq!(fleet.routable()[0].account_id, "anthropic-api-1");
        assert_eq!(fleet.skipped().len(), 1);
        assert!(fleet.has_routable_account());
    }

    #[test]
    fn an_empty_fleet_is_actionable_rather_than_an_error() {
        let fleet = FleetHealth::project(&[]);
        assert!(!fleet.has_routable_account());
        assert!(fleet.reports.is_empty());
        // A fresh install has no accounts; the app must still open and say
        // what to do next.
        let guidance = fleet.guidance();
        assert!(guidance.contains("Connect a provider"));
    }

    #[test]
    fn guidance_names_the_reason_each_account_was_skipped() {
        let mut expired = account("anthropic-api-1", "anthropic", Credential::ApiKey);
        expired.status = AccountStatus::Error("Invalid API key".to_string());
        let fleet = FleetHealth::project(&[expired]);

        let guidance = fleet.guidance();
        assert!(guidance.contains("anthropic-api-1"));
        assert!(guidance.contains("credential"));
    }

    #[test]
    fn guidance_is_silent_when_a_routable_account_exists() {
        let fleet =
            FleetHealth::project(&[stocked("anthropic-api-1", "anthropic", Credential::ApiKey)]);
        assert!(fleet.guidance().is_empty());
    }
}
