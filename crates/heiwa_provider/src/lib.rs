use heiwa_paths::RuntimePaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub mod adapter;
pub mod detect;
pub mod keychain;
pub mod providers;
pub mod registry;

// ---------------------------------------------------------------------------
// Re-exports for convenience
// ---------------------------------------------------------------------------

pub use registry::{
    AccountRegistry, AccountStatus, Credential, DetectedModel, InventoryTruth, ProviderAccount,
};

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

fn get_heiwa_state_dir() -> PathBuf {
    RuntimePaths::discover().root().to_path_buf()
}

fn get_legacy_identity_path() -> PathBuf {
    get_heiwa_state_dir().join("identity.json")
}

pub fn get_identity_path() -> std::path::PathBuf {
    RuntimePaths::discover().identity()
}

pub fn load_identity() -> Option<HeiwaIdentity> {
    for path in [get_identity_path(), get_legacy_identity_path()] {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(path).ok()?;
        if let Ok(identity) = serde_json::from_str(&content) {
            return Some(identity);
        }
    }
    None
}

pub fn save_identity(identity: &HeiwaIdentity) -> anyhow::Result<()> {
    let path = get_identity_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(identity)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn clear_identity() -> anyhow::Result<()> {
    for path in [get_identity_path(), get_legacy_identity_path()] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn login_heiwa(token: &str) -> anyhow::Result<HeiwaIdentity> {
    // In a real implementation, this would verify the token against Heiwa Hub
    let identity = HeiwaIdentity {
        user_id: "devon-canonical".to_string(),
        auth_token: token.to_string(),
        email: Some("devon@heiwa.ltd".to_string()),
        display_name: Some("Devon".to_string()),
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
    RuntimePaths::discover().legacy_connections()
}

fn get_legacy_provider_connections_path() -> PathBuf {
    get_heiwa_state_dir().join("provider_connections.json")
}

fn load_provider_connections() -> Vec<String> {
    for path in [get_provider_connections_path(), get_legacy_provider_connections_path()] {
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(path).ok();
        let parsed = content
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .unwrap_or_default();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    Vec::new()
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
                if connected { "connected".to_string() } else { "installed_unverified".to_string() }
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
                if connected { "connected".to_string() } else { "installed_unverified".to_string() }
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
                if connected { "connected".to_string() } else { "installed_unverified".to_string() }
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
                if connected { "connected".to_string() } else { "installed_unverified".to_string() }
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
            Command::new("claude").arg("auth").arg("login").status()?;
            mark_provider_connected(provider_id)?;
        }
        "codex" => {
            Command::new("codex").arg("login").status()?;
            mark_provider_connected(provider_id)?;
        }
        "gemini" => {
            println!("Gemini CLI does not expose a stable non-interactive auth subcommand here. Authenticate in Gemini, then rerun Heiwa once connected.");
        }
        "antigravity" => {
            println!("Antigravity auth remains provider-owned. Connect Antigravity in its own surface, then let Heiwa wrap the installed runtime.");
        }
        _ => {
            println!("No automated login flow for {}. Please login manually.", provider_id);
        }
    }
    Ok(())
}

pub fn logout(provider_id: &str) -> anyhow::Result<()> {
    println!("Logging out from {}...", provider_id);
    match provider_id {
        "claude" => {
            Command::new("claude").arg("auth").arg("logout").status()?;
            clear_provider_connection(provider_id)?;
        }
        "codex" => {
            Command::new("codex").arg("logout").status()?;
            clear_provider_connection(provider_id)?;
        }
        "gemini" | "antigravity" => {
            clear_provider_connection(provider_id)?;
            println!("Cleared Heiwa's local connection record for {}. Provider-owned auth may still need to be disconnected in the provider surface.", provider_id);
        }
        _ => {
            println!("No automated logout flow for {}. Please logout manually.", provider_id);
        }
    }
    Ok(())
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn is_ollama_running() -> bool {
    std::net::TcpStream::connect("127.0.0.1:11434").is_ok()
}
