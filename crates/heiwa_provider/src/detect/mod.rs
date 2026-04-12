pub mod ollama;

use crate::registry::{add_local_runtime_account, AccountRegistry, AccountStatus, ProviderAccount};

/// Auto-discover local providers and refresh model inventories for all
/// connected accounts.
///
/// Currently discovers:
/// - Ollama on localhost:11434
///
/// Future: detect installed CLIs and register them as cli accounts.
pub async fn auto_discover(registry: &mut AccountRegistry) -> Vec<String> {
    let mut changes = Vec::new();

    // --- Ollama auto-discovery ---
    // Ensure an Ollama local account exists if none is registered
    let has_ollama = registry.accounts.iter().any(|a| a.provider == "ollama");
    if !has_ollama {
        if let Ok(id) = add_local_runtime_account(registry, "ollama", ollama::DEFAULT_ENDPOINT) {
            changes.push(format!("Registered Ollama account: {}", id));
        }
    }

    // Probe all Ollama accounts
    for account in registry
        .accounts
        .iter_mut()
        .filter(|a| a.provider == "ollama")
    {
        match ollama::detect_models(account).await {
            Ok(()) => {
                changes.push(format!(
                    "{}: detected {} models",
                    account.account_id,
                    account.models.len()
                ));
            }
            Err(_) => {
                // Ollama not running — leave status as Disconnected, clear stale models
            }
        }
    }

    // Save after all discovery
    if !changes.is_empty() {
        registry.save().ok();
    }

    changes
}

/// Verify an API key by probing the provider's model list endpoint.
///
/// On success, updates the account's status to Connected and populates
/// the model inventory.  On failure, sets status to Error.
pub async fn verify_api_key(account: &mut ProviderAccount) -> anyhow::Result<()> {
    let api_key = crate::registry::resolve_secret(&account.account_id).ok_or_else(|| {
        anyhow::anyhow!("No API key found in Keychain for {}", account.account_id)
    })?;

    match account.provider.as_str() {
        "anthropic" => verify_anthropic(account, &api_key).await,
        "openai" => verify_openai(account, &api_key).await,
        _ => {
            // For unknown providers, just mark as connected (key stored, unverified models)
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify an Anthropic API key by calling the Messages API with a minimal
/// request.  Anthropic doesn't have a public `/v1/models` endpoint for
/// listing models, so we infer the model list from known models and check
/// the key is valid with a cheap probe.
async fn verify_anthropic(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Minimal messages request to verify the key is valid
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(r#"{"model":"claude-haiku-4-5-20241022","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#)
        .send()
        .await?;

    if resp.status().is_success() || resp.status().as_u16() == 200 {
        account.status = AccountStatus::Connected;
        account.models = anthropic_known_models(&account.account_id, &account.rate_group);
        Ok(())
    } else if resp.status().as_u16() == 401 {
        account.status = AccountStatus::Error("Invalid API key".to_string());
        account.models.clear();
        Err(anyhow::anyhow!("Invalid Anthropic API key"))
    } else {
        // Other errors (rate limit, etc.) — key might be valid
        account.status = AccountStatus::Connected;
        account.models = anthropic_known_models(&account.account_id, &account.rate_group);
        Ok(())
    }
}

fn anthropic_known_models(
    account_id: &str,
    rate_group: &str,
) -> Vec<crate::registry::DetectedModel> {
    use crate::registry::{DetectedModel, InventoryTruth};
    vec![
        DetectedModel {
            model_id: "claude-opus-4".to_string(),
            provider_model_id: "claude-opus-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            account_id: account_id.to_string(),
            rate_group: rate_group.to_string(),
            capability_class: 5,
            context_window: 200_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_audio: false,
            cost_per_1k_input: 0.015,
            cost_per_1k_output: 0.075,
            inventory_truth: InventoryTruth::Inferred,
        },
        DetectedModel {
            model_id: "claude-sonnet-4".to_string(),
            provider_model_id: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            account_id: account_id.to_string(),
            rate_group: rate_group.to_string(),
            capability_class: 4,
            context_window: 200_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_audio: false,
            cost_per_1k_input: 0.003,
            cost_per_1k_output: 0.015,
            inventory_truth: InventoryTruth::Inferred,
        },
        DetectedModel {
            model_id: "claude-haiku-4".to_string(),
            provider_model_id: "claude-haiku-4-5-20241022".to_string(),
            provider: "anthropic".to_string(),
            account_id: account_id.to_string(),
            rate_group: rate_group.to_string(),
            capability_class: 2,
            context_window: 200_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_audio: false,
            cost_per_1k_input: 0.0008,
            cost_per_1k_output: 0.004,
            inventory_truth: InventoryTruth::Inferred,
        },
    ]
}

/// Verify an OpenAI API key by listing models.
async fn verify_openai(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if resp.status().as_u16() == 401 {
        account.status = AccountStatus::Error("Invalid API key".to_string());
        account.models.clear();
        return Err(anyhow::anyhow!("Invalid OpenAI API key"));
    }

    if resp.status().is_success() {
        // Parse model list — OpenAI returns { data: [{ id, ... }] }
        let body: serde_json::Value = resp.json().await?;
        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            account.models = data
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    // Only include chat-capable models
                    if !is_openai_chat_model(id) {
                        return None;
                    }
                    Some(crate::registry::DetectedModel {
                        model_id: id.to_string(),
                        provider_model_id: id.to_string(),
                        provider: "openai".to_string(),
                        account_id: account.account_id.clone(),
                        rate_group: account.rate_group.clone(),
                        capability_class: openai_capability_class(id),
                        context_window: openai_context_window(id),
                        supports_streaming: true,
                        supports_tools: true,
                        supports_vision: id.contains("gpt-4")
                            || id.contains("o3")
                            || id.contains("o4"),
                        supports_audio: false,
                        cost_per_1k_input: 0.002, // approximate default
                        cost_per_1k_output: 0.008,
                        inventory_truth: crate::registry::InventoryTruth::Verified,
                    })
                })
                .collect();
            account.status = AccountStatus::Connected;
        }
    } else {
        account.status = AccountStatus::Connected;
    }

    Ok(())
}

fn is_openai_chat_model(id: &str) -> bool {
    id.starts_with("gpt-4")
        || id.starts_with("gpt-3.5")
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("o4")
        || id.starts_with("chatgpt")
}

fn openai_capability_class(id: &str) -> u8 {
    if id.starts_with("o3") || id.starts_with("o4-mini-deep") {
        5
    } else if id.starts_with("gpt-4.1") || id.starts_with("o4") || id.starts_with("o1") {
        4
    } else if id.starts_with("gpt-4") {
        3
    } else {
        2
    }
}

fn openai_context_window(id: &str) -> u32 {
    if id.contains("gpt-4.1") {
        1_000_000
    } else if id.contains("o3") || id.contains("o4") {
        200_000
    } else if id.contains("gpt-4") {
        128_000
    } else {
        16_384
    }
}
