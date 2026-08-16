//! OS keychain wrapper for Heiwa.
//!
//! Stores OAuth tokens and secrets per user, per machine. Backends:
//! - macOS: Keychain (via `keyring` → Security framework)
//! - Linux: Secret Service (via `keyring` → dbus)
//! - Windows: Credential Manager (via `keyring` → wincred)
//!
//! Secrets never touch disk in plaintext. Each entry is keyed by `(service,
//! account)` where `service` scopes to a Heiwa subsystem (e.g. `"heiwa-oauth"`)
//! and `account` identifies the principal (e.g. `"claude-code:user@local"`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_SERVICE: &str = "heiwa";

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("secret not found: service={service} account={account}")]
    NotFound { service: String, account: String },
    #[error("keyring backend error: {0}")]
    Backend(#[from] keyring::Error),
    #[error("secret is not valid utf8")]
    InvalidUtf8,
    #[error("json decode failed: {0}")]
    Decode(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthSecret {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix: Option<u64>,
    pub scope: Option<String>,
}

pub struct Vault {
    service: String,
}

impl Vault {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(&self.service, account)?)
    }

    pub fn store(&self, account: &str, secret: &str) -> Result<()> {
        self.entry(account)?.set_password(secret)?;
        Ok(())
    }

    pub fn load(&self, account: &str) -> Result<String> {
        match self.entry(account)?.get_password() {
            Ok(s) => Ok(s),
            Err(keyring::Error::NoEntry) => Err(VaultError::NotFound {
                service: self.service.clone(),
                account: account.to_string(),
            }),
            Err(e) => Err(VaultError::Backend(e)),
        }
    }

    pub fn delete(&self, account: &str) -> Result<()> {
        match self.entry(account)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Err(VaultError::NotFound {
                service: self.service.clone(),
                account: account.to_string(),
            }),
            Err(e) => Err(VaultError::Backend(e)),
        }
    }

    pub fn store_oauth(&self, account: &str, secret: &OAuthSecret) -> Result<()> {
        let payload = serde_json::to_string(secret)?;
        self.store(account, &payload)
    }

    pub fn load_oauth(&self, account: &str) -> Result<OAuthSecret> {
        let raw = self.load(account)?;
        Ok(serde_json::from_str(&raw)?)
    }
}

impl Default for Vault {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> String {
        format!("heiwa-test-{}", uuid_like())
    }

    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos:x}")
    }

    // Integration tests: require a real keychain session (GUI on macOS, dbus
    // on Linux, credential manager on Windows). Not safe in sandboxed CI.
    // Run with: `cargo test -p heiwa_vault -- --ignored`.

    #[test]
    #[ignore = "requires real OS keychain session"]
    fn round_trip_string() {
        let vault = Vault::new(test_service());
        let account = "round-trip-user";
        let secret = "s3cret-value";

        vault.store(account, secret).expect("store");
        let loaded = vault.load(account).expect("load");
        assert_eq!(loaded, secret);
        vault.delete(account).expect("delete");
        assert!(matches!(
            vault.load(account),
            Err(VaultError::NotFound { .. })
        ));
    }

    #[test]
    #[ignore = "requires real OS keychain session"]
    fn round_trip_oauth() {
        let vault = Vault::new(test_service());
        let account = "oauth-user";
        let secret = OAuthSecret {
            access_token: "at-abc".into(),
            refresh_token: Some("rt-xyz".into()),
            expires_at_unix: Some(1_800_000_000),
            scope: Some("read write".into()),
        };

        vault.store_oauth(account, &secret).expect("store oauth");
        let loaded = vault.load_oauth(account).expect("load oauth");
        assert_eq!(loaded, secret);
        vault.delete(account).expect("delete");
    }

    #[test]
    fn not_found_returns_typed_error() {
        let vault = Vault::new(test_service());
        match vault.load("does-not-exist") {
            Err(VaultError::NotFound { .. }) => {}
            Ok(_) => panic!("should not find anything"),
            Err(VaultError::Backend(e)) => {
                eprintln!("skipping: backend unavailable: {e}");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}
