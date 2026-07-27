use heiwa_provider::registry::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

static OLLAMA_BASE_ENV_LOCK: Mutex<()> = Mutex::new(());

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
            assert!(
                !account.models.is_empty(),
                "should detect at least one model"
            );
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
            supports_audio: false,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            inventory_truth: InventoryTruth::Verified,
        }],
    };

    let result = heiwa_provider::detect::ollama::detect_models(&mut account).await;
    assert!(result.is_err());
    assert_eq!(account.status, AccountStatus::Disconnected);
    assert!(
        account.models.is_empty(),
        "stale models should be cleared on failure"
    );
}

/// Hermetic callers must be able to prevent discovery from touching a live
/// operator daemon, even when an account still carries the production default.
#[tokio::test]
async fn detect_ollama_honors_hermetic_base_override() {
    let _env_lock = OLLAMA_BASE_ENV_LOCK.lock().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let bytes = stream.read(&mut request).unwrap();
        let request = std::str::from_utf8(&request[..bytes]).unwrap();
        assert!(request.starts_with("GET /api/tags HTTP/1.1"), "{request}");
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"models\":[]}",
            )
            .unwrap();
    });

    let previous = std::env::var_os("HEIWA_OLLAMA_BASE");
    std::env::set_var("HEIWA_OLLAMA_BASE", &endpoint);
    let mut account = ProviderAccount {
        account_id: "ollama-override-test".to_string(),
        provider: "ollama".to_string(),
        credential: Credential::LocalRuntime {
            endpoint: "http://127.0.0.1:11434".to_string(),
        },
        rate_group: "local".to_string(),
        status: AccountStatus::Disconnected,
        models: vec![],
    };
    let result = heiwa_provider::detect::ollama::detect_models(&mut account).await;
    match previous {
        Some(value) => std::env::set_var("HEIWA_OLLAMA_BASE", value),
        None => std::env::remove_var("HEIWA_OLLAMA_BASE"),
    }
    server.join().unwrap();

    result.unwrap();
    assert_eq!(account.status, AccountStatus::Connected);
    assert!(account.models.is_empty());
}
