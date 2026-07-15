use heiwa_core::config::RuntimeConfig;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[test]
fn runtime_config_reads_expected_defaults() {
    let _guard = env_lock();
    let vars = [
        "PORT",
        "HEIWA_STATE_BACKEND",
        "HEIWA_AUTH_TOKEN",
        "HEIWA_MACHINE_AUTH_TOKEN",
        "HEIWA_AUTH_SECRET",
        "HEIWA_JWT_SIGNING_SECRET",
        "HEIWA_NODE_ID",
        "MODEL_TIERS_SEED_PATH",
        "AI_ROUTER_SEED_PATH",
        "LOG_LEVEL",
    ];
    let saved: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|key| ((*key).to_string(), std::env::var(key).ok()))
        .collect();
    for key in vars {
        unsafe { std::env::remove_var(key) };
    }

    let cfg = RuntimeConfig::from_env();

    for (key, value) in saved {
        match value {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.state_backend, "local-jsonl");
    assert!(cfg.machine_auth_token.is_empty());
    assert!(cfg.jwt_signing_secret.is_empty());
}

#[test]
fn runtime_config_prefers_split_tokens_with_legacy_fallbacks() {
    let _guard = env_lock();
    let vars = [
        ("HEIWA_MACHINE_AUTH_TOKEN", Some("machine-new")),
        ("HEIWA_AUTH_TOKEN", Some("machine-legacy")),
        ("HEIWA_JWT_SIGNING_SECRET", Some("jwt-new")),
        ("HEIWA_AUTH_SECRET", Some("jwt-legacy")),
    ];
    let saved: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
        .collect();
    for (key, value) in vars {
        unsafe { std::env::set_var(key, value.expect("value")) };
    }

    let cfg = RuntimeConfig::from_env();

    for (key, value) in saved {
        match value {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    assert_eq!(cfg.machine_auth_token, "machine-new");
    assert_eq!(cfg.jwt_signing_secret, "jwt-new");
}
