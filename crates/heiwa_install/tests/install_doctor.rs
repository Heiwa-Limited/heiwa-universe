use heiwa_install::{
    check_installation, get_heiwa_dir, parse_plugin_source, plan_plugin_install, run_install,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn with_temp_home<T>(f: impl FnOnce(&PathBuf) -> T) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let tmp = std::env::temp_dir().join(format!("heiwa-install-test-{}", uuid::Uuid::new_v4()));
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
fn test_doctor_discovery() {
    let report = check_installation().expect("failed to run doctor");

    // In this environment, we expect at least Rust and Python to be present
    assert!(report.rust_version.is_some(), "Rust should be detected");
    assert!(report.python_version.is_some(), "Python should be detected");
}

#[test]
fn test_heiwa_dir_uses_owner_home_state_root() {
    let dir = get_heiwa_dir();
    let dir_str = dir.to_string_lossy();
    assert!(
        dir_str.contains(".heiwa"),
        "expected heiwa state dir under ~/.heiwa, got {:?}",
        dir
    );
    assert!(
        !dir_str.contains(".gemini/tmp/heiwa-universe"),
        "heiwa state dir should not live under gemini temp roots: {:?}",
        dir
    );
}

#[test]
fn test_install_creates_runtime_layout_and_canonical_launcher() {
    with_temp_home(|home| {
        run_install().expect("run_install should succeed");

        let runtime_root = home.join(".heiwa");
        assert!(
            runtime_root.join("machine.json").exists(),
            "machine manifest should exist"
        );

        for dirname in [
            "bin", "logs", "sessions", "cache", "state", "secrets", "plugins",
        ] {
            assert!(
                runtime_root.join(dirname).is_dir(),
                "expected runtime directory {} under ~/.heiwa",
                dirname
            );
        }

        let launcher_path = runtime_root.join("bin").join("heiwa");
        assert!(
            launcher_path.exists(),
            "expected canonical launcher at ~/.heiwa/bin/heiwa"
        );

        let launcher = fs::read_to_string(&launcher_path).expect("read launcher");
        assert!(
            launcher.contains("target/debug/heiwa"),
            "launcher should prefer the real Rust binary: {}",
            launcher
        );
        assert!(
            launcher.contains("apps/heiwa_cli/bin/heiwa"),
            "launcher should still support repo/dev wrapper fallback: {}",
            launcher
        );
    });
}

#[test]
fn test_parse_plugin_source_supports_github_shortform() {
    let source = parse_plugin_source("gh:Strategizing/heiwa-example").expect("parse plugin source");

    assert_eq!(source.scheme, "gh");
    assert_eq!(source.host, "github.com");
    assert_eq!(source.owner, "Strategizing");
    assert_eq!(source.repo, "heiwa-example");
    assert_eq!(source.reference, None);
    assert_eq!(source.canonical(), "gh:Strategizing/heiwa-example");
    assert_eq!(
        source.clone_url(),
        "https://github.com/Strategizing/heiwa-example.git"
    );
}

#[test]
fn test_parse_plugin_source_supports_optional_reference() {
    let source =
        parse_plugin_source("gh:Strategizing/heiwa-example@v1.2.3").expect("parse plugin source");

    assert_eq!(source.reference.as_deref(), Some("v1.2.3"));
    assert_eq!(source.canonical(), "gh:Strategizing/heiwa-example@v1.2.3");
}

#[test]
fn test_parse_plugin_source_rejects_invalid_specs() {
    for raw in [
        "Strategizing/heiwa-example",
        "gh:Strategizing",
        "gh:/heiwa-example",
        "gh:Strategizing/heiwa example",
        "gh:Strategizing/heiwa-example@",
        "gh:Strategizing/heiwa-example/extra",
    ] {
        assert!(
            parse_plugin_source(raw).is_err(),
            "expected invalid plugin source: {}",
            raw
        );
    }
}

#[test]
fn test_plan_plugin_install_uses_runtime_plugin_layout() {
    with_temp_home(|home| {
        let planned = plan_plugin_install("gh:Strategizing/heiwa-example@stable")
            .expect("plan plugin install");
        let expected_dir = home
            .join(".heiwa")
            .join("plugins")
            .join("github.com")
            .join("Strategizing")
            .join("heiwa-example");

        assert_eq!(planned.install_dir, expected_dir);
        assert_eq!(
            planned.receipt_path,
            expected_dir.join(".heiwa-install.json")
        );
        assert_eq!(planned.canonical, "gh:Strategizing/heiwa-example@stable");
        assert_eq!(
            planned.clone_url,
            "https://github.com/Strategizing/heiwa-example.git"
        );
    });
}
