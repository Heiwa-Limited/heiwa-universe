use anyhow::{anyhow, Result};
use std::process::Command;

const KEYCHAIN_SERVICE: &str = "heiwa";

/// Store a secret in the macOS Keychain.
///
/// Uses `security add-generic-password` with `-U` (upsert).
/// Falls back to nothing on non-macOS — caller should handle that.
pub fn store_secret(account_id: &str, secret: &str) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow!("Keychain storage only supported on macOS"));
    }

    let status = Command::new("security")
        .arg("add-generic-password")
        .arg("-a").arg(KEYCHAIN_SERVICE)
        .arg("-s").arg(account_id)
        .arg("-w").arg(secret)
        .arg("-U") // upsert
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Failed to store secret in Keychain for {}", account_id))
    }
}

/// Retrieve a secret from the macOS Keychain.
///
/// Uses `security find-generic-password -w` which outputs just the password.
pub fn load_secret(account_id: &str) -> Result<String> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow!("Keychain storage only supported on macOS"));
    }

    let output = Command::new("security")
        .arg("find-generic-password")
        .arg("-a").arg(KEYCHAIN_SERVICE)
        .arg("-s").arg(account_id)
        .arg("-w")
        .output()?;

    if output.status.success() {
        let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(secret)
    } else {
        Err(anyhow!("No secret found in Keychain for {}", account_id))
    }
}

/// Delete a secret from the macOS Keychain.
pub fn delete_secret(account_id: &str) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow!("Keychain storage only supported on macOS"));
    }

    let status = Command::new("security")
        .arg("delete-generic-password")
        .arg("-a").arg(KEYCHAIN_SERVICE)
        .arg("-s").arg(account_id)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        // Not finding the entry is not an error for delete
        Ok(())
    }
}

/// Check if the Keychain is available (macOS with `security` CLI).
pub fn is_available() -> bool {
    cfg!(target_os = "macos")
        && Command::new("security")
            .arg("help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_available_on_macos() {
        if cfg!(target_os = "macos") {
            assert!(is_available());
        }
    }
}
