use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub port: u16,
    pub state_backend: String,
    pub log_level: String,
    pub machine_auth_token: String,
    pub jwt_signing_secret: String,
    pub node_id: String,
    pub model_tiers_seed_path: String,
    pub ai_router_seed_path: String,
}

impl RuntimeConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let heiwa_home = heiwa_home_from_env();
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            state_backend: env::var("HEIWA_STATE_BACKEND")
                .unwrap_or_else(|_| "local-jsonl".to_string()),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),
            machine_auth_token: resolve_runtime_secret(
                env::var("HEIWA_MACHINE_AUTH_TOKEN").ok(),
                env::var("HEIWA_AUTH_TOKEN").ok(),
                heiwa_home.as_deref(),
                "machine_auth_token",
            ),
            jwt_signing_secret: resolve_runtime_secret(
                env::var("HEIWA_JWT_SIGNING_SECRET").ok(),
                env::var("HEIWA_AUTH_SECRET").ok(),
                heiwa_home.as_deref(),
                "jwt_signing_secret",
            ),
            node_id: env::var("HEIWA_NODE_ID").unwrap_or_else(|_| "heiwa-core-0".to_string()),
            model_tiers_seed_path: env::var("MODEL_TIERS_SEED_PATH")
                .unwrap_or_else(|_| "config/seeds/model_tiers.json".to_string()),
            ai_router_seed_path: env::var("AI_ROUTER_SEED_PATH")
                .unwrap_or_else(|_| "config/swarm/ai_router.json".to_string()),
        }
    }
}

/// The runtime root, or `None` when no real home exists.
///
/// Deliberately strict: this feeds `resolve_runtime_secret`, which reads the
/// JWT signing secret and machine auth token off disk. A cwd-relative
/// fallback would let anything able to write the process working directory
/// supply those secrets, so no root means no secret adopted.
fn heiwa_home_from_env() -> Option<PathBuf> {
    heiwa_config::HeiwaPaths::try_resolve().map(|paths| paths.runtime_root)
}

/// Bootstrap only the private desktop-to-runtime credential on a fresh profile.
/// Existing credentials, including malformed files, are never replaced here.
pub fn ensure_desktop_machine_auth() -> std::io::Result<()> {
    if !RuntimeConfig::from_env().machine_auth_token.is_empty() {
        return Ok(());
    }
    let root = heiwa_home_from_env()
        .ok_or_else(|| std::io::Error::other("local runtime root unavailable"))?;
    ensure_machine_auth_in(&root)
}

fn ensure_machine_auth_in(root: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let directory = root.join("secrets");
    fs::create_dir_all(root)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(&directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::other(
            "local secrets directory must be a real directory",
        ));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(std::io::Error::other(
            "local secrets directory must be owner-private",
        ));
    }
    let target = directory.join("machine_auth_token");
    match fs::symlink_metadata(&target) {
        Ok(_) => return validate_existing_machine_auth(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    // Publish a fully written private file without replacing a concurrent
    // launcher's credential. A hard link gives create-if-absent semantics.
    let temporary = directory.join(format!(".machine-auth-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let credential = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        file.write_all(credential.as_bytes())?;
        file.sync_all()?;
        match fs::hard_link(&temporary, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        #[cfg(unix)]
        fs::File::open(&directory)?.sync_all()?;
        validate_existing_machine_auth(root)
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn validate_existing_machine_auth(root: &Path) -> std::io::Result<()> {
    if resolve_runtime_secret(None, None, Some(root), "machine_auth_token").is_empty() {
        Err(std::io::Error::other(
            "existing local machine credential is invalid; it was preserved",
        ))
    } else {
        Ok(())
    }
}

fn resolve_runtime_secret(
    primary: Option<String>,
    legacy: Option<String>,
    heiwa_home: Option<&Path>,
    filename: &str,
) -> String {
    for value in [primary, legacy].into_iter().flatten() {
        if let Some(value) = normalize_runtime_secret(&value) {
            return value;
        }
    }

    let Some(heiwa_home) = heiwa_home else {
        return String::new();
    };
    let secret_path = heiwa_home.join("secrets").join(filename);
    let Ok(link_metadata) = fs::symlink_metadata(&secret_path) else {
        return String::new();
    };
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return String::new();
    }
    if link_metadata.len() == 0 || link_metadata.len() > 4096 {
        return String::new();
    }
    #[cfg(unix)]
    if link_metadata.permissions().mode() & 0o077 != 0 {
        return String::new();
    }

    fs::read_to_string(secret_path)
        .ok()
        .and_then(|value| normalize_runtime_secret(&value))
        .unwrap_or_default()
}

fn normalize_runtime_secret(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 4096
        || !value.is_ascii()
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn desktop_auth_bootstrap_is_private_stable_and_profile_isolated() {
        let first = tempdir().unwrap();
        let other = tempdir().unwrap();
        ensure_machine_auth_in(first.path()).unwrap();
        let token = resolve_runtime_secret(None, None, Some(first.path()), "machine_auth_token");
        assert_eq!(token.len(), 64);
        ensure_machine_auth_in(first.path()).unwrap();
        assert_eq!(
            token,
            resolve_runtime_secret(None, None, Some(first.path()), "machine_auth_token")
        );
        ensure_machine_auth_in(other.path()).unwrap();
        assert_ne!(
            token,
            resolve_runtime_secret(None, None, Some(other.path()), "machine_auth_token")
        );
        assert_eq!(
            fs::read_dir(first.path().join("secrets")).unwrap().count(),
            1
        );
    }

    #[test]
    fn desktop_auth_bootstrap_does_not_replace_invalid_credentials() {
        let root = tempdir().unwrap();
        ensure_machine_auth_in(root.path()).unwrap();
        let target = root.path().join("secrets/machine_auth_token");
        fs::write(&target, "malformed token").unwrap();
        assert!(ensure_machine_auth_in(root.path()).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "malformed token");
    }

    #[cfg(unix)]
    #[test]
    fn desktop_auth_bootstrap_rejects_symlinked_storage() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let elsewhere = tempdir().unwrap();
        symlink(elsewhere.path(), root.path().join("secrets")).unwrap();
        assert!(ensure_machine_auth_in(root.path()).is_err());
        assert!(!elsewhere.path().join("machine_auth_token").exists());
    }

    #[test]
    fn desktop_auth_concurrent_launches_share_one_credential() {
        let root = tempdir().unwrap();
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| ensure_machine_auth_in(root.path())))
                .collect();
            for handle in handles {
                handle.join().unwrap().unwrap();
            }
        });
        assert_eq!(
            fs::read_dir(root.path().join("secrets")).unwrap().count(),
            1
        );
        validate_existing_machine_auth(root.path()).unwrap();
    }

    #[test]
    fn runtime_secret_prefers_primary_then_legacy_environment_values() {
        let root = tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("secrets")).expect("secrets dir");
        std::fs::write(
            root.path().join("secrets/machine_auth_token"),
            "file-token\n",
        )
        .expect("secret file");

        assert_eq!(
            resolve_runtime_secret(
                Some("primary-token".to_string()),
                Some("legacy-token".to_string()),
                Some(root.path()),
                "machine_auth_token",
            ),
            "primary-token"
        );
        assert_eq!(
            resolve_runtime_secret(
                None,
                Some("legacy-token".to_string()),
                Some(root.path()),
                "machine_auth_token",
            ),
            "legacy-token"
        );
    }

    #[test]
    #[cfg(unix)]
    fn runtime_secret_reads_only_owner_private_heiwa_secret_files() {
        let root = tempdir().expect("tempdir");
        let secrets = root.path().join("secrets");
        std::fs::create_dir_all(&secrets).expect("secrets dir");
        let secret = secrets.join("machine_auth_token");
        std::fs::write(&secret, "file-token\n").expect("secret file");
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
            .expect("private permissions");

        assert_eq!(
            resolve_runtime_secret(None, None, Some(root.path()), "machine_auth_token"),
            "file-token"
        );

        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644))
            .expect("insecure permissions");
        assert_eq!(
            resolve_runtime_secret(None, None, Some(root.path()), "machine_auth_token"),
            ""
        );

        std::fs::write(&secret, "token\nheader-injection\n").expect("invalid secret content");
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
            .expect("private permissions");
        assert_eq!(
            resolve_runtime_secret(None, None, Some(root.path()), "machine_auth_token"),
            ""
        );
    }
}
