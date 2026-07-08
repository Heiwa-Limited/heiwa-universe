//! Integration tests for the programmatic Heiwa.app API CLI bridge.

use std::process::Command;

#[test]
fn app_api_dry_run_exposes_get_and_post_contracts_without_network() {
    let home = tempfile::tempdir().unwrap();

    let get = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .args([
            "app",
            "api",
            "get",
            "/api/v1/session",
            "--port",
            "7475",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("binary runs");
    assert!(
        get.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&get.stderr)
    );
    let get_payload: serde_json::Value = serde_json::from_slice(&get.stdout).expect("get json");
    assert_eq!(get_payload["command"], "app api");
    assert_eq!(get_payload["method"], "GET");
    assert_eq!(get_payload["path"], "/api/v1/session");
    assert_eq!(get_payload["url"], "http://127.0.0.1:7475/api/v1/session");
    assert_eq!(get_payload["dry_run"], true);

    let post = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .args([
            "app",
            "api",
            "post",
            "/api/v1/agents/dispatch",
            "--port",
            "7475",
            "--body",
            r#"{"task":"summarize session","lane":"auto","approval_policy":"ask"}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("binary runs");
    assert!(
        post.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&post.stderr)
    );
    let post_payload: serde_json::Value = serde_json::from_slice(&post.stdout).expect("post json");
    assert_eq!(post_payload["method"], "POST");
    assert_eq!(post_payload["path"], "/api/v1/agents/dispatch");
    assert_eq!(post_payload["body"]["lane"], "auto");

    assert!(
        !home.path().join(".heiwa").exists(),
        "dry-run must not write state"
    );
}
