//! Secret storage for plain-string credentials (API keys).
//!
//! Delegates to [`heiwa_vault::Vault`], which wraps the platform keyring:
//! macOS Keychain, Linux Secret Service, Windows Credential Manager.
//!
//! Structured OAuth tokens go through [`crate::oauth::ProviderVault`] instead,
//! which uses a separate service namespace so OAuth and API-key secrets can
//! be rotated or wiped independently.

use anyhow::{anyhow, Result};
use heiwa_vault::{Vault, VaultError};

const KEYCHAIN_SERVICE: &str = "heiwa";

fn vault() -> Vault {
    Vault::new(KEYCHAIN_SERVICE)
}

/// Store a secret (API key) under the shared heiwa service.
pub fn store_secret(account_id: &str, secret: &str) -> Result<()> {
    vault()
        .store(account_id, secret)
        .map_err(|e| anyhow!("failed to store secret for {account_id}: {e}"))
}

/// Retrieve a stored secret. Returns an error if not present.
pub fn load_secret(account_id: &str) -> Result<String> {
    match vault().load(account_id) {
        Ok(s) => Ok(s),
        Err(VaultError::NotFound { .. }) => Err(anyhow!("no secret found for {account_id}")),
        Err(e) => Err(anyhow!("keyring error for {account_id}: {e}")),
    }
}

/// Delete a stored secret. Missing entries are treated as success.
pub fn delete_secret(account_id: &str) -> Result<()> {
    match vault().delete(account_id) {
        Ok(()) => Ok(()),
        Err(VaultError::NotFound { .. }) => Ok(()),
        Err(e) => Err(anyhow!("failed to delete secret for {account_id}: {e}")),
    }
}

/// Check whether the keyring backend is reachable on this platform.
///
/// Attempts a throwaway load on a synthetic account. `NotFound` counts as
/// "backend available but account missing" — i.e. healthy.
pub fn is_available() -> bool {
    match vault().load("__heiwa_probe__") {
        Ok(_) => true,
        Err(VaultError::NotFound { .. }) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_available_does_not_panic() {
        // The result depends on the test sandbox; we just assert no panic.
        let _ = is_available();
    }

    #[test]
    #[ignore = "requires real OS keychain session"]
    fn missing_secret_returns_error() {
        let err = load_secret("__nonexistent_heiwa_account__").unwrap_err();
        assert!(err.to_string().contains("no secret found"));
    }
}
