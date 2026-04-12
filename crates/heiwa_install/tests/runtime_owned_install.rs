use heiwa_install::run_install;
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

    let tmp = std::env::temp_dir().join(format!("heiwa-runtime-owned-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp).expect("create temp home");

    let original_home = env::var_os("HOME");
    let original_root = env::var_os("HEIWA_ROOT");
    env::set_var("HOME", &tmp);
    env::set_var(
        "HEIWA_ROOT",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crate parent")
            .parent()
            .expect("repo root")
            .to_path_buf(),
    );

    let result = f(&tmp);

    match original_home {
        Some(v) => env::set_var("HOME", v),
        None => env::remove_var("HOME"),
    }
    match original_root {
        Some(v) => env::set_var("HEIWA_ROOT", v),
        None => env::remove_var("HEIWA_ROOT"),
    }

    let _ = fs::remove_dir_all(&tmp);
    result
}

#[test]
fn install_creates_structured_runtime_layout() {
    with_temp_home(|home| {
        run_install().expect("run_install should succeed");

        let runtime_root = home.join(".heiwa");
        for dirname in [
            "bin",
            "logs",
            "sessions",
            "cache",
            "state",
            "secrets",
            "providers",
            "models",
            "capabilities",
            "modes",
            "policies",
            "generated",
            "artifacts",
        ] {
            assert!(
                runtime_root.join(dirname).is_dir(),
                "expected {} under ~/.heiwa",
                dirname
            );
        }

        assert!(runtime_root.join("config.toml").exists(), "expected config.toml");
        assert!(runtime_root.join("machine.json").exists(), "expected machine.json");
        assert!(runtime_root.join("modes/concise/MODE.md").exists(), "expected concise mode");
        assert!(
            runtime_root.join("capabilities/research/manifest.json").exists(),
            "expected research capability manifest"
        );
        assert!(
            runtime_root.join("capabilities/operator/manifest.json").exists(),
            "expected operator capability manifest"
        );
        assert!(runtime_root.join("models/inventory.json").exists(), "expected model inventory");
        assert!(runtime_root.join("policies/runtime.toml").exists(), "expected runtime policy");
        assert!(
            runtime_root.join("generated/codex/config.toml").exists(),
            "expected generated codex projection"
        );
        assert!(
            runtime_root.join("generated/claude/settings.json").exists(),
            "expected generated claude projection"
        );
        assert!(
            runtime_root.join("generated/gemini/settings.json").exists(),
            "expected generated gemini projection"
        );
        assert!(
            runtime_root.join("generated/antigravity/settings.json").exists(),
            "expected generated antigravity projection"
        );
        assert!(
            home.join(".codex/skills/heiwa-concise-mode/SKILL.md").exists(),
            "expected codex concise mode install"
        );
        assert!(
            home.join(".claude/skills/heiwa-concise-mode/SKILL.md").exists(),
            "expected claude concise mode install"
        );
        assert!(
            home.join(".gemini/extensions/heiwa-concise-mode/gemini-extension.json")
                .exists(),
            "expected gemini concise extension manifest"
        );
        assert!(
            home.join(".gemini/extensions/heiwa-concise-mode/skills/heiwa-concise-mode/SKILL.md")
                .exists(),
            "expected gemini concise skill install"
        );
    });
}

#[test]
fn install_migrates_flat_root_files_forward_without_deleting_them() {
    with_temp_home(|home| {
        let runtime_root = home.join(".heiwa");
        fs::create_dir_all(&runtime_root).expect("create runtime root");

        fs::write(runtime_root.join("accounts.json"), "{\n  \"accounts\": []\n}\n").expect("write accounts");
        fs::write(runtime_root.join("provider_connections.json"), "[\"codex\"]\n").expect("write provider connections");
        fs::write(runtime_root.join("identity.json"), "{\n  \"user_id\": \"devon\"\n}\n").expect("write identity");
        fs::write(runtime_root.join("connection.json"), "{\n  \"url\": \"https://example.com\"\n}\n").expect("write connection");

        run_install().expect("run_install should succeed");

        assert!(runtime_root.join("accounts.json").exists(), "legacy accounts should remain");
        assert!(runtime_root.join("provider_connections.json").exists(), "legacy provider connections should remain");
        assert!(runtime_root.join("identity.json").exists(), "legacy identity should remain");
        assert!(runtime_root.join("connection.json").exists(), "legacy connection should remain");

        assert_eq!(
            fs::read_to_string(runtime_root.join("providers/registry.json")).expect("read registry"),
            "{\n  \"accounts\": []\n}\n"
        );
        assert_eq!(
            fs::read_to_string(runtime_root.join("providers/legacy_connections.json"))
                .expect("read migrated provider connections"),
            "[\"codex\"]\n"
        );
        assert_eq!(
            fs::read_to_string(runtime_root.join("state/identity.json")).expect("read migrated identity"),
            "{\n  \"user_id\": \"devon\"\n}\n"
        );
        assert_eq!(
            fs::read_to_string(runtime_root.join("state/connection.json")).expect("read migrated connection"),
            "{\n  \"url\": \"https://example.com\"\n}\n"
        );
    });
}
