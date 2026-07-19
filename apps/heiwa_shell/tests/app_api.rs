//! Integration tests for the programmatic Heiwa.app API CLI bridge.

use std::process::Command;

#[test]
fn app_api_dry_run_exposes_get_and_post_contracts_without_network() {
    let home = tempfile::tempdir().unwrap();

    let get = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .env_remove("HEIWA_MACHINE_AUTH_TOKEN")
        .env_remove("HEIWA_AUTH_TOKEN")
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
    assert_eq!(get_payload["auth"], "missing");

    let post = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .env("HEIWA_MACHINE_AUTH_TOKEN", "dry-run-secret-token")
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
    assert_eq!(post_payload["auth"], "machine_token_configured");
    assert!(!String::from_utf8_lossy(&post.stdout).contains("dry-run-secret-token"));

    assert!(
        !home.path().join(".heiwa").exists(),
        "dry-run must not write state"
    );
}

#[test]
fn app_api_fails_before_network_without_auth_configuration() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .env_remove("HEIWA_MACHINE_AUTH_TOKEN")
        .env_remove("HEIWA_AUTH_TOKEN")
        .args([
            "app",
            "api",
            "get",
            "/api/v1/operator/threads",
            "--port",
            "9",
        ])
        .output()
        .expect("binary runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("auth_not_configured"), "stderr: {stderr}");
    assert!(!stderr.contains("cannot connect"), "stderr: {stderr}");
}

#[test]
fn app_api_injects_bearer_token_without_echoing_it() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sent, received) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buf).unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buf[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        sent.send(String::from_utf8_lossy(&request).to_string())
            .unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .unwrap();
    });

    let token = "wire-only-machine-token";
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HEIWA_MACHINE_AUTH_TOKEN", token)
        .args([
            "app",
            "api",
            "get",
            "/api/v1/operator/threads",
            "--port",
            &port.to_string(),
        ])
        .output()
        .expect("binary runs");
    server.join().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = received.recv().unwrap();
    assert!(request.contains(&format!("Authorization: Bearer {token}\r\n")));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(token));
}
