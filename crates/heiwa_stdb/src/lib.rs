pub mod evidence;

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
// StdbProbe — synchronous reachability snapshot for `heiwa doctor`
// ---------------------------------------------------------------------------

/// Honest snapshot of the configured SpacetimeDB endpoint and its
/// loopback-or-network reachability. Never contains auth secrets — only whether
/// the official `spacetime login show` shell identity or a legacy token is
/// present.
#[derive(Debug, Clone, Serialize)]
pub struct StdbProbe {
    pub configured: bool,
    pub url: Option<String>,
    pub database: Option<String>,
    pub token_present: bool,
    pub auth_mode: Option<String>,
    pub auth_source: Option<String>,
    pub reachable: Option<bool>,
    pub latency_ms: Option<u64>,
}

impl StdbProbe {
    /// Resolve config and attempt a short TCP probe of the host:port from
    /// the URL. When unconfigured, returns a probe with `configured: false`
    /// and no reachability fields populated.
    pub fn probe() -> Self {
        let shell_identity = spacetime_shell_identity();
        let config = StdbConfig::resolve().or_else(|| {
            shell_identity.as_ref().map(|_| StdbConfig {
                url: DEFAULT_URL.to_string(),
                database: DEFAULT_DATABASE.to_string(),
                token: String::new(),
            })
        });
        let Some(config) = config else {
            return Self {
                configured: false,
                url: None,
                database: None,
                token_present: false,
                auth_mode: None,
                auth_source: None,
                reachable: None,
                latency_ms: None,
            };
        };

        let legacy_token_present = !config.token.is_empty();
        let shell_auth_present = shell_identity.is_some();
        let token_present = shell_auth_present || legacy_token_present;
        let (auth_mode, auth_source) = if shell_auth_present {
            (
                Some("spacetime_cli_login".to_string()),
                Some("spacetime login show".to_string()),
            )
        } else if legacy_token_present {
            (
                Some("legacy_token".to_string()),
                Some("STDB_TOKEN_or_connection_json".to_string()),
            )
        } else {
            (None, None)
        };
        let endpoint = parse_endpoint(&config.url);
        let (reachable, latency_ms) = match endpoint {
            Some(addr) => {
                let start = Instant::now();
                let ok = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok();
                let latency = ok.then(|| start.elapsed().as_millis() as u64);
                (Some(ok), latency)
            }
            None => (Some(false), None),
        };

        Self {
            configured: true,
            url: Some(config.url),
            database: Some(config.database),
            token_present,
            auth_mode,
            auth_source,
            reachable,
            latency_ms,
        }
    }
}

/// True when the official SpacetimeDB CLI has an authenticated shell identity.
/// This is Heiwa's preferred Maincloud publisher/operator auth probe: no raw API
/// token is required or serialized.
pub fn spacetime_shell_auth_present() -> bool {
    spacetime_shell_identity().is_some()
}

/// Return the `spacetime login show` identity line if the CLI is authenticated.
/// The output is identity-only status text, never a token.
pub fn spacetime_shell_identity() -> Option<String> {
    let bin = std::env::var("HEIWA_SPACETIME_BIN").unwrap_or_else(|_| "spacetime".to_string());
    spacetime_shell_identity_with_bin(&bin)
}

fn spacetime_shell_identity_with_bin(bin: &str) -> Option<String> {
    let output = Command::new(bin).args(["login", "show"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Parse `scheme://host[:port][/...]` into a resolvable `SocketAddr`.
/// Returns `None` on parse failure or DNS resolution failure.
fn parse_endpoint(url: &str) -> Option<SocketAddr> {
    let (scheme, rest) = url.split_once("://")?;
    let default_port = match scheme {
        "https" | "wss" => 443,
        "http" | "ws" => 80,
        _ => return None,
    };

    let authority = rest.split('/').next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (authority, default_port),
    };
    if host.is_empty() {
        return None;
    }

    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut iter| iter.next())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_endpoint_handles_https_default_port() {
        let addr = parse_endpoint("https://maincloud.spacetimedb.com").expect("addr");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_endpoint_handles_explicit_port() {
        let addr = parse_endpoint("http://localhost:3000/heiwa").expect("addr");
        assert_eq!(addr.port(), 3000);
    }

    #[test]
    fn parse_endpoint_handles_ws_scheme() {
        let addr = parse_endpoint("wss://maincloud.spacetimedb.com/db").expect("addr");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_endpoint_rejects_unknown_scheme() {
        assert!(parse_endpoint("ftp://example.com").is_none());
        assert!(parse_endpoint("noscheme").is_none());
    }

    #[test]
    fn stdb_probe_when_unconfigured_returns_inert_snapshot() {
        // We can't fully control the test env (CI may have ~/.heiwa from prior
        // jobs) but we can assert the shape contract: auth material must never leak.
        let probe = StdbProbe::probe();
        // The probe is always Serialize-safe and never holds a raw token.
        let json = serde_json::to_string(&probe).expect("serialize");
        assert!(
            !json.contains("\"token\":"),
            "stdb probe must never serialize a raw token field: {json}"
        );
    }

    // The fake `spacetime` is a shebang shell script — unix-only by nature.
    #[cfg(unix)]
    #[test]
    fn spacetime_shell_identity_uses_login_show_without_token_material() {
        let dir = std::env::temp_dir().join(format!("heiwa-fake-spacetime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fake spacetime dir");
        let bin = dir.join("spacetime");
        std::fs::write(
            &bin,
            "#!/usr/bin/env sh\nif [ \"$1 $2\" = \"login show\" ]; then echo 'You are logged in as c200abc'; exit 0; fi\nexit 1\n",
        )
        .expect("write fake spacetime");
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).expect("chmod fake spacetime");
        }

        // Retry: a concurrent test in this harness can fork while the script's
        // write fd is briefly held, making the first exec fail with ETXTBSY.
        let mut identity = None;
        for _ in 0..50 {
            identity = spacetime_shell_identity_with_bin(bin.to_str().expect("utf8 path"));
            if identity.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let identity = identity.expect("fake spacetime login show should authenticate");
        assert_eq!(identity, "You are logged in as c200abc");
        assert!(!identity.contains("token"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
