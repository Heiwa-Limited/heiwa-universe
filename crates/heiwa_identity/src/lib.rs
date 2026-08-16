//! Local per-installation identity (L2).
//!
//! The anchor L3 attaches connector credentials to, and the thing L5 would
//! synchronize. It is deliberately *local and per-installation*: minted on
//! first run, stored under the config root, and never contacting a server.
//!
//! Whether identity is also backed by a Heiwa account is the same open fork
//! as cross-device sync (D1) and is not decided here. Nothing in this module
//! forecloses it — the record is versioned, and account backing arrives as an
//! added field rather than a rewrite.
//!
//! Distinct from `heiwa_provider::HeiwaIdentity`, which is a *login* to the
//! Heiwa service: a token obtained from a server. This is the identity that
//! exists before any account does.

pub mod onboarding;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Schema version of the on-disk record.
///
/// Written so a future reader can tell what it is looking at. A record from
/// an unknown future version is left alone rather than overwritten — losing
/// the anchor that connector credentials hang off is worse than refusing.
pub const SCHEMA_VERSION: u32 = 1;

/// Who this installation is, locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIdentity {
    pub version: u32,
    /// Stable for the life of the installation. Connector credentials and
    /// evidence are attributed to it, so it is never regenerated for an
    /// existing install.
    pub installation_id: String,
    /// What the user is called in the interface. Free text, user-editable,
    /// and never used as a key — renaming must not orphan anything.
    pub display_name: String,
    /// RFC 3339, supplied by the caller. This crate does not read the clock,
    /// so its behavior is reproducible under test.
    pub created_at: String,
}

#[derive(Debug)]
pub enum IdentityError {
    /// No per-user root exists, so there is nowhere legitimate to write.
    NoStateRoot,
    /// The stored record is from a schema this build does not understand.
    UnknownVersion(u32),
    Io(std::io::Error),
    Malformed(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::NoStateRoot => write!(
                f,
                "no Heiwa state root: set HEIWA_HOME or HOME before establishing an identity"
            ),
            IdentityError::UnknownVersion(version) => write!(
                f,
                "identity record is schema version {version}, newer than this build understands \
                 (supports {SCHEMA_VERSION}); upgrade Heiwa rather than overwriting it"
            ),
            IdentityError::Io(error) => write!(f, "identity file: {error}"),
            IdentityError::Malformed(detail) => write!(f, "identity file is unreadable: {detail}"),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<std::io::Error> for IdentityError {
    fn from(error: std::io::Error) -> Self {
        IdentityError::Io(error)
    }
}

/// Where the record lives under a given root.
pub fn identity_path_in(runtime_root: &Path) -> PathBuf {
    runtime_root.join("local-identity.json")
}

/// The record for this installation, or `None` if first run has not happened.
pub fn load_from(runtime_root: &Path) -> Result<Option<LocalIdentity>, IdentityError> {
    let path = identity_path_in(runtime_root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let identity: LocalIdentity =
        serde_json::from_str(&raw).map_err(|error| IdentityError::Malformed(error.to_string()))?;
    if identity.version > SCHEMA_VERSION {
        return Err(IdentityError::UnknownVersion(identity.version));
    }
    Ok(Some(identity))
}

/// Establish the identity for this installation, or return the existing one.
///
/// Idempotent by design: first run may be attempted from more than one
/// surface, and the installation id must not change underneath credentials
/// that were issued against it.
pub fn establish_in(
    runtime_root: &Path,
    display_name: &str,
    created_at: &str,
    new_id: impl FnOnce() -> String,
) -> Result<LocalIdentity, IdentityError> {
    if let Some(existing) = load_from(runtime_root)? {
        return Ok(existing);
    }
    let identity = LocalIdentity {
        version: SCHEMA_VERSION,
        installation_id: new_id(),
        display_name: display_name.trim().to_string(),
        created_at: created_at.to_string(),
    };
    write_to(runtime_root, &identity)?;
    Ok(identity)
}

/// Change the display name, keeping the installation id.
pub fn rename_in(runtime_root: &Path, display_name: &str) -> Result<LocalIdentity, IdentityError> {
    let mut identity = load_from(runtime_root)?.ok_or(IdentityError::Malformed(
        "no identity to rename; run first-run setup".to_string(),
    ))?;
    identity.display_name = display_name.trim().to_string();
    write_to(runtime_root, &identity)?;
    Ok(identity)
}

fn write_to(runtime_root: &Path, identity: &LocalIdentity) -> Result<(), IdentityError> {
    std::fs::create_dir_all(runtime_root)?;
    let body = serde_json::to_string_pretty(identity)
        .map_err(|error| IdentityError::Malformed(error.to_string()))?;
    std::fs::write(identity_path_in(runtime_root), body)?;
    Ok(())
}

/// The runtime root for this user, strictly resolved.
///
/// Strict: identity is what credentials attach to, so minting one under the
/// process working directory would silently split a user's installation.
fn runtime_root() -> Result<PathBuf, IdentityError> {
    heiwa_config::HeiwaPaths::try_resolve()
        .map(|paths| paths.runtime_root)
        .ok_or(IdentityError::NoStateRoot)
}

/// The identity for this machine's user, or `None` before first run.
pub fn load() -> Result<Option<LocalIdentity>, IdentityError> {
    load_from(&runtime_root()?)
}

/// Establish this machine's identity, minting an id and stamping the clock.
pub fn establish(display_name: &str, created_at: &str) -> Result<LocalIdentity, IdentityError> {
    establish_in(&runtime_root()?, display_name, created_at, || {
        uuid::Uuid::new_v4().to_string()
    })
}

pub fn rename(display_name: &str) -> Result<LocalIdentity, IdentityError> {
    rename_in(&runtime_root()?, display_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn a_fresh_installation_has_no_identity_yet() {
        let dir = root();
        assert_eq!(load_from(dir.path()).expect("load"), None);
    }

    #[test]
    fn establishing_mints_an_identity_that_survives_a_reload() {
        let dir = root();
        let created = establish_in(dir.path(), "Ada", "2026-08-15T00:00:00Z", || {
            "install-1".to_string()
        })
        .expect("establish");

        assert_eq!(created.installation_id, "install-1");
        assert_eq!(created.display_name, "Ada");
        assert_eq!(created.version, SCHEMA_VERSION);
        assert_eq!(load_from(dir.path()).expect("reload"), Some(created));
    }

    #[test]
    fn establishing_twice_keeps_the_first_installation_id() {
        // First run can be attempted from the CLI and the desktop, or simply
        // re-run. Minting a second id would orphan every connector
        // credential issued against the first.
        let dir = root();
        establish_in(dir.path(), "Ada", "2026-08-15T00:00:00Z", || {
            "install-1".to_string()
        })
        .expect("first");

        let second = establish_in(dir.path(), "Someone Else", "2027-01-01T00:00:00Z", || {
            panic!("a second id must never be minted for an existing installation")
        })
        .expect("second");

        assert_eq!(second.installation_id, "install-1");
        assert_eq!(second.display_name, "Ada");
        assert_eq!(second.created_at, "2026-08-15T00:00:00Z");
    }

    #[test]
    fn renaming_keeps_the_installation_id() {
        let dir = root();
        establish_in(dir.path(), "Ada", "2026-08-15T00:00:00Z", || {
            "install-1".to_string()
        })
        .expect("establish");

        let renamed = rename_in(dir.path(), "Ada Lovelace").expect("rename");

        assert_eq!(renamed.display_name, "Ada Lovelace");
        assert_eq!(renamed.installation_id, "install-1");
    }

    #[test]
    fn a_record_from_a_newer_schema_is_refused_rather_than_overwritten() {
        // Downgrading, or running an old build against a newer install, must
        // not silently discard the anchor credentials hang off.
        let dir = root();
        std::fs::write(
            identity_path_in(dir.path()),
            serde_json::json!({
                "version": SCHEMA_VERSION + 1,
                "installation_id": "install-future",
                "display_name": "Ada",
                "created_at": "2027-01-01T00:00:00Z",
            })
            .to_string(),
        )
        .expect("write future record");

        let error = load_from(dir.path()).expect_err("must refuse");
        assert!(matches!(error, IdentityError::UnknownVersion(_)));

        let error = establish_in(dir.path(), "Ada", "2026-08-15T00:00:00Z", || {
            panic!("must not mint over an unreadable record")
        })
        .expect_err("must refuse");
        assert!(matches!(error, IdentityError::UnknownVersion(_)));
    }

    #[test]
    fn a_corrupt_record_is_reported_rather_than_silently_replaced() {
        let dir = root();
        std::fs::write(identity_path_in(dir.path()), "{not json").expect("write");

        let error = load_from(dir.path()).expect_err("must refuse");
        assert!(matches!(error, IdentityError::Malformed(_)));
    }

    #[test]
    fn the_display_name_is_trimmed_but_otherwise_left_alone() {
        let dir = root();
        let identity = establish_in(
            dir.path(),
            "  Ada Lovelace  ",
            "2026-08-15T00:00:00Z",
            || "install-1".to_string(),
        )
        .expect("establish");

        assert_eq!(identity.display_name, "Ada Lovelace");
    }

    #[test]
    fn the_record_lands_under_the_given_root_and_nowhere_else() {
        let dir = root();
        establish_in(dir.path(), "Ada", "2026-08-15T00:00:00Z", || {
            "install-1".to_string()
        })
        .expect("establish");

        assert!(dir.path().join("local-identity.json").is_file());
    }
}
