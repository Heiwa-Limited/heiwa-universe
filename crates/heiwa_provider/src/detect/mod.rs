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
        if let Ok(endpoint) = ollama::resolve_configured_endpoint(None) {
            if let Ok(id) = add_local_runtime_account(registry, "ollama", endpoint.as_str()) {
                changes.push(format!("Registered Ollama account: {}", id));
            }
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
        "google" => verify_google(account, &api_key).await,
        "openrouter" => verify_openrouter(account, &api_key).await,
        _ => {
            // For unknown providers, just mark as connected (key stored, unverified models)
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify an Anthropic API key by listing models.
///
/// Anthropic publishes `GET /v1/models`, so the key check and the inventory
/// are one call and the inventory is `Verified` rather than a list this
/// crate carries. The previous implementation probed the Messages API with a
/// hardcoded model id, which both invented inventory and pinned a snapshot
/// that goes stale on every model release.
async fn verify_anthropic(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    match crate::providers::anthropic_api::discover_models(
        api_key,
        crate::providers::anthropic_api::DEFAULT_BASE_URL,
        &account.account_id,
        &account.rate_group,
    )
    .await
    {
        Ok(models) => {
            account.models = models;
            account.status = AccountStatus::Connected;
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            if message.contains("401") || message.contains("403") {
                account.status = AccountStatus::Error("Invalid API key".to_string());
                account.models.clear();
                return Err(anyhow::anyhow!("Invalid Anthropic API key"));
            }
            // Transient upstream failure: the key may still be good, so keep
            // the account usable rather than marking it broken.
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify a Google API key by listing models.
async fn verify_google(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    match crate::providers::gemini_api::discover_models(
        api_key,
        crate::providers::gemini_api::DEFAULT_BASE_URL,
        &account.account_id,
        &account.rate_group,
    )
    .await
    {
        Ok(models) => {
            account.models = models;
            account.status = AccountStatus::Connected;
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            if message.contains("400") || message.contains("401") || message.contains("403") {
                account.status = AccountStatus::Error("Invalid API key".to_string());
                account.models.clear();
                return Err(anyhow::anyhow!("Invalid Google API key"));
            }
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify an OpenAI API key by listing models.
async fn verify_openai(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    match crate::providers::openai_api::discover_models(
        api_key,
        crate::providers::openai_api::DEFAULT_BASE_URL,
        &account.account_id,
        &account.rate_group,
    )
    .await
    {
        Ok(models) => {
            account.models = models;
            account.status = AccountStatus::Connected;
            Ok(())
        }
        Err(error) => {
            if error.to_string().contains("401") {
                account.status = AccountStatus::Error("Invalid API key".to_string());
                account.models.clear();
                return Err(anyhow::anyhow!("Invalid OpenAI API key"));
            }
            account.status = AccountStatus::Connected;
            Ok(())
        }
    }
}

/// Verify an OpenRouter API key and detect the free-tier model inventory.
///
/// Calls `GET https://openrouter.ai/api/v1/models` and keeps only models
/// whose id carries the `:free` suffix — the zero-cost overflow tier that
/// replaced the dead gemini-cli free seat.  Note the models endpoint is
/// public, so a syntactically-stored but revoked key is only truly proven
/// on the first completion call.
async fn verify_openrouter(account: &mut ProviderAccount, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if resp.status().as_u16() == 401 {
        account.status = AccountStatus::Error("Invalid API key".to_string());
        account.models.clear();
        return Err(anyhow::anyhow!("Invalid OpenRouter API key"));
    }

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        account.models = openrouter_free_models(&body, &account.account_id, &account.rate_group);
        account.status = AccountStatus::Connected;
    } else {
        // Transient upstream error — key may still be valid.
        account.status = AccountStatus::Connected;
    }

    Ok(())
}

/// Map an OpenRouter `/models` response to free-tier `DetectedModel`s.
fn openrouter_free_models(
    body: &serde_json::Value,
    account_id: &str,
    rate_group: &str,
) -> Vec<crate::registry::DetectedModel> {
    use crate::registry::{DetectedModel, InventoryTruth};

    let Some(data) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };

    data.iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?;
            if !id.ends_with(":free") {
                return None;
            }
            let input_modalities: Vec<&str> = m
                .pointer("/architecture/input_modalities")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let supported_parameters: Vec<&str> = m
                .get("supported_parameters")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            Some(DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: "openrouter".to_string(),
                account_id: account_id.to_string(),
                rate_group: rate_group.to_string(),
                capability_class: openrouter_capability_class(id),
                context_window: m
                    .get("context_length")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8_192) as u32,
                supports_streaming: true,
                supports_tools: supported_parameters.contains(&"tools"),
                supports_vision: input_modalities.contains(&"image"),
                supports_audio: input_modalities.contains(&"audio"),
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            })
        })
        .collect()
}

/// Capability class for a free-tier OpenRouter model.
///
/// Hard cap at 3: free models absorb bulk/overflow work but must never
/// outrank the OAuth seats (claude/codex, class 4-5) on quality-sensitive
/// intents — class >= 4 is what earns the `advanced_coding` strength.
fn openrouter_capability_class(id: &str) -> u8 {
    let lower = id.to_lowercase();
    let big = [
        "70b",
        "72b",
        "235b",
        "405b",
        "671b",
        "deepseek-r1",
        "deepseek-v3",
    ]
    .iter()
    .any(|hint| lower.contains(hint));
    if big {
        3
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InventoryTruth;

    fn sample_models_body() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "id": "meta-llama/llama-3.3-70b-instruct:free",
                    "context_length": 131072,
                    "architecture": { "input_modalities": ["text"] },
                    "supported_parameters": ["tools", "temperature"]
                },
                {
                    "id": "openai/gpt-4o",
                    "context_length": 128000,
                    "architecture": { "input_modalities": ["text", "image"] },
                    "supported_parameters": ["tools"]
                },
                {
                    "id": "qwen/qwen2.5-vl-7b-instruct:free",
                    "context_length": 32768,
                    "architecture": { "input_modalities": ["text", "image"] },
                    "supported_parameters": ["temperature"]
                }
            ]
        })
    }

    #[test]
    fn openrouter_keeps_only_free_suffix_models() {
        let models =
            openrouter_free_models(&sample_models_body(), "openrouter-api-3", "openrouter");
        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .all(|m| m.provider_model_id.ends_with(":free")));
        assert!(models.iter().all(|m| m.provider == "openrouter"));
    }

    #[test]
    fn openrouter_models_are_zero_cost_verified_in_account_rate_group() {
        let models =
            openrouter_free_models(&sample_models_body(), "openrouter-api-3", "openrouter");
        for m in &models {
            assert_eq!(m.account_id, "openrouter-api-3");
            assert_eq!(m.rate_group, "openrouter");
            assert_eq!(m.cost_per_1k_input, 0.0);
            assert_eq!(m.cost_per_1k_output, 0.0);
            assert_eq!(m.inventory_truth, InventoryTruth::Verified);
        }
    }

    #[test]
    fn openrouter_capability_caps_below_oauth_seats() {
        // Free-tier models must never outrank OAuth seats (class 4-5) on
        // quality-sensitive intents: cap at 3 even for large models.
        let models = openrouter_free_models(&sample_models_body(), "a", "openrouter");
        let llama70b = &models[0];
        let qwen7b = &models[1];
        assert_eq!(llama70b.capability_class, 3);
        assert_eq!(qwen7b.capability_class, 2);
        assert!(models.iter().all(|m| m.capability_class <= 3));
    }

    #[test]
    fn openrouter_maps_context_window_and_modalities() {
        let models = openrouter_free_models(&sample_models_body(), "a", "openrouter");
        let llama70b = &models[0];
        assert_eq!(llama70b.context_window, 131_072);
        assert!(llama70b.supports_tools);
        assert!(!llama70b.supports_vision);

        let qwen_vl = &models[1];
        assert_eq!(qwen_vl.context_window, 32_768);
        assert!(!qwen_vl.supports_tools);
        assert!(qwen_vl.supports_vision);
    }
}
