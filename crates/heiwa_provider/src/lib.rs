use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod adapter;
pub mod detect;
pub mod health;
pub mod keychain;
pub mod oauth;
pub mod providers;
pub mod registry;
pub mod routing;

// ---------------------------------------------------------------------------
// Re-exports for convenience
// ---------------------------------------------------------------------------

pub use oauth::{needs_refresh, OAuthBridgeError, ProviderVault, OAUTH_SERVICE};
pub use registry::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};

// Re-export the OAuth payload type so callers don't need a direct dep on heiwa-vault.
pub use heiwa_vault::OAuthSecret;

// ---------------------------------------------------------------------------
// Heiwa identity (account plane — separate from provider accounts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeiwaIdentity {
    pub user_id: String,
    pub auth_token: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// The runtime root, or `None` when no real per-user root exists.
///
/// Strict rather than lenient: this backs the identity file, which holds an
/// auth token, and the account registry. The lenient resolver falls back to
/// `./.heiwa`, which would write credentials into whatever directory the
/// process happened to start in.
fn try_heiwa_state_dir() -> Option<PathBuf> {
    heiwa_config::HeiwaPaths::try_resolve().map(|paths| paths.runtime_root)
}

fn get_heiwa_state_dir() -> PathBuf {
    try_heiwa_state_dir().unwrap_or_else(|| heiwa_config::HeiwaPaths::resolve().runtime_root)
}

pub fn get_identity_path() -> std::path::PathBuf {
    identity_path_in(get_heiwa_state_dir())
}

fn identity_path_in(root: PathBuf) -> PathBuf {
    root.join("identity.json")
}

/// The identity file, or `None` when there is no per-user root to hold it.
///
/// Callers that write the token use this: refusing is better than writing an
/// auth token into the current working directory.
pub fn try_identity_path() -> Option<PathBuf> {
    try_heiwa_state_dir().map(identity_path_in)
}

pub fn load_identity() -> Option<HeiwaIdentity> {
    let path = get_identity_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_identity(identity: &HeiwaIdentity) -> anyhow::Result<()> {
    let path = try_identity_path().ok_or_else(|| {
        anyhow::anyhow!(
            "no Heiwa state root: set HEIWA_HOME or HOME before storing an identity token"
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(identity)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn clear_identity() -> anyhow::Result<()> {
    let path = get_identity_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn login_heiwa(token: &str) -> anyhow::Result<HeiwaIdentity> {
    // Local per-install identity stamp. Verification against a hosted
    // account plane is L2 work; until then no personal identity is invented
    // here — the id is neutral and profile fields stay empty.
    let identity = HeiwaIdentity {
        user_id: "local-user".to_string(),
        auth_token: token.to_string(),
        email: None,
        display_name: None,
    };
    save_identity(&identity)?;
    Ok(identity)
}

// ---------------------------------------------------------------------------
// Legacy provider auth status — kept for backward compat during migration
//
// The new path is: AccountRegistry + detect::* for model inventory.
// These functions will be replaced as main.rs migrates to the new registry.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    OauthCli,
    ApiKey,
    RouterApi,
    LocalRuntime,
    CustomProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LegacyProviderAccount {
    pub provider_id: String,
    pub account_id: String,
    pub auth_kind: AuthKind,
    pub status: String,
    pub rate_group: String,
    pub default_model: Option<String>,
    pub device_binding: Option<String>,
}

fn get_provider_connections_path() -> PathBuf {
    get_heiwa_state_dir().join("provider_connections.json")
}

fn load_provider_connections() -> Vec<String> {
    let path = get_provider_connections_path();
    if !path.exists() {
        return Vec::new();
    }
    let content = fs::read_to_string(path).ok();
    content
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

fn save_provider_connections(connections: &[String]) -> anyhow::Result<()> {
    let path = get_provider_connections_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(connections)?)?;
    Ok(())
}

fn mark_provider_connected(provider_id: &str) -> anyhow::Result<()> {
    let mut connections = load_provider_connections();
    if !connections.iter().any(|p| p == provider_id) {
        connections.push(provider_id.to_string());
    }
    save_provider_connections(&connections)
}

fn clear_provider_connection(provider_id: &str) -> anyhow::Result<()> {
    let mut connections = load_provider_connections();
    connections.retain(|p| p != provider_id);
    save_provider_connections(&connections)
}

fn provider_is_connected(provider_id: &str) -> bool {
    load_provider_connections().iter().any(|p| p == provider_id)
}

pub fn get_auth_status(provider_id: &str) -> Option<LegacyProviderAccount> {
    let connected = provider_is_connected(provider_id);
    match provider_id {
        "claude" => {
            let status = if has_command("claude") {
                if connected || claude_has_native_auth() {
                    "connected".to_string()
                } else {
                    "installed_unverified".to_string()
                }
            } else {
                "not_installed".to_string()
            };
            Some(LegacyProviderAccount {
                provider_id: "claude".to_string(),
                account_id: "claude-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status,
                rate_group: "anthropic".to_string(),
                default_model: None,
                device_binding: None,
            })
        }
        "ollama" => {
            let status = if is_ollama_running() {
                "running".to_string()
            } else if has_command("ollama") {
                "installed_stopped".to_string()
            } else {
                "not_installed".to_string()
            };
            Some(LegacyProviderAccount {
                provider_id: "ollama".to_string(),
                account_id: "local".to_string(),
                auth_kind: AuthKind::LocalRuntime,
                status,
                rate_group: "local".to_string(),
                default_model: None,
                device_binding: None,
            })
        }
        "codex" => {
            let status = if has_command("codex") {
                if connected || codex_has_native_auth() {
                    "connected".to_string()
                } else {
                    "installed_unverified".to_string()
                }
            } else {
                "not_installed".to_string()
            };
            Some(LegacyProviderAccount {
                provider_id: "codex".to_string(),
                account_id: "codex-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status,
                rate_group: "openai".to_string(),
                default_model: None,
                device_binding: None,
            })
        }
        "gemini" => {
            let status = if has_command("gemini") {
                if connected || gemini_has_native_auth() {
                    "connected".to_string()
                } else {
                    "installed_unverified".to_string()
                }
            } else {
                "not_installed".to_string()
            };
            Some(LegacyProviderAccount {
                provider_id: "gemini".to_string(),
                account_id: "gemini-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status,
                rate_group: "google".to_string(),
                default_model: None,
                device_binding: None,
            })
        }
        "antigravity" => {
            let status = if has_command("antigravity") {
                if connected || antigravity_has_native_auth() {
                    "connected".to_string()
                } else {
                    "installed_unverified".to_string()
                }
            } else {
                "not_installed".to_string()
            };
            Some(LegacyProviderAccount {
                provider_id: "antigravity".to_string(),
                account_id: "antigravity-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status,
                rate_group: "google_bonus".to_string(),
                default_model: None,
                device_binding: None,
            })
        }
        _ => None,
    }
}

pub fn login(provider_id: &str) -> anyhow::Result<()> {
    println!("Initiating real login for {}...", provider_id);
    match provider_id {
        "claude" => {
            Command::new(resolve_command_or_name("claude"))
                .arg("auth")
                .arg("login")
                .status()?;
            mark_provider_connected(provider_id)?;
        }
        "codex" => {
            Command::new(resolve_command_or_name("codex"))
                .arg("login")
                .status()?;
            mark_provider_connected(provider_id)?;
        }
        "gemini" => {
            println!("Gemini CLI does not expose a stable non-interactive auth subcommand here. Authenticate in Gemini, then rerun Heiwa once connected.");
        }
        "antigravity" => {
            println!("Antigravity auth remains provider-owned. Connect Antigravity in its own surface, then let Heiwa wrap the installed runtime.");
        }
        _ => {
            println!(
                "No automated login flow for {}. Please login manually.",
                provider_id
            );
        }
    }
    Ok(())
}

pub fn logout(provider_id: &str) -> anyhow::Result<()> {
    println!("Logging out from {}...", provider_id);
    match provider_id {
        "claude" => {
            Command::new(resolve_command_or_name("claude"))
                .arg("auth")
                .arg("logout")
                .status()?;
            clear_provider_connection(provider_id)?;
        }
        "codex" => {
            Command::new(resolve_command_or_name("codex"))
                .arg("logout")
                .status()?;
            clear_provider_connection(provider_id)?;
        }
        "gemini" | "antigravity" => {
            clear_provider_connection(provider_id)?;
            println!("Cleared Heiwa's local connection record for {}. Provider-owned auth may still need to be disconnected in the provider surface.", provider_id);
        }
        _ => {
            println!(
                "No automated logout flow for {}. Please logout manually.",
                provider_id
            );
        }
    }
    Ok(())
}

/// System-wide directories probed for a provider CLI beyond `PATH`.
///
/// Package managers install into these whether or not they are on the
/// invoking shell's `PATH`, so discovery would miss real installs without
/// them. Exposed so a caller that must model a machine with nothing
/// installed — the fresh-install harness — can pass an empty set instead.
pub const DEFAULT_SYSTEM_BIN_DIRS: &[&str] =
    &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];

/// Directories probed for a provider CLI, in priority order. `runtime_root`
/// comes from ConfigRoot; this function stays pure so the ordering can be
/// tested without touching process-global environment.
#[cfg(test)]
fn provider_search_paths(home: &Path, runtime_root: &Path) -> Vec<PathBuf> {
    provider_search_paths_with_system_dirs(home, runtime_root, DEFAULT_SYSTEM_BIN_DIRS)
}

fn provider_search_paths_with_system_dirs(
    home: &Path,
    runtime_root: &Path,
    system_dirs: &[&str],
) -> Vec<PathBuf> {
    let mut dirs = vec![
        runtime_root.join("bin"),
        home.join(".local").join("bin"),
        home.join(".npm-global").join("bin"),
        home.join(".cargo").join("bin"),
    ];
    dirs.extend(system_dirs.iter().map(PathBuf::from));
    dirs
}

/// Resolve a command against explicit inputs, including which system
/// directories to consider installed.
///
/// [`resolve_command`] is the production entry point and passes the real
/// home, runtime root, and [`DEFAULT_SYSTEM_BIN_DIRS`]. Pass an empty
/// `system_dirs` slice to model a machine on which no provider CLI is
/// installed anywhere Heiwa looks.
///
/// Every input is explicit — including the runtime root — so a caller that
/// injects a sandbox home cannot accidentally probe, and execute, binaries
/// out of the real user's runtime.
pub fn resolve_command_in(
    cmd: &str,
    home: &Path,
    runtime_root: &Path,
    path: &str,
    system_dirs: &[&str],
) -> Option<PathBuf> {
    #[cfg(windows)]
    const EXTENSIONS: &[&str] = &["", ".exe", ".cmd", ".bat", ".com"];
    #[cfg(not(windows))]
    const EXTENSIONS: &[&str] = &[""];

    let mut dirs = env::split_paths(path).collect::<Vec<_>>();
    dirs.extend(provider_search_paths_with_system_dirs(
        home,
        runtime_root,
        system_dirs,
    ));
    for dir in dirs {
        for extension in EXTENSIONS {
            let candidate = dir.join(format!("{cmd}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn resolve_command(cmd: &str) -> Option<PathBuf> {
    let paths = heiwa_config::HeiwaPaths::resolve();
    let path = env::var("PATH").unwrap_or_default();
    resolve_command_in(
        cmd,
        &paths.home_dir,
        &paths.runtime_root,
        &path,
        DEFAULT_SYSTEM_BIN_DIRS,
    )
}

pub fn resolve_command_or_name(cmd: &str) -> PathBuf {
    resolve_command(cmd).unwrap_or_else(|| PathBuf::from(cmd))
}

fn has_command(cmd: &str) -> bool {
    resolve_command(cmd).is_some()
}

fn is_ollama_running() -> bool {
    let registry = AccountRegistry::load();
    let stored_endpoint = crate::detect::ollama::registered_endpoint(&registry.accounts);
    let Ok(endpoint) = crate::detect::ollama::resolve_configured_endpoint(stored_endpoint) else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(&endpoint.api_url("/api/tags")) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let Some(port) = url.port_or_known_default() else {
        return false;
    };
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    let host_header = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let request =
        format!("GET /api/tags HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\n\r\n");

    addresses.into_iter().any(|address| {
        let Ok(mut stream) =
            TcpStream::connect_timeout(&address, crate::detect::ollama::ENDPOINT_CONNECT_TIMEOUT)
        else {
            return false;
        };
        if stream
            .set_read_timeout(Some(crate::detect::ollama::ENDPOINT_CONNECT_TIMEOUT))
            .is_err()
            || stream
                .set_write_timeout(Some(crate::detect::ollama::ENDPOINT_CONNECT_TIMEOUT))
                .is_err()
            || stream.write_all(request.as_bytes()).is_err()
        {
            return false;
        }
        let mut status_line = String::new();
        let mut reader = BufReader::new(stream).take(128);
        if reader.read_line(&mut status_line).is_err() {
            return false;
        }
        status_line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse::<u16>().ok())
            .is_some_and(|status| (200..300).contains(&status))
    })
}

fn gemini_has_native_auth() -> bool {
    let home = heiwa_config::HeiwaPaths::resolve().home_dir;
    home.join(".gemini").join("oauth_creds.json").exists()
}

fn codex_has_native_auth() -> bool {
    let home = heiwa_config::HeiwaPaths::resolve().home_dir;
    home.join(".codex").join("auth.json").exists()
}

fn claude_has_native_auth() -> bool {
    let home = heiwa_config::HeiwaPaths::resolve().home_dir;
    let claude_dir = home.join(".claude");
    // settings.json is written only after the OAuth flow completes.
    claude_dir.join("settings.json").exists()
}

fn antigravity_has_native_auth() -> bool {
    let home = heiwa_config::HeiwaPaths::resolve().home_dir;
    let gemini_oauth = home.join(".gemini").join("oauth_creds.json");
    let ag_initialized = home.join(".antigravity").join("argv.json");
    gemini_oauth.exists() && ag_initialized.exists()
}

#[cfg(test)]
mod command_resolution_tests {
    use super::*;

    #[test]
    fn provider_search_paths_include_user_local_bins() {
        let home = PathBuf::from("/Users/example");
        let runtime_root = home.join(".heiwa");
        let paths = provider_search_paths(&home, &runtime_root);

        assert!(paths.contains(&runtime_root.join("bin")));
        assert!(paths.contains(&home.join(".local").join("bin")));
        assert!(paths.contains(&home.join(".npm-global").join("bin")));
        assert!(paths.contains(&home.join(".cargo").join("bin")));
    }

    #[test]
    fn provider_search_paths_honor_explicit_runtime_root() {
        let home = PathBuf::from("/Users/example");
        let runtime_root = PathBuf::from("/opt/heiwa-runtime");
        let paths = provider_search_paths(&home, &runtime_root);

        assert!(paths.contains(&runtime_root.join("bin")));
        assert!(!paths.contains(&home.join(".heiwa").join("bin")));
    }

    #[test]
    fn resolve_command_prefers_path_then_known_user_bins() {
        let temp = env::temp_dir().join(format!(
            "heiwa-provider-command-resolution-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        let bin = temp.join(".npm-global").join("bin");
        fs::create_dir_all(&bin).expect("create bin");
        let exe = bin.join("codex");
        fs::write(&exe, "#!/bin/sh\n").expect("write fake codex");

        let resolved = resolve_command_in("codex", &temp, &temp.join(".heiwa"), "", &[]);

        assert_eq!(resolved.as_deref(), Some(exe.as_path()));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn resolve_command_accepts_windows_command_shims() {
        let temp = env::temp_dir().join(format!(
            "heiwa-provider-windows-command-resolution-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        let bin = temp.join("bin");
        fs::create_dir_all(&bin).expect("create bin");
        let shim = bin.join("gemini.cmd");
        fs::write(&shim, "@exit /b 0\r\n").expect("write fake gemini shim");
        let path = env::join_paths([&bin])
            .expect("join test PATH")
            .to_string_lossy()
            .into_owned();

        // On Windows the resolver appends shim extensions; elsewhere the
        // bare name is the only candidate, so the shim is not a match.
        let resolved = resolve_command_in("gemini", &temp, &temp.join(".heiwa"), &path, &[]);

        #[cfg(windows)]
        assert_eq!(resolved.as_deref(), Some(shim.as_path()));
        #[cfg(not(windows))]
        assert!(resolved.is_none(), "got {resolved:?}");
        let _ = &shim;
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn resolve_command_or_name_falls_back_to_command_name() {
        let resolved = resolve_command_in(
            "missing-heiwa-cli",
            Path::new("/nope"),
            Path::new("/nope/.heiwa"),
            "",
            &[],
        )
        .unwrap_or_else(|| PathBuf::from("missing-heiwa-cli"));

        assert_eq!(resolved, PathBuf::from("missing-heiwa-cli"));
    }
}
