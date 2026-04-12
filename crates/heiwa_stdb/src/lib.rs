pub mod evidence;

use std::sync::Arc;

use heiwa_paths::RuntimePaths;
use heiwa_bindings::DbConnection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// StdbConfig — resolved connection parameters
// ---------------------------------------------------------------------------

const DEFAULT_URL: &str = "https://maincloud.spacetimedb.com";
const DEFAULT_DATABASE: &str = "heiwaproductiondb";

/// Persisted connection config written by `heiwa login`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdbConfig {
    pub url: String,
    pub database: String,
    pub token: String,
}

/// On-disk shape of `~/.heiwa/connection.json`.
#[derive(Deserialize, Default)]
struct ConnectionFile {
    url: Option<String>,
    database: Option<String>,
    token: Option<String>,
}

impl StdbConfig {
    /// Resolve connection config from file, env vars, and defaults.
    ///
    /// Returns `None` only when there is genuinely no reason to connect:
    /// no structured or legacy Heiwa identity, no env vars set, and no connection file.
    pub fn resolve() -> Option<Self> {
        let paths = RuntimePaths::discover();
        let heiwa_dir = paths.root().to_path_buf();
        let structured_connection = paths.connection();
        let legacy_connection = heiwa_dir.join("connection.json");
        let structured_identity = paths.identity();
        let legacy_identity = heiwa_dir.join("identity.json");

        // ── 1. Read persisted connection file ───────────────────────────
        let file_cfg = [structured_connection.as_path(), legacy_connection.as_path()]
            .into_iter()
            .find_map(|path| {
                path.exists().then(|| {
                    std::fs::read_to_string(path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<ConnectionFile>(&s).ok())
                })?
            })
            .unwrap_or_default();

        // ── 2. Env-var overrides ────────────────────────────────────────
        let env_url = std::env::var("STDB_URL").ok();
        let env_database = std::env::var("STDB_DATABASE")
            .or_else(|_| std::env::var("STDB_IDENTITY"))
            .ok();
        let env_token = std::env::var("STDB_TOKEN")
            .or_else(|_| std::env::var("STDB_AUTH_TOKEN"))
            .or_else(|_| std::env::var("SPACETIMEDB_TOKEN"))
            .ok();

        // ── 3. Merge: env > file > default ──────────────────────────────
        let url = env_url
            .or(file_cfg.url)
            .unwrap_or_else(|| DEFAULT_URL.to_string());
        let database = env_database
            .or(file_cfg.database)
            .unwrap_or_else(|| DEFAULT_DATABASE.to_string());
        let token = env_token
            .or(file_cfg.token)
            .unwrap_or_default();

        // ── 4. Gate: any reason to connect? ─────────────────────────────
        let has_identity = structured_identity.exists() || legacy_identity.exists();
        let has_env = std::env::var("STDB_URL").is_ok()
            || std::env::var("STDB_DATABASE").is_ok()
            || std::env::var("STDB_IDENTITY").is_ok()
            || std::env::var("STDB_TOKEN").is_ok()
            || std::env::var("STDB_AUTH_TOKEN").is_ok()
            || std::env::var("SPACETIMEDB_TOKEN").is_ok();
        let has_connection_file = structured_connection.exists() || legacy_connection.exists();

        if !has_identity && !has_env && !has_connection_file {
            return None;
        }

        Some(StdbConfig {
            url,
            database,
            token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::StdbConfig;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn with_temp_home<T>(f: impl FnOnce(&PathBuf) -> T) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner());

        let tmp = std::env::temp_dir().join(format!(
            "heiwa-stdb-runtime-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).expect("create temp home");

        let original_home = env::var_os("HOME");
        let original_stdb_url = env::var_os("STDB_URL");
        let original_stdb_database = env::var_os("STDB_DATABASE");
        let original_stdb_identity = env::var_os("STDB_IDENTITY");
        let original_stdb_token = env::var_os("STDB_TOKEN");
        let original_stdb_auth_token = env::var_os("STDB_AUTH_TOKEN");
        let original_spacetimedb_token = env::var_os("SPACETIMEDB_TOKEN");

        env::set_var("HOME", &tmp);
        env::remove_var("STDB_URL");
        env::remove_var("STDB_DATABASE");
        env::remove_var("STDB_IDENTITY");
        env::remove_var("STDB_TOKEN");
        env::remove_var("STDB_AUTH_TOKEN");
        env::remove_var("SPACETIMEDB_TOKEN");

        let result = f(&tmp);

        match original_home {
            Some(v) => env::set_var("HOME", v),
            None => env::remove_var("HOME"),
        }
        match original_stdb_url {
            Some(v) => env::set_var("STDB_URL", v),
            None => env::remove_var("STDB_URL"),
        }
        match original_stdb_database {
            Some(v) => env::set_var("STDB_DATABASE", v),
            None => env::remove_var("STDB_DATABASE"),
        }
        match original_stdb_identity {
            Some(v) => env::set_var("STDB_IDENTITY", v),
            None => env::remove_var("STDB_IDENTITY"),
        }
        match original_stdb_token {
            Some(v) => env::set_var("STDB_TOKEN", v),
            None => env::remove_var("STDB_TOKEN"),
        }
        match original_stdb_auth_token {
            Some(v) => env::set_var("STDB_AUTH_TOKEN", v),
            None => env::remove_var("STDB_AUTH_TOKEN"),
        }
        match original_spacetimedb_token {
            Some(v) => env::set_var("SPACETIMEDB_TOKEN", v),
            None => env::remove_var("SPACETIMEDB_TOKEN"),
        }

        let _ = fs::remove_dir_all(&tmp);
        result
    }

    #[test]
    fn resolve_prefers_structured_state_files() {
        with_temp_home(|home| {
            let runtime_root = home.join(".heiwa");
            fs::create_dir_all(runtime_root.join("state")).expect("create state dir");
            fs::write(
                runtime_root.join("state/identity.json"),
                "{\n  \"user_id\": \"devon\"\n}\n",
            )
            .expect("write structured identity");
            fs::write(
                runtime_root.join("state/connection.json"),
                "{\n  \"url\": \"https://structured.example\",\n  \"database\": \"structureddb\",\n  \"token\": \"abc\"\n}\n",
            )
            .expect("write structured connection");

            let cfg = StdbConfig::resolve().expect("resolve structured config");
            assert_eq!(cfg.url, "https://structured.example");
            assert_eq!(cfg.database, "structureddb");
            assert_eq!(cfg.token, "abc");
        });
    }

    #[test]
    fn resolve_falls_back_to_legacy_flat_files() {
        with_temp_home(|home| {
            let runtime_root = home.join(".heiwa");
            fs::create_dir_all(&runtime_root).expect("create runtime root");
            fs::write(
                runtime_root.join("identity.json"),
                "{\n  \"user_id\": \"devon\"\n}\n",
            )
            .expect("write legacy identity");
            fs::write(
                runtime_root.join("connection.json"),
                "{\n  \"url\": \"https://legacy.example\",\n  \"database\": \"legacydb\",\n  \"token\": \"xyz\"\n}\n",
            )
            .expect("write legacy connection");

            let cfg = StdbConfig::resolve().expect("resolve legacy config");
            assert_eq!(cfg.url, "https://legacy.example");
            assert_eq!(cfg.database, "legacydb");
            assert_eq!(cfg.token, "xyz");
        });
    }
}

// ---------------------------------------------------------------------------
// StdbClient — thin wrapper around an optional DbConnection
// ---------------------------------------------------------------------------

/// Thin, Clone-able handle to the SpacetimeDB connection.
///
/// Wraps `Option<Arc<DbConnection>>` so callers can degrade gracefully
/// when offline.
#[derive(Clone)]
pub struct StdbClient {
    conn: Option<Arc<DbConnection>>,
}

impl StdbClient {
    /// Create a disconnected (offline) client.
    pub fn offline() -> Self {
        Self { conn: None }
    }

    /// Try to connect using the given config. On failure, log and return
    /// an offline client instead of propagating the error.
    pub fn connect(config: &StdbConfig) -> Self {
        info!(
            url = %config.url,
            database = %config.database,
            "Connecting to SpacetimeDB"
        );

        let token = if config.token.is_empty() {
            None
        } else {
            Some(config.token.as_str())
        };

        match DbConnection::builder()
            .with_uri(&config.url)
            .with_database_name(&config.database)
            .with_token(token)
            .build()
        {
            Ok(conn) => {
                info!("SpacetimeDB connection established");
                Self {
                    conn: Some(Arc::new(conn)),
                }
            }
            Err(e) => {
                warn!("SpacetimeDB connection failed, running offline: {e}");
                Self::offline()
            }
        }
    }

    /// Access the raw `DbConnection`, if connected.
    pub fn connection(&self) -> Option<&Arc<DbConnection>> {
        self.conn.as_ref()
    }

    /// Whether this client holds a live connection.
    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Spawn a background tokio task that continuously advances the STDB
    /// message loop. Without this, no subscription updates or reducer
    /// callbacks will fire.
    pub fn spawn_advance_loop(&self) {
        let Some(conn) = self.conn.clone() else {
            return;
        };

        tokio::spawn(async move {
            loop {
                if let Err(e) = conn.advance_one_message_async().await {
                    warn!("STDB advance error: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        });
    }
}
