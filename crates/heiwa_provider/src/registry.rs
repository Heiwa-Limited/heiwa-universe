use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Credential types
// ---------------------------------------------------------------------------

/// How Heiwa authenticates against a provider for a given account.
///
/// **Secrets are never stored in this struct or in accounts.json.**
/// API keys and OAuth tokens are stored in the macOS Keychain, looked up
/// by `account_id` at runtime via `resolve_secret()`.
///
/// This enum is a *reference type* — it describes the auth mode and any
/// non-secret metadata needed to use it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    /// Direct API key — secret stored in Keychain, not here.
    ApiKey,

    /// OAuth token — secrets stored in Keychain, not here.
    /// `expires_at` is metadata (not secret) so it lives here for
    /// refresh scheduling.
    OAuth { expires_at: Option<String> },

    /// Reference to an installed provider CLI.  Heiwa wraps the subprocess
    /// for subscription-backed access.
    OauthCli { binary: String },

    /// Local runtime with no auth required (Ollama, future local runners).
    LocalRuntime { endpoint: String },
}

impl Credential {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Credential::ApiKey => "api_key",
            Credential::OAuth { .. } => "oauth",
            Credential::OauthCli { .. } => "oauth_cli",
            Credential::LocalRuntime { .. } => "local_runtime",
        }
    }

    /// Whether this credential type has a secret that needs Keychain storage.
    pub fn has_secret(&self) -> bool {
        matches!(self, Credential::ApiKey | Credential::OAuth { .. })
    }
}

// ---------------------------------------------------------------------------
// Provider account
// ---------------------------------------------------------------------------

/// A single authenticated connection to a provider.
///
/// One provider (e.g. "anthropic") can have multiple accounts: an API key
/// AND an OAuth CLI subscription.  Each account has its own rate group,
/// credential, and detected model inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccount {
    /// Unique within the registry.  e.g. "anthropic-api-1", "anthropic-cli-1"
    pub account_id: String,

    /// Provider identifier.  e.g. "anthropic", "openai", "google", "ollama", "openrouter"
    pub provider: String,

    /// How this account authenticates.
    pub credential: Credential,

    /// Rate-limit grouping.  Accounts in the same rate group share quota.
    /// e.g. "anthropic_api", "anthropic_sub", "openai_api", "local"
    pub rate_group: String,

    /// Current connection status.
    pub status: AccountStatus,

    /// Models detected through this account.
    pub models: Vec<DetectedModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Connected,
    Disconnected,
    NeedsAuth,
    Error(String),
}

// ---------------------------------------------------------------------------
// Model inventory
// ---------------------------------------------------------------------------

/// How the model list was obtained — Heiwa stays honest about what it knows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InventoryTruth {
    /// Model list came from a verified source (API endpoint, CLI probe).
    Verified,
    /// Model list was inferred from subscription tier or version string.
    Inferred,
    /// User manually configured this model entry.
    UserConfigured,
}

/// A model detected or configured for a specific provider account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedModel {
    /// Heiwa-normalized model name.  e.g. "claude-sonnet-4"
    pub model_id: String,

    /// Provider's own model identifier.  e.g. "claude-sonnet-4-20250514"
    pub provider_model_id: String,

    /// Which provider this belongs to.
    pub provider: String,

    /// Which account detected this model.
    pub account_id: String,

    /// Rate group inherited from the account.
    pub rate_group: String,

    /// Capability class 1-5 (1=small/fast, 5=frontier).
    pub capability_class: u8,

    /// Maximum context window in tokens.
    pub context_window: u32,

    /// Whether the model supports streaming responses.
    pub supports_streaming: bool,

    /// Whether the model supports tool use / function calling.
    pub supports_tools: bool,

    /// Whether the model supports vision/image inputs.
    pub supports_vision: bool,

    /// Whether the model supports audio inputs/outputs.
    #[serde(default)]
    pub supports_audio: bool,

    /// Cost per 1K input tokens (0.0 for local models).
    pub cost_per_1k_input: f64,

    /// Cost per 1K output tokens (0.0 for local models).
    pub cost_per_1k_output: f64,

    /// How this model information was obtained.
    pub inventory_truth: InventoryTruth,
}

// ---------------------------------------------------------------------------
// Account registry
// ---------------------------------------------------------------------------

fn get_registry_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".heiwa").join("accounts.json")
}

/// The full set of provider accounts Heiwa knows about.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountRegistry {
    pub accounts: Vec<ProviderAccount>,
}

impl AccountRegistry {
    /// Load the registry from disk.  Returns an empty registry if the file
    /// does not exist.
    pub fn load() -> Self {
        let path = get_registry_path();
        if !path.exists() {
            return Self::default();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Persist the registry to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = get_registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// All accounts for a given provider.
    pub fn accounts_for(&self, provider: &str) -> Vec<&ProviderAccount> {
        self.accounts
            .iter()
            .filter(|a| a.provider == provider)
            .collect()
    }

    /// Find an account by its unique ID.
    pub fn get(&self, account_id: &str) -> Option<&ProviderAccount> {
        self.accounts.iter().find(|a| a.account_id == account_id)
    }

    /// Add or update an account.  If an account with the same `account_id`
    /// exists, it is replaced.
    pub fn upsert(&mut self, account: ProviderAccount) {
        if let Some(pos) = self
            .accounts
            .iter()
            .position(|a| a.account_id == account.account_id)
        {
            self.accounts[pos] = account;
        } else {
            self.accounts.push(account);
        }
    }

    /// Remove an account by ID.
    pub fn remove(&mut self, account_id: &str) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.account_id != account_id);
        self.accounts.len() < before
    }

    /// Every detected model across all connected accounts, sorted by
    /// rate group then capability class descending.
    pub fn all_models(&self) -> Vec<&DetectedModel> {
        let mut models: Vec<&DetectedModel> = self
            .accounts
            .iter()
            .filter(|a| a.status == AccountStatus::Connected)
            .flat_map(|a| a.models.iter())
            .collect();
        models.sort_by(|a, b| {
            a.rate_group
                .cmp(&b.rate_group)
                .then(b.capability_class.cmp(&a.capability_class))
                .then(
                    a.cost_per_1k_input
                        .partial_cmp(&b.cost_per_1k_input)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
        models
    }

    /// Unique provider names that have at least one connected account.
    pub fn connected_providers(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for a in &self.accounts {
            if a.status == AccountStatus::Connected && !seen.contains(&a.provider) {
                seen.push(a.provider.clone());
            }
        }
        seen
    }
}

// ---------------------------------------------------------------------------
// Secret storage — Keychain-first
// ---------------------------------------------------------------------------

/// Store an API key or OAuth token for an account.
///
/// On macOS, secrets go to the system Keychain via `security` CLI.
/// The `accounts.json` file never contains raw secrets.
pub fn store_secret(account_id: &str, secret: &str) -> anyhow::Result<()> {
    crate::keychain::store_secret(account_id, secret)
}

/// Retrieve the secret (API key or OAuth token) for an account at runtime.
///
/// Returns `None` if the account has no stored secret or the Keychain
/// is not available.
pub fn resolve_secret(account_id: &str) -> Option<String> {
    crate::keychain::load_secret(account_id).ok()
}

/// Remove the stored secret for an account.
pub fn remove_secret(account_id: &str) -> anyhow::Result<()> {
    crate::keychain::delete_secret(account_id)
}

// ---------------------------------------------------------------------------
// Convenience: add an API key account
// ---------------------------------------------------------------------------

/// Register an API key for a provider.
///
/// The key is stored in the macOS Keychain — `accounts.json` only records
/// the credential kind and account metadata.  Returns the generated account_id.
pub fn add_api_key_account(
    registry: &mut AccountRegistry,
    provider: &str,
    api_key: &str,
    rate_group: &str,
) -> anyhow::Result<String> {
    let existing_count = registry
        .accounts_for(provider)
        .iter()
        .filter(|a| matches!(a.credential, Credential::ApiKey))
        .count();
    let account_id = format!("{}-api-{}", provider, existing_count + 1);

    // Secret → Keychain, not disk
    store_secret(&account_id, api_key)?;

    let account = ProviderAccount {
        account_id: account_id.clone(),
        provider: provider.to_string(),
        credential: Credential::ApiKey,
        rate_group: rate_group.to_string(),
        status: AccountStatus::NeedsAuth, // verified after model detection
        models: Vec::new(),
    };

    registry.upsert(account);
    registry.save()?;

    Ok(account_id)
}

/// Register a local runtime account (e.g. Ollama).
/// No secret needed — local runtimes have no auth.
pub fn add_local_runtime_account(
    registry: &mut AccountRegistry,
    provider: &str,
    endpoint: &str,
) -> anyhow::Result<String> {
    let account_id = format!("{}-local", provider);

    let account = ProviderAccount {
        account_id: account_id.clone(),
        provider: provider.to_string(),
        credential: Credential::LocalRuntime {
            endpoint: endpoint.to_string(),
        },
        rate_group: "local".to_string(),
        status: AccountStatus::Disconnected, // verified after probe
        models: Vec::new(),
    };

    registry.upsert(account);
    registry.save()?;

    Ok(account_id)
}

/// Register an OAuth CLI account (e.g. user has Claude Code installed).
/// No secret stored — the CLI owns its own auth.
pub fn add_cli_account(
    registry: &mut AccountRegistry,
    provider: &str,
    binary: &str,
    rate_group: &str,
) -> anyhow::Result<String> {
    let account_id = format!("{}-cli", provider);

    let account = ProviderAccount {
        account_id: account_id.clone(),
        provider: provider.to_string(),
        credential: Credential::OauthCli {
            binary: binary.to_string(),
        },
        rate_group: rate_group.to_string(),
        status: AccountStatus::Disconnected, // verified after probe
        models: Vec::new(),
    };

    registry.upsert(account);
    registry.save()?;

    Ok(account_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_upsert_and_lookup() {
        let mut reg = AccountRegistry::default();
        let account = ProviderAccount {
            account_id: "test-api-1".to_string(),
            provider: "test".to_string(),
            credential: Credential::ApiKey,
            rate_group: "test_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![],
        };

        reg.upsert(account.clone());
        assert_eq!(reg.accounts.len(), 1);
        assert!(reg.get("test-api-1").is_some());

        // Upsert same ID replaces
        reg.upsert(account);
        assert_eq!(reg.accounts.len(), 1);
    }

    #[test]
    fn registry_multi_account_per_provider() {
        let mut reg = AccountRegistry::default();
        reg.upsert(ProviderAccount {
            account_id: "anthropic-api-1".to_string(),
            provider: "anthropic".to_string(),
            credential: Credential::ApiKey,
            rate_group: "anthropic_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![],
        });
        reg.upsert(ProviderAccount {
            account_id: "anthropic-cli".to_string(),
            provider: "anthropic".to_string(),
            credential: Credential::OauthCli {
                binary: "claude".to_string(),
            },
            rate_group: "anthropic_sub".to_string(),
            status: AccountStatus::Connected,
            models: vec![],
        });

        assert_eq!(reg.accounts_for("anthropic").len(), 2);
        assert_eq!(reg.connected_providers(), vec!["anthropic"]);
    }

    #[test]
    fn all_models_sorted_by_rate_group_then_capability() {
        let mut reg = AccountRegistry::default();
        reg.upsert(ProviderAccount {
            account_id: "a-api-1".to_string(),
            provider: "anthropic".to_string(),
            credential: Credential::ApiKey,
            rate_group: "anthropic_api".to_string(),
            status: AccountStatus::Connected,
            models: vec![
                DetectedModel {
                    model_id: "haiku".to_string(),
                    provider_model_id: "claude-haiku-4-5".to_string(),
                    provider: "anthropic".to_string(),
                    account_id: "a-api-1".to_string(),
                    rate_group: "anthropic_api".to_string(),
                    capability_class: 2,
                    context_window: 200_000,
                    supports_streaming: true,
                    supports_tools: true,
                    supports_vision: true,
                    supports_audio: false,
                    cost_per_1k_input: 0.0008,
                    cost_per_1k_output: 0.004,
                    inventory_truth: InventoryTruth::Verified,
                },
                DetectedModel {
                    model_id: "sonnet".to_string(),
                    provider_model_id: "claude-sonnet-4".to_string(),
                    provider: "anthropic".to_string(),
                    account_id: "a-api-1".to_string(),
                    rate_group: "anthropic_api".to_string(),
                    capability_class: 4,
                    context_window: 200_000,
                    supports_streaming: true,
                    supports_tools: true,
                    supports_vision: true,
                    supports_audio: false,
                    cost_per_1k_input: 0.003,
                    cost_per_1k_output: 0.015,
                    inventory_truth: InventoryTruth::Verified,
                },
            ],
        });
        reg.upsert(ProviderAccount {
            account_id: "ollama-local".to_string(),
            provider: "ollama".to_string(),
            credential: Credential::LocalRuntime {
                endpoint: "http://127.0.0.1:11434".to_string(),
            },
            rate_group: "local".to_string(),
            status: AccountStatus::Connected,
            models: vec![DetectedModel {
                model_id: "llama3.2:3b".to_string(),
                provider_model_id: "llama3.2:3b".to_string(),
                provider: "ollama".to_string(),
                account_id: "ollama-local".to_string(),
                rate_group: "local".to_string(),
                capability_class: 1,
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: false,
                supports_vision: false,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            }],
        });

        let models = reg.all_models();
        assert_eq!(models.len(), 3);
        // anthropic_api sorts before "local", and within anthropic_api, class 4 before class 2
        assert_eq!(models[0].model_id, "sonnet");
        assert_eq!(models[1].model_id, "haiku");
        assert_eq!(models[2].model_id, "llama3.2:3b");
    }

    #[test]
    fn credential_kind_labels() {
        assert_eq!(Credential::ApiKey.kind_label(), "api_key");
        assert_eq!(Credential::OAuth { expires_at: None }.kind_label(), "oauth");
        assert_eq!(
            Credential::OauthCli {
                binary: "claude".to_string()
            }
            .kind_label(),
            "oauth_cli"
        );
        assert_eq!(
            Credential::LocalRuntime {
                endpoint: "http://localhost:11434".to_string()
            }
            .kind_label(),
            "local_runtime"
        );
    }

    #[test]
    fn remove_account() {
        let mut reg = AccountRegistry::default();
        reg.upsert(ProviderAccount {
            account_id: "to-remove".to_string(),
            provider: "test".to_string(),
            credential: Credential::ApiKey,
            rate_group: "test".to_string(),
            status: AccountStatus::Connected,
            models: vec![],
        });
        assert!(reg.remove("to-remove"));
        assert!(!reg.remove("to-remove"));
        assert_eq!(reg.accounts.len(), 0);
    }
}
