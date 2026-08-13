use heiwa_install::{
    check_ai_ops_at, check_installation, get_heiwa_dir, parse_plugin_source, plan_plugin_install,
    run_install,
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
    let original_heiwa_home = env::var_os("HEIWA_HOME");
    let original_root = env::var_os("HEIWA_ROOT");
    env::set_var("HOME", &tmp);
    env::remove_var("HEIWA_HOME");
    env::set_var(
        "HEIWA_ROOT",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crate parent")
            .parent()
            .expect("repo root"),
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
    match original_heiwa_home {
        Some(v) => env::set_var("HEIWA_HOME", v),
        None => env::remove_var("HEIWA_HOME"),
    }
    let _ = fs::remove_dir_all(&tmp);

    result
}

#[test]
fn test_heiwa_dir_honors_explicit_runtime_root() {
    with_temp_home(|home| {
        let runtime_root = home.join("custom-runtime");
        env::set_var("HEIWA_HOME", &runtime_root);
        assert_eq!(get_heiwa_dir(), runtime_root);
    });
}

#[test]
fn test_doctor_discovery() {
    let report = check_installation().expect("failed to run doctor");

    // In this environment, we expect at least Rust and Python to be present
    assert!(report.rust_version.is_some(), "Rust should be detected");
    assert!(report.python_version.is_some(), "Python should be detected");
}

#[test]
fn test_ai_ops_doctor_checks_repo_hygiene_gates() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("repo root")
        .to_path_buf();
    let report = check_ai_ops_at(&repo_root).expect("ai ops check should run");

    assert!(report.mcp_notion_http, "Notion MCP must be typed as http");
    assert!(report.biome_configured, "Biome config must exist");
    assert!(report.npm_lint_uses_biome, "npm lint must run Biome");
    assert!(report.ci_lint_uses_biome, "CI must run the Biome gate");
    assert!(
        report.ci_clippy_dead_code_enforced,
        "Rust certification Clippy must not suppress dead_code"
    );
    assert!(
        report.ci_unused_deps_uses_cargo_machete,
        "Rust certification must run cargo machete for unused dependencies"
    );
    assert!(report.is_clean(), "all ai ops checks should be clean");
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
            "app", "bin", "logs", "sessions", "cache", "state", "secrets", "plugins",
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

        let app_bundle = runtime_root.join("app").join("Heiwa.app");
        assert!(
            app_bundle.join("Contents").join("Info.plist").exists(),
            "expected HOME-local Heiwa.app bundle metadata"
        );

        let app_executable = app_bundle.join("Contents").join("MacOS").join("Heiwa");
        assert!(
            app_executable.exists(),
            "expected HOME-local Heiwa.app executable launcher"
        );

        let app_executable_bytes = fs::read(&app_executable).expect("read app executable");
        if let Ok(app_launcher) = String::from_utf8(app_executable_bytes.clone()) {
            assert!(
                app_launcher.contains("app start"),
                "Heiwa.app launcher should start the local app runtime: {}",
                app_launcher
            );
        } else {
            assert!(
                app_executable_bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
                    || app_executable_bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe]),
                "Tauri Heiwa.app executable should be a Mach-O binary"
            );
        }

        let bin_app = runtime_root.join("bin").join("heiwa-app");
        assert!(
            bin_app.exists(),
            "expected CLI shim at ~/.heiwa/bin/heiwa-app"
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
