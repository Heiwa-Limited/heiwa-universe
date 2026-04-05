use heiwa_stdb::StdbClient;

#[tokio::test]
async fn evidence_methods_are_noops_when_offline() {
    let client = StdbClient::offline();
    assert!(!client.is_connected());

    // All evidence methods should return Ok when offline
    assert!(client
        .register_device("dev-1", "user-1", "test-host", "macos", "aarch64")
        .is_ok());
    assert!(client.heartbeat_device("dev-1").is_ok());
    assert!(client
        .sync_provider_status(
            "acct-1",
            "anthropic",
            "dev-1",
            "api_key",
            "local-ref",
            "connected",
            None,
            None,
            "[]",
        )
        .is_ok());
    assert!(client
        .record_route_decision(
            "req-1",
            "task-1",
            "test task",
            "code",
            "low",
            "standard",
            "ollama",
            "ollama",
            "llama3",
            "local",
            "best_score",
            0.9,
        )
        .is_ok());
    assert!(client
        .record_run(
            "run-1",
            "user-1",
            "prop-1",
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:01:00Z",
            "SUCCESS",
            "llama3",
            100,
            50,
            0.0,
            None,
            None,
            None,
        )
        .is_ok());
}
