pub mod evidence;

use std::path::PathBuf;
use std::sync::Arc;

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
    /// no `~/.heiwa/identity.json`, no env vars set, and no connection file.
    pub fn resolve() -> Option<Self> {
        let heiwa_dir = dirs_home().unwrap_or_default();

        // ── 1. Read persisted connection file ───────────────────────────
        let file_cfg = heiwa_dir
            .join("connection.json")
            .exists()
            .then(|| {
                std::fs::read_to_string(heiwa_dir.join("connection.json"))
                    .ok()
                    .and_then(|s| serde_json::from_str::<ConnectionFile>(&s).ok())
            })
            .flatten()
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
        let token = env_token.or(file_cfg.token).unwrap_or_default();

        // ── 4. Gate: any reason to connect? ─────────────────────────────
        let has_identity = heiwa_dir.join("identity.json").exists();
        let has_env = std::env::var("STDB_URL").is_ok()
            || std::env::var("STDB_DATABASE").is_ok()
            || std::env::var("STDB_IDENTITY").is_ok()
            || std::env::var("STDB_TOKEN").is_ok()
            || std::env::var("STDB_AUTH_TOKEN").is_ok()
            || std::env::var("SPACETIMEDB_TOKEN").is_ok();
        let has_connection_file = heiwa_dir.join("connection.json").exists();

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

/// Return `~/.heiwa/` path without depending on heiwa_provider.
fn dirs_home() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".heiwa"))
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
