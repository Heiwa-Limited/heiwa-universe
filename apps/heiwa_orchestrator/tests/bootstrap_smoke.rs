use heiwa_orchestrator::config::RuntimeConfig;

#[test]
fn runtime_config_reads_expected_defaults() {
    let cfg = RuntimeConfig::from_env();
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.state_backend, "spacetimedb");
}
