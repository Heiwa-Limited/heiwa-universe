use heiwa_provider::{
    load_identity, save_identity, AccountRegistry, AccountStatus, Credential, HeiwaIdentity,
    ProviderAccount,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn with_temp_home<T>(f: impl FnOnce(&PathBuf) -> T) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner());

    let tmp = std::env::temp_dir().join(format!("heiwa-provider-runtime-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp).expect("create temp home");

    let original_home = env::var_os("HOME");
    env::set_var("HOME", &tmp);

    let result = f(&tmp);

    match original_home {
        Some(v) => env::set_var("HOME", v),
        None => env::remove_var("HOME"),
    }

    let _ = fs::remove_dir_all(&tmp);
    result
}

#[test]
fn registry_save_uses_structured_provider_path() {
    with_temp_home(|home| {
        let registry = AccountRegistry::default();
        registry.save().expect("save registry");

        assert!(
            home.join(".heiwa/providers/registry.json").exists(),
            "registry should save under ~/.heiwa/providers/registry.json"
        );
    });
}

#[test]
fn registry_load_falls_back_to_legacy_flat_accounts_file() {
    with_temp_home(|home| {
        let legacy_registry = AccountRegistry {
            accounts: vec![ProviderAccount {
                account_id: "legacy-openai".to_string(),
                provider: "openai".to_string(),
                credential: Credential::ApiKey,
                rate_group: "openai_api".to_string(),
                status: AccountStatus::Connected,
                models: vec![],
            }],
        };

        let runtime_root = home.join(".heiwa");
        fs::create_dir_all(&runtime_root).expect("create runtime root");
        fs::write(
            runtime_root.join("accounts.json"),
            serde_json::to_string_pretty(&legacy_registry).expect("serialize registry"),
        )
        .expect("write legacy accounts");

        let loaded = AccountRegistry::load();
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].account_id, "legacy-openai");
    });
}

#[test]
fn identity_saves_to_state_and_reads_legacy_fallback() {
    with_temp_home(|home| {
        let identity = HeiwaIdentity {
            user_id: "devon".to_string(),
            auth_token: "token-1".to_string(),
            email: Some("devon@heiwa.ltd".to_string()),
            display_name: Some("Devon".to_string()),
        };

        save_identity(&identity).expect("save identity");
        let structured_identity = home.join(".heiwa/state/identity.json");
        assert!(
            structured_identity.exists(),
            "identity should save under ~/.heiwa/state/identity.json"
        );

        fs::remove_file(&structured_identity).expect("remove structured identity");
        fs::write(
            home.join(".heiwa/identity.json"),
            serde_json::to_string_pretty(&identity).expect("serialize identity"),
        )
        .expect("write legacy identity");

        let loaded = load_identity().expect("load legacy identity");
        assert_eq!(loaded.user_id, "devon");
    });
}
