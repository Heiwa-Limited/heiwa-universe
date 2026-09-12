//! Atomic account snapshots with optimistic, per-account conflict detection.
use super::AccountRegistry;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

impl AccountRegistry {
    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.into()),
            Ok(metadata) if !metadata.is_file() => bail!("Account registry must be a regular file"),
            Ok(_) => fs::read_to_string(path).context("Could not read account registry")?,
        };
        let mut registry: Self = serde_json::from_str(&raw).context(
            "Account registry is damaged or uses an unsupported format; it was preserved",
        )?;
        account_values(&registry)?;
        registry.baseline = Some(raw);
        Ok(registry)
    }

    pub fn save_to(&mut self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("Account registry has no parent directory")?;
        fs::create_dir_all(parent)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path.with_extension("lock"))?;
        lock.lock()?;
        let current = Self::load_from(path)?;
        let baseline: Self = self
            .baseline
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        let before = account_values(&baseline)?;
        let requested = account_values(self)?;
        let mut merged = account_values(&current)?;
        let ids: BTreeSet<_> = before.keys().chain(requested.keys()).cloned().collect();
        for id in ids {
            let old = before.get(&id);
            let next = requested.get(&id);
            if old == next {
                continue;
            }
            let live = merged.get(&id);
            if live != old && live != next {
                bail!("Account {id} changed in another window or process. Reload before trying again.");
            }
            if let Some(value) = next {
                merged.insert(id, value.clone());
            } else {
                merged.remove(&id);
            }
        }
        let accounts = merged
            .into_values()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut result = Self::from_accounts(accounts);
        let raw = serde_json::to_string_pretty(&result)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(raw.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        result.baseline = Some(raw);
        *self = result;
        Ok(())
    }
}

fn account_values(registry: &AccountRegistry) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut values = BTreeMap::new();
    for account in &registry.accounts {
        if account.account_id.is_empty()
            || values
                .insert(account.account_id.clone(), serde_json::to_value(account)?)
                .is_some()
        {
            bail!("Account registry contains an empty or duplicate account identity");
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{AccountStatus, Credential, ProviderAccount};

    fn account(id: &str) -> ProviderAccount {
        ProviderAccount {
            account_id: id.into(),
            provider: "openai".into(),
            credential: Credential::ApiKey,
            rate_group: "openai_api".into(),
            status: AccountStatus::NeedsAuth,
            models: vec![],
        }
    }

    #[test]
    fn concurrent_additions_preserve_both_connections_and_repeat_saves_work() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("accounts.json");
        let mut first = AccountRegistry::load_from(&path).unwrap();
        let mut second = first.clone();
        first.upsert(account("first"));
        second.upsert(account("second"));
        first.save_to(&path).unwrap();
        second.save_to(&path).unwrap();
        second.get_mut("second").unwrap().status = AccountStatus::Connected;
        second.save_to(&path).unwrap();
        let loaded = AccountRegistry::load_from(&path).unwrap();
        assert_eq!(loaded.accounts.len(), 2);
        assert_eq!(
            loaded.get("second").unwrap().status,
            AccountStatus::Connected
        );
    }

    #[test]
    fn a_late_probe_cannot_resurrect_a_removed_account() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("accounts.json");
        let mut original = AccountRegistry::from_accounts(vec![account("seat")]);
        original.save_to(&path).unwrap();
        let mut probe = original.clone();
        original.remove("seat");
        original.save_to(&path).unwrap();
        probe.get_mut("seat").unwrap().status = AccountStatus::Connected;
        assert!(probe
            .save_to(&path)
            .unwrap_err()
            .to_string()
            .contains("changed in another"));
        assert!(AccountRegistry::load_from(&path)
            .unwrap()
            .accounts
            .is_empty());
    }

    #[test]
    fn competing_changes_to_one_account_fail_without_overwriting_the_winner() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("accounts.json");
        let mut original = AccountRegistry::from_accounts(vec![account("seat")]);
        original.save_to(&path).unwrap();
        let mut stale = original.clone();
        original.get_mut("seat").unwrap().status = AccountStatus::Connected;
        original.save_to(&path).unwrap();
        stale.get_mut("seat").unwrap().status = AccountStatus::Disconnected;
        assert!(stale.save_to(&path).is_err());
        assert_eq!(
            AccountRegistry::load_from(&path)
                .unwrap()
                .get("seat")
                .unwrap()
                .status,
            AccountStatus::Connected
        );
    }

    #[test]
    fn malformed_future_and_duplicate_records_are_preserved() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("accounts.json");
        let duplicate = serde_json::to_string(&AccountRegistry::from_accounts(vec![
            account("same"),
            account("same"),
        ]))
        .unwrap();
        for raw in [
            "{broken",
            "{\"accounts\":[],\"future_schema\":2}",
            duplicate.as_str(),
        ] {
            fs::write(&path, raw).unwrap();
            assert!(AccountRegistry::load_from(&path).is_err());
            assert!(AccountRegistry::default().save_to(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        }
    }

    #[test]
    fn profiles_do_not_share_connections() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        AccountRegistry::from_accounts(vec![account("personal")])
            .save_to(&first.path().join("accounts.json"))
            .unwrap();
        assert!(
            AccountRegistry::load_from(&second.path().join("accounts.json"))
                .unwrap()
                .accounts
                .is_empty()
        );
    }
}
