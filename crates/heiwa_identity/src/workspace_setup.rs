//! Desktop welcome completion is independent of provider readiness.
//!
//! A user can enter their local workspace with no inference account. This
//! record acknowledges setup; it grants no connector or provider permission.

use serde::{Deserialize, Serialize};
use std::path::Path;

const VERSION: u32 = 1;
const FILE_NAME: &str = "workspace-setup.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSetup {
    pub version: u32,
    pub installation_id: String,
}

pub fn load_from(root: &Path) -> Result<Option<WorkspaceSetup>, String> {
    let bytes = match std::fs::read(root.join(FILE_NAME)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read workspace setup: {error}")),
    };
    let record: WorkspaceSetup = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Cannot read workspace setup: {error}"))?;
    if record.version != VERSION {
        return Err(format!(
            "Workspace setup version {} is unsupported; update Heiwa.",
            record.version
        ));
    }
    let identity = super::load_from(root).map_err(|error| error.to_string())?;
    if identity.as_ref().map(|id| &id.installation_id) != Some(&record.installation_id) {
        return Err(
            "Workspace setup belongs to a different installation; its record was preserved.".into(),
        );
    }
    Ok(Some(record))
}

pub fn complete_in(root: &Path) -> Result<WorkspaceSetup, String> {
    // Preserve unreadable/future records instead of treating them as first run.
    if let Some(record) = load_from(root)? {
        return Ok(record);
    }
    let identity = super::load_from(root)
        .map_err(|error| error.to_string())?
        .ok_or("Create your local identity before opening the workspace.")?;
    let record = WorkspaceSetup {
        version: VERSION,
        installation_id: identity.installation_id,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    super::write_record(&root.join(FILE_NAME), &bytes)
        .map_err(|error| format!("Cannot save workspace setup: {error}"))?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(root: &Path, id: &str) {
        crate::establish_in(root, "New user", "2026-09-11T00:00:00Z", || id.into()).unwrap();
    }

    #[test]
    fn fresh_user_can_finish_without_a_provider_and_reopen_after_restart() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load_from(root.path()).unwrap(), None);
        assert!(complete_in(root.path()).is_err());
        identity(root.path(), "user-a");
        let saved = complete_in(root.path()).unwrap();
        assert_eq!(load_from(root.path()).unwrap(), Some(saved.clone()));
        assert_eq!(complete_in(root.path()).unwrap(), saved);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn profiles_do_not_inherit_another_users_setup() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        identity(a.path(), "user-a");
        identity(b.path(), "user-b");
        complete_in(a.path()).unwrap();
        assert_eq!(load_from(b.path()).unwrap(), None);
        std::fs::copy(a.path().join(FILE_NAME), b.path().join(FILE_NAME)).unwrap();
        assert!(complete_in(b.path())
            .unwrap_err()
            .contains("different installation"));
    }

    #[test]
    fn corrupt_and_future_records_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        identity(root.path(), "user-a");
        for bytes in ["truncated", r#"{"version":2,"installation_id":"user-a"}"#] {
            std::fs::write(root.path().join(FILE_NAME), bytes).unwrap();
            assert!(complete_in(root.path()).is_err());
            assert_eq!(
                std::fs::read_to_string(root.path().join(FILE_NAME)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn failed_replace_preserves_destination_and_cleans_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("record.json");
        std::fs::create_dir(&destination).unwrap();
        assert!(crate::write_record(&destination, b"{}").is_err());
        assert!(destination.is_dir());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
