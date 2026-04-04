use serde::{Deserialize, Serialize};
use std::process::Command;

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
pub struct ProviderAccount {
    pub provider_id: String,
    pub account_id: String,
    pub auth_kind: AuthKind,
    pub status: String,
    pub rate_group: String,
    pub default_model: Option<String>,
    pub device_binding: Option<String>,
}

pub fn get_auth_status(provider_id: &str) -> Option<ProviderAccount> {
    match provider_id {
        "claude" => {
            let is_installed = has_command("claude");
            Some(ProviderAccount {
                provider_id: "claude".to_string(),
                account_id: "claude-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status: if is_installed { "authenticated".to_string() } else { "not_installed".to_string() },
                rate_group: "standard".to_string(),
                default_model: Some("claude-3-5-sonnet".to_string()),
                device_binding: None,
            })
        }
        "codex" => {
            let is_installed = has_command("codex");
            Some(ProviderAccount {
                provider_id: "codex".to_string(),
                account_id: "codex-default".to_string(),
                auth_kind: AuthKind::ApiKey,
                status: if is_installed { "authenticated".to_string() } else { "not_installed".to_string() },
                rate_group: "standard".to_string(),
                default_model: Some("gpt-4o".to_string()),
                device_binding: None,
            })
        }
        "gemini" => {
            let is_installed = has_command("gemini");
            Some(ProviderAccount {
                provider_id: "gemini".to_string(),
                account_id: "gemini-default".to_string(),
                auth_kind: AuthKind::OauthCli,
                status: if is_installed { "authenticated".to_string() } else { "not_installed".to_string() },
                rate_group: "standard".to_string(),
                default_model: Some("gemini-1.5-pro".to_string()),
                device_binding: None,
            })
        }
        "ollama" => {
            let is_installed = has_command("ollama");
            Some(ProviderAccount {
                provider_id: "ollama".to_string(),
                account_id: "local".to_string(),
                auth_kind: AuthKind::LocalRuntime,
                status: if is_installed { "running".to_string() } else { "not_installed".to_string() },
                rate_group: "local".to_string(),
                default_model: Some("llama3".to_string()),
                device_binding: None,
            })
        }
        _ => None,
    }
}

pub fn login(provider_id: &str) -> anyhow::Result<()> {
    println!("Initiating login for {}...", provider_id);
    // In a real implementation, this would call the provider's CLI login command
    Ok(())
}

pub fn logout(provider_id: &str) -> anyhow::Result<()> {
    println!("Logging out from {}...", provider_id);
    // In a real implementation, this would call the provider's CLI logout command
    Ok(())
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
