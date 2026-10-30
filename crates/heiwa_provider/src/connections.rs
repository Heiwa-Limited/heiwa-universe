//! User-initiated connection setup shared by native clients and the CLI.
//! Credentials enter the OS vault; only account metadata reaches the UI.
use crate::registry::{AccountRegistry, AccountStatus, Credential, ProviderAccount};
use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConnectionSummary {
    pub account_id: String,
    pub provider: String,
    pub channel: &'static str,
    pub status: &'static str,
    pub model_count: usize,
    pub can_manage_key: bool,
}

pub fn summaries(registry: &AccountRegistry) -> Vec<ConnectionSummary> {
    registry
        .accounts
        .iter()
        .map(|account| ConnectionSummary {
            account_id: account.account_id.clone(),
            provider: account.provider.clone(),
            channel: match account.credential {
                Credential::ApiKey => "API key",
                Credential::OauthCli { .. } => "Provider CLI",
                Credential::OAuth { .. } => "Provider account",
                Credential::LocalRuntime { .. } => "Local runtime",
            },
            status: match account.status {
                AccountStatus::Connected => "connected",
                AccountStatus::Disconnected => "disconnected",
                AccountStatus::NeedsAuth => "needs_verification",
                AccountStatus::Error(_) => "verification_failed",
            },
            model_count: account.models.len(),
            can_manage_key: matches!(account.credential, Credential::ApiKey),
        })
        .collect()
}

fn rate_group(provider: &str) -> Result<&'static str> {
    match provider {
        "openai" => Ok("openai_api"),
        "anthropic" => Ok("anthropic_api"),
        "google" => Ok("google_api"),
        "openrouter" => Ok("openrouter"),
        _ => bail!("Choose a supported API provider"),
    }
}

fn checked_key(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty() || key.len() > 16_384 || key.chars().any(char::is_control) {
        bail!("Enter a nonempty API key without control characters (maximum 16 KiB)");
    }
    Ok(key)
}

/// Store first, then verify via the account's model-list endpoint. Verification
/// never generates content. Failed verification keeps the connection visible
/// for retry/removal and clears its candidate inventory.
pub async fn connect_api_key(provider: &str, key: &str) -> Result<()> {
    let group = rate_group(provider)?;
    let key = checked_key(key)?;
    let mut registry = AccountRegistry::load_strict()?;
    let id = crate::registry::add_api_key_account(&mut registry, provider, key, group)
        .context("Could not save the connection in this profile and its OS credential store")?;
    verify_in_registry(&mut registry, &id).await
}

pub async fn verify_api_connection(account_id: &str) -> Result<()> {
    let mut registry = AccountRegistry::load_strict()?;
    verify_in_registry(&mut registry, account_id).await
}

async fn verify_in_registry(registry: &mut AccountRegistry, account_id: &str) -> Result<()> {
    let account = registry
        .get_mut(account_id)
        .context("Connection no longer exists; refresh resources")?;
    require_api_account(account)?;
    // Provider errors can contain response bodies. Persist a bounded safe
    // diagnosis, never provider text or the submitted secret.
    if crate::detect::verify_api_key(account).await.is_err() {
        account.status = AccountStatus::Error(
            "Verification failed; check the key and network, then retry".into(),
        );
        account.models.clear();
    }
    registry.save()?;
    Ok(())
}

pub fn disconnect_api_connection(account_id: &str) -> Result<()> {
    let mut registry = AccountRegistry::load_strict()?;
    let account = registry
        .get(account_id)
        .context("Connection no longer exists; refresh resources")?;
    require_api_account(account)?;
    registry.remove(account_id);
    // Remove routing access before touching the credential. A concurrent
    // verification cannot reinsert the account because save checks its base.
    registry.save()?;
    crate::keychain::delete_secret(account_id)
        .context("Connection removed, but its credential could not be removed from the OS store")?;
    Ok(())
}

fn require_api_account(account: &ProviderAccount) -> Result<()> {
    rate_group(&account.provider)?;
    if !matches!(account.credential, Credential::ApiKey) {
        bail!("This action manages API keys only; provider-owned sign-in remains in its own application");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_rejects_unknown_targets_and_control_characters_before_storage() {
        assert!(rate_group("https://untrusted.example").is_err());
        for input in ["", "   ", "key\ninjected", "key\0injected"] {
            assert!(checked_key(input).is_err());
        }
        assert!(checked_key(&"x".repeat(16_385)).is_err());
    }

    #[test]
    fn connection_projection_never_includes_provider_error_bodies() {
        let mut account = ProviderAccount {
            account_id: "seat".into(),
            provider: "openai".into(),
            credential: Credential::ApiKey,
            rate_group: "openai_api".into(),
            status: AccountStatus::Error("sensitive provider response".into()),
            models: vec![],
        };
        let json = serde_json::to_string(&summaries(&AccountRegistry::from_accounts(vec![
            account.clone(),
        ])))
        .unwrap();
        assert!(!json.contains("sensitive provider response"));
        assert!(json.contains("verification_failed"));
        account.credential = Credential::OauthCli {
            binary: "codex".into(),
        };
        assert!(require_api_account(&account).is_err());
    }
}

/// Explicit subscription verification, never discovery: run a tiny host-tool
/// challenge through the existing Codex login and admit only the exact model
/// that actually completes it. No private endpoint or credential extraction.
pub async fn connect_codex(model: Option<&str>) -> Result<String> {
    use crate::adapter::{Message, ProviderTools, Role, StreamEvent, ToolDefinition, ToolResponse};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    let binary = crate::resolve_command("codex")
        .context("Install Codex CLI and sign in with ChatGPT first")?;
    let model = match model {
        Some(value) => value.to_string(),
        None => {
            crate::providers::codex_app_server::configured_model(&binary.display().to_string())
                .await?
        }
    };
    if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
        bail!("invalid Codex model ID");
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let (requests, mut request_rx) = mpsc::channel::<crate::adapter::ProviderToolRequest>(2);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let expected = nonce.clone();
    let dispatcher = tokio::spawn(async move {
        while let Some(request) = request_rx.recv().await {
            count.fetch_add(1, Ordering::Relaxed);
            request
                .response
                .send(ToolResponse {
                    output: expected.clone(),
                    success: true,
                })
                .ok();
        }
    });
    let tools = ProviderTools {
        definitions: vec![ToolDefinition {
            name: "heiwa.verify".into(),
            description: "Return the one-time Heiwa connection verification nonce.".into(),
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        }],
        requests: Some(requests),
        provider: "codex".into(),
        model: model.clone(),
        ..Default::default()
    };
    let (tx, mut rx) = mpsc::channel(32);
    let verify_model = model.clone();
    let inference = tokio::spawn(async move {
        crate::providers::codex_app_server::send(&binary.display().to_string(),&verify_model,&[
            Message{role:Role::System,content:"This is a bounded Heiwa connection test. Call heiwa__verify once, then reply with only its returned nonce. Do no other work.".into()},
            Message{role:Role::User,content:"Verify the connection with the host tool now.".into()},
        ],tx,tools).await
    });
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let mut answer = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(text) => {
                    answer.push_str(&text);
                    if answer.len() > 4096 {
                        bail!("Codex verification answer was oversized");
                    }
                }
                StreamEvent::Done(usage) => {
                    if calls.load(Ordering::Relaxed) != 1 || !answer.contains(&nonce) {
                        bail!("Codex did not complete the host-tool verification challenge");
                    }
                    return Ok(usage);
                }
                StreamEvent::Error(error) => bail!("{error}"),
                _ => {}
            }
        }
        bail!("Codex verification ended without completion")
    })
    .await;
    inference.abort();
    let _ = inference.await;
    dispatcher.abort();
    let _ = dispatcher.await;
    let usage = result.context("Codex verification timed out")??;
    let mut registry = AccountRegistry::load_strict()?;
    let account_id = "openai-cli";
    let detected = crate::DetectedModel {
        model_id: format!("codex/{model}"),
        provider_model_id: model.clone(),
        provider: "openai".into(),
        account_id: account_id.into(),
        rate_group: "openai_sub".into(),
        capability_class: 3,
        context_window: 32_000,
        supports_streaming: true,
        supports_tools: true,
        supports_vision: false,
        supports_audio: false,
        cost_per_1k_input: 0.0,
        cost_per_1k_output: 0.0,
        price_truth: crate::PriceTruth::Unknown,
        inventory_truth: crate::InventoryTruth::Verified,
    };
    registry.upsert(ProviderAccount {
        account_id: account_id.into(),
        provider: "openai".into(),
        credential: Credential::OauthCli {
            binary: "codex".into(),
        },
        rate_group: "openai_sub".into(),
        status: AccountStatus::Connected,
        models: vec![detected],
    });
    registry.save()?;
    let paths = heiwa_config::HeiwaPaths::try_resolve().context("Heiwa profile unavailable")?;
    let dir = paths.receipts_dir().join("providers");
    std::fs::create_dir_all(&dir)?;
    let proof = json!({"schema":"heiwa.provider-verification/v1","channel":"codex-app-server","account_id":account_id,"model":model,"verified_at":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),"proof":"host-tool nonce returned through completed inference","tool_calls":1,"usage":usage,"billing":"ChatGPT plan; provider limits apply; no per-token price asserted"});
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(dir.join(format!("codex-{}.json", uuid::Uuid::new_v4())))?;
    file.write_all(&serde_json::to_vec(&proof)?)?;
    file.sync_all()?;
    Ok(model)
}
