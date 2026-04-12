use heiwa_provider::registry::*;

#[test]
fn multi_account_per_provider() {
    let mut reg = AccountRegistry::default();

    reg.upsert(ProviderAccount {
        account_id: "anthropic-api-1".to_string(),
        provider: "anthropic".to_string(),
        credential: Credential::ApiKey,
        rate_group: "anthropic_api".to_string(),
        status: AccountStatus::Connected,
        models: vec![DetectedModel {
            model_id: "claude-sonnet-4".to_string(),
            provider_model_id: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            account_id: "anthropic-api-1".to_string(),
            rate_group: "anthropic_api".to_string(),
            capability_class: 4,
            context_window: 200_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_audio: false,
            cost_per_1k_input: 0.003,
            cost_per_1k_output: 0.015,
            inventory_truth: InventoryTruth::Verified,
        }],
    });

    reg.upsert(ProviderAccount {
        account_id: "anthropic-cli".to_string(),
        provider: "anthropic".to_string(),
        credential: Credential::OauthCli {
            binary: "claude".to_string(),
        },
        rate_group: "anthropic_sub".to_string(),
        status: AccountStatus::Connected,
        models: vec![DetectedModel {
            model_id: "claude-sonnet-4".to_string(),
            provider_model_id: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            account_id: "anthropic-cli".to_string(),
            rate_group: "anthropic_sub".to_string(),
            capability_class: 4,
            context_window: 200_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_audio: false,
            cost_per_1k_input: 0.003,
            cost_per_1k_output: 0.015,
            inventory_truth: InventoryTruth::Inferred,
        }],
    });

    // Two accounts for same provider
    assert_eq!(reg.accounts_for("anthropic").len(), 2);

    // Both connected
    assert_eq!(reg.connected_providers(), vec!["anthropic"]);

    // Models from both accounts, sorted by rate group then capability
    let models = reg.all_models();
    assert_eq!(models.len(), 2);
    // "anthropic_api" sorts before "anthropic_sub"
    assert_eq!(models[0].account_id, "anthropic-api-1");
    assert_eq!(models[0].inventory_truth, InventoryTruth::Verified);
    assert_eq!(models[1].account_id, "anthropic-cli");
    assert_eq!(models[1].inventory_truth, InventoryTruth::Inferred);
}

#[test]
fn ollama_local_account() {
    let mut reg = AccountRegistry::default();

    reg.upsert(ProviderAccount {
        account_id: "ollama-local".to_string(),
        provider: "ollama".to_string(),
        credential: Credential::LocalRuntime {
            endpoint: "http://127.0.0.1:11434".to_string(),
        },
        rate_group: "local".to_string(),
        status: AccountStatus::Connected,
        models: vec![
            DetectedModel {
                model_id: "qwen3.5:4b".to_string(),
                provider_model_id: "qwen3.5:4b".to_string(),
                provider: "ollama".to_string(),
                account_id: "ollama-local".to_string(),
                rate_group: "local".to_string(),
                capability_class: 2,
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: false,
                supports_vision: false,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            },
            DetectedModel {
                model_id: "qwen2.5-coder:0.5b".to_string(),
                provider_model_id: "qwen2.5-coder:0.5b".to_string(),
                provider: "ollama".to_string(),
                account_id: "ollama-local".to_string(),
                rate_group: "local".to_string(),
                capability_class: 1,
                context_window: 32_768,
                supports_streaming: true,
                supports_tools: false,
                supports_vision: false,
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            },
        ],
    });

    let models = reg.all_models();
    assert_eq!(models.len(), 2);
    // Higher capability class first
    assert_eq!(models[0].model_id, "qwen3.5:4b");
    assert_eq!(models[1].model_id, "qwen2.5-coder:0.5b");
}

#[test]
fn disconnected_accounts_excluded_from_models() {
    let mut reg = AccountRegistry::default();

    reg.upsert(ProviderAccount {
        account_id: "disconnected-1".to_string(),
        provider: "test".to_string(),
        credential: Credential::ApiKey,
        rate_group: "test".to_string(),
        status: AccountStatus::Disconnected,
        models: vec![DetectedModel {
            model_id: "should-not-appear".to_string(),
            provider_model_id: "x".to_string(),
            provider: "test".to_string(),
            account_id: "disconnected-1".to_string(),
            rate_group: "test".to_string(),
            capability_class: 3,
            context_window: 8192,
            supports_streaming: true,
            supports_tools: false,
            supports_vision: false,
            supports_audio: false,
            cost_per_1k_input: 0.001,
            cost_per_1k_output: 0.002,
            inventory_truth: InventoryTruth::Verified,
        }],
    });

    assert!(reg.all_models().is_empty());
    assert!(reg.connected_providers().is_empty());
}
