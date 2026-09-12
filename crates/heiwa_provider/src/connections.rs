//! User-initiated connection setup shared by native clients and the CLI.
//! Credentials enter the OS vault; only account metadata reaches the UI.
use crate::registry::{AccountRegistry, AccountStatus, Credential, ProviderAccount};
use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConnectionSummary {
    pub account_id: String,
    pub provider: String,
    pub channel: &'static str,
    pub status: &'static str,
    pub model_count: usize,
    pub can_manage_key: bool,
}

pub fn summaries(registry: &AccountRegistry) -> Vec<ConnectionSummary> {
    registry
        .accounts
        .iter()
        .map(|account| ConnectionSummary {
            account_id: account.account_id.clone(),
            provider: account.provider.clone(),
            channel: match account.credential {
                Credential::ApiKey => "API key",
                Credential::OauthCli { .. } => "Provider CLI",
                Credential::OAuth { .. } => "Provider account",
                Credential::LocalRuntime { .. } => "Local runtime",
            },
            status: match account.status {
                AccountStatus::Connected => "connected",
                AccountStatus::Disconnected => "disconnected",
                AccountStatus::NeedsAuth => "needs_verification",
                AccountStatus::Error(_) => "verification_failed",
            },
            model_count: account.models.len(),
            can_manage_key: matches!(account.credential, Credential::ApiKey),
        })
        .collect()
}

fn rate_group(provider: &str) -> Result<&'static str> {
    match provider {
        "openai" => Ok("openai_api"),
        "anthropic" => Ok("anthropic_api"),
        "google" => Ok("google_api"),
        "openrouter" => Ok("openrouter"),
        _ => bail!("Choose a supported API provider"),
    }
}

fn checked_key(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty() || key.len() > 16_384 || key.chars().any(char::is_control) {
        bail!("Enter a nonempty API key without control characters (maximum 16 KiB)");
    }
    Ok(key)
}

/// Store first, then verify via the account's model-list endpoint. Verification
/// never generates content. Failed verification keeps the connection visible
/// for retry/removal and clears its candidate inventory.
pub async fn connect_api_key(provider: &str, key: &str) -> Result<()> {
    let group = rate_group(provider)?;
    let key = checked_key(key)?;
    let mut registry = AccountRegistry::load_strict()?;
    let id = crate::registry::add_api_key_account(&mut registry, provider, key, group)
        .context("Could not save the connection in this profile and its OS credential store")?;
    verify_in_registry(&mut registry, &id).await
}

pub async fn verify_api_connection(account_id: &str) -> Result<()> {
    let mut registry = AccountRegistry::load_strict()?;
    verify_in_registry(&mut registry, account_id).await
}

async fn verify_in_registry(registry: &mut AccountRegistry, account_id: &str) -> Result<()> {
    let account = registry
        .get_mut(account_id)
        .context("Connection no longer exists; refresh resources")?;
    require_api_account(account)?;
    // Provider errors can contain response bodies. Persist a bounded safe
    // diagnosis, never provider text or the submitted secret.
    if crate::detect::verify_api_key(account).await.is_err() {
        account.status = AccountStatus::Error(
            "Verification failed; check the key and network, then retry".into(),
        );
        account.models.clear();
    }
    registry.save()?;
    Ok(())
}

pub fn disconnect_api_connection(account_id: &str) -> Result<()> {
    let mut registry = AccountRegistry::load_strict()?;
    let account = registry
        .get(account_id)
        .context("Connection no longer exists; refresh resources")?;
    require_api_account(account)?;
    registry.remove(account_id);
    // Remove routing access before touching the credential. A concurrent
    // verification cannot reinsert the account because save checks its base.
    registry.save()?;
    crate::keychain::delete_secret(account_id)
        .context("Connection removed, but its credential could not be removed from the OS store")?;
    Ok(())
}

fn require_api_account(account: &ProviderAccount) -> Result<()> {
    rate_group(&account.provider)?;
    if !matches!(account.credential, Credential::ApiKey) {
        bail!("This action manages API keys only; provider-owned sign-in remains in its own application");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_rejects_unknown_targets_and_control_characters_before_storage() {
        assert!(rate_group("https://untrusted.example").is_err());
        for input in ["", "   ", "key\ninjected", "key\0injected"] {
            assert!(checked_key(input).is_err());
        }
        assert!(checked_key(&"x".repeat(16_385)).is_err());
    }

    #[test]
    fn connection_projection_never_includes_provider_error_bodies() {
        let mut account = ProviderAccount {
            account_id: "seat".into(),
            provider: "openai".into(),
            credential: Credential::ApiKey,
            rate_group: "openai_api".into(),
            status: AccountStatus::Error("sensitive provider response".into()),
            models: vec![],
        };
        let json = serde_json::to_string(&summaries(&AccountRegistry::from_accounts(vec![
            account.clone(),
        ])))
        .unwrap();
        assert!(!json.contains("sensitive provider response"));
        assert!(json.contains("verification_failed"));
        account.credential = Credential::OauthCli {
            binary: "codex".into(),
        };
        assert!(require_api_account(&account).is_err());
    }
}
