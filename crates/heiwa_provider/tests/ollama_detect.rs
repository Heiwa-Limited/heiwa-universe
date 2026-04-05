use heiwa_provider::registry::*;

/// Test Ollama model detection against a running instance.
///
/// This test is ignored by default because it requires Ollama running
/// on localhost:11434.  Run with:
///   cargo test -p heiwa-provider --test ollama_detect -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn detect_ollama_models_live() {
    let mut account = ProviderAccount {
        account_id: "ollama-local".to_string(),
        provider: "ollama".to_string(),
        credential: Credential::LocalRuntime {
            endpoint: "http://127.0.0.1:11434".to_string(),
        },
        rate_group: "local".to_string(),
        status: AccountStatus::Disconnected,
        models: vec![],
    };

    let result = heiwa_provider::detect::ollama::detect_models(&mut account).await;

    match result {
        Ok(()) => {
            assert_eq!(account.status, AccountStatus::Connected);
            assert!(!account.models.is_empty(), "should detect at least one model");
            println!("Detected {} Ollama models:", account.models.len());
            for m in &account.models {
                println!(
                    "  {} (class:{}, ctx:{}, truth:{:?})",
                    m.model_id, m.capability_class, m.context_window, m.inventory_truth
                );
            }
            // All should be verified since they came from the API
            for m in &account.models {
                assert_eq!(m.inventory_truth, InventoryTruth::Verified);
                assert_eq!(m.cost_per_1k_input, 0.0);
                assert_eq!(m.cost_per_1k_output, 0.0);
                assert_eq!(m.rate_group, "local");
            }
        }
        Err(e) => {
            println!("Ollama not reachable (expected in CI): {}", e);
            assert_eq!(account.status, AccountStatus::Disconnected);
            assert!(account.models.is_empty());
        }
    }
}

/// Test that detection gracefully handles Ollama not running.
#[tokio::test]
async fn detect_ollama_unreachable() {
    let mut account = ProviderAccount {
        account_id: "ollama-test".to_string(),
        provider: "ollama".to_string(),
        credential: Credential::LocalRuntime {
            endpoint: "http://127.0.0.1:19999".to_string(), // unlikely to be running
        },
        rate_group: "local".to_string(),
        status: AccountStatus::Connected, // start connected to verify it flips
        models: vec![DetectedModel {
            model_id: "stale".to_string(),
            provider_model_id: "stale".to_string(),
            provider: "ollama".to_string(),
            account_id: "ollama-test".to_string(),
            rate_group: "local".to_string(),
            capability_class: 1,
            context_window: 8192,
            supports_streaming: true,
            supports_tools: false,
            supports_vision: false,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            inventory_truth: InventoryTruth::Verified,
        }],
    };

    let result = heiwa_provider::detect::ollama::detect_models(&mut account).await;
    assert!(result.is_err());
    assert_eq!(account.status, AccountStatus::Disconnected);
    assert!(account.models.is_empty(), "stale models should be cleared on failure");
}
