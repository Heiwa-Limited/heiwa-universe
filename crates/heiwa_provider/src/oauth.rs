//! OAuth credential bridge for provider accounts.
//!
//! Wraps [`heiwa_vault::Vault`] with a provider-scoped service namespace so
//! structured [`OAuthSecret`]s can be stored and retrieved keyed on the
//! `account_id` of a [`crate::registry::ProviderAccount`].
//!
//! Actual provider-specific refresh flows (HTTP to the provider's token
//! endpoint) are deliberately out of scope here — this module just gates
//! storage and exposes the pure [`needs_refresh`] decision helper.

use heiwa_vault::{OAuthSecret, Vault, VaultError};
use thiserror::Error;

/// Service namespace used for all OAuth secrets. Kept separate from the
/// generic `heiwa` service (plain API keys) so the two can be rotated or
/// wiped independently.
pub const OAUTH_SERVICE: &str = "heiwa-oauth";

#[derive(Debug, Error)]
pub enum OAuthBridgeError {
    #[error("no OAuth secret stored for account '{account_id}'")]
    NotFound { account_id: String },
    #[error(transparent)]
    Vault(#[from] VaultError),
}

pub type Result<T> = std::result::Result<T, OAuthBridgeError>;

/// Provider-scoped wrapper around [`heiwa_vault::Vault`].
pub struct ProviderVault {
    vault: Vault,
}

impl ProviderVault {
    pub fn new() -> Self {
        Self {
            vault: Vault::new(OAUTH_SERVICE),
        }
    }

    /// Test/injection constructor — lets callers use a custom service name so
    /// real-keyring round-trip tests don't collide with production secrets.
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            vault: Vault::new(service),
        }
    }

    pub fn store(&self, account_id: &str, secret: &OAuthSecret) -> Result<()> {
        self.vault.store_oauth(account_id, secret)?;
        Ok(())
    }

    pub fn load(&self, account_id: &str) -> Result<OAuthSecret> {
        match self.vault.load_oauth(account_id) {
            Ok(s) => Ok(s),
            Err(VaultError::NotFound { .. }) => Err(OAuthBridgeError::NotFound {
                account_id: account_id.to_string(),
            }),
            Err(e) => Err(OAuthBridgeError::Vault(e)),
        }
    }

    pub fn remove(&self, account_id: &str) -> Result<()> {
        match self.vault.delete(account_id) {
            Ok(()) => Ok(()),
            Err(VaultError::NotFound { .. }) => Ok(()),
            Err(e) => Err(OAuthBridgeError::Vault(e)),
        }
    }
}

impl Default for ProviderVault {
    fn default() -> Self {
        Self::new()
    }
}

/// Decide whether an OAuth secret should be refreshed *now*.
///
/// Returns `true` when `expires_at_unix` is known and `now_unix + skew_seconds`
/// is at or past it. Returns `false` when the secret has no expiry (treated as
/// long-lived) or when there is still more than `skew_seconds` of headroom.
///
/// `skew_seconds` is the early-refresh window — clients typically pass 60–300
/// to avoid presenting a token that expires mid-request.
pub fn needs_refresh(secret: &OAuthSecret, now_unix: u64, skew_seconds: u64) -> bool {
    match secret.expires_at_unix {
        None => false,
        Some(exp) => now_unix.saturating_add(skew_seconds) >= exp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret_expiring_at(exp: Option<u64>) -> OAuthSecret {
        OAuthSecret {
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            expires_at_unix: exp,
            scope: Some("read".into()),
        }
    }

    #[test]
    fn needs_refresh_returns_false_when_no_expiry_recorded() {
        assert!(!needs_refresh(&secret_expiring_at(None), 1_700_000_000, 60));
    }

    #[test]
    fn needs_refresh_returns_false_when_fresh() {
        let s = secret_expiring_at(Some(1_700_000_600));
        assert!(!needs_refresh(&s, 1_700_000_000, 60));
    }

    #[test]
    fn needs_refresh_returns_true_when_expired() {
        let s = secret_expiring_at(Some(1_700_000_000));
        assert!(needs_refresh(&s, 1_700_000_100, 0));
    }

    #[test]
    fn needs_refresh_returns_true_inside_skew_window() {
        let s = secret_expiring_at(Some(1_700_000_030));
        // now + 60s skew puts us past exp, so refresh.
        assert!(needs_refresh(&s, 1_700_000_000, 60));
    }

    #[test]
    fn needs_refresh_saturates_on_overflow() {
        let s = secret_expiring_at(Some(1));
        // Extreme skew — saturate rather than panic.
        assert!(needs_refresh(&s, u64::MAX - 10, u64::MAX));
    }

    fn test_service() -> String {
        format!(
            "heiwa-oauth-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    #[ignore = "requires real OS keychain session"]
    fn round_trip_oauth_secret() {
        let vault = ProviderVault::with_service(test_service());
        let account = "claude-code-oauth-1";
        let stored = OAuthSecret {
            access_token: "tok-abc".into(),
            refresh_token: Some("ref-xyz".into()),
            expires_at_unix: Some(1_999_999_999),
            scope: Some("read write".into()),
        };

        vault.store(account, &stored).unwrap();
        let loaded = vault.load(account).unwrap();
        assert_eq!(loaded, stored);
        vault.remove(account).unwrap();
        assert!(matches!(
            vault.load(account),
            Err(OAuthBridgeError::NotFound { .. })
        ));
    }

    #[test]
    #[ignore = "requires real OS keychain session"]
    fn load_missing_account_is_typed_not_found() {
        let vault = ProviderVault::with_service(test_service());
        let err = vault.load("never-stored").unwrap_err();
        assert!(matches!(err, OAuthBridgeError::NotFound { .. }));
    }
}
