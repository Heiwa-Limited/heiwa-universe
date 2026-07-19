//! Integration tests for the authenticated operator HTTP contract.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TOKEN: &str = "test-machine-token";

struct TestRuntime {
    child: Child,
    port: u16,
    _home: tempfile::TempDir,
    _evidence: tempfile::TempDir,
}

impl TestRuntime {
    fn start(configured_auth: bool) -> Self {
        Self::start_with_provider_path(configured_auth, true)
    }

    fn start_without_providers() -> Self {
        Self::start_with_provider_path(true, false)
    }

    fn start_with_provider_path(configured_auth: bool, provider_path: bool) -> Self {
        let port = reserve_port();
        let home = tempfile::tempdir().unwrap();
        let evidence = tempfile::tempdir().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_heiwa"));
        command
            .env("HOME", home.path())
            .env("HEIWA_EVIDENCE_DIR", evidence.path())
            .env_remove("HEIWA_MACHINE_AUTH_TOKEN")
            .env_remove("HEIWA_AUTH_TOKEN")
            .env_remove("HEIWA_JWT_SIGNING_SECRET")
            .env_remove("HEIWA_AUTH_SECRET")
            .args(["app", "start", "--port", &port.to_string(), "--no-open"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if configured_auth {
            command.env("HEIWA_MACHINE_AUTH_TOKEN", TOKEN);
        }
        if !provider_path {
            let empty_path = home.path().join("empty-path");
            std::fs::create_dir(&empty_path).unwrap();
            command.env("PATH", empty_path);
        }
        let child = command.spawn().expect("start test runtime");
        wait_for_port(port);
        Self {
            child,
            port,
            _home: home,
            _evidence: evidence,
        }
    }

    fn request(&self, method: &str, target: &str, token: Option<&str>, body: Value) -> Response {
        request(self.port, method, target, token, &body.to_string())
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Response {
    status: u16,
    body: Value,
}

struct HandshakeResponse {
    status: u16,
    head: String,
    body: Value,
}

#[test]
fn operator_routes_fail_closed_before_read_or_action() {
    let runtime = TestRuntime::start(true);

    for (method, target, body) in [
        ("GET", "/api/v1/operator/threads", json!(null)),
        (
            "POST",
            "/api/v1/operator/threads/default/turns",
            json!({"client_request_id": "unauthenticated", "prompt": "hi"}),
        ),
        (
            "POST",
            "/api/v1/operator/turns/turn-not-real/cancel",
            json!(null),
        ),
        ("POST", "/api/v1/repl", json!({"prompt": "hi"})),
        ("POST", "/api/v1/repl/stream", json!({"prompt": "hi"})),
    ] {
        let response = runtime.request(method, target, None, body);
        assert_eq!(response.status, 401, "{method} {target}: {}", response.body);
        assert_eq!(response.body["error"]["code"], "unauthorized");
    }
}

#[test]
fn missing_operator_auth_configuration_is_distinct_from_bad_credentials() {
    let runtime = TestRuntime::start(false);

    let missing = runtime.request("GET", "/api/v1/operator/threads", None, json!(null));
    assert_eq!(missing.status, 500);
    assert_eq!(missing.body["error"]["code"], "auth_not_configured");

    let configured = TestRuntime::start(true);
    let bad = configured.request(
        "GET",
        "/api/v1/operator/threads",
        Some("wrong-token"),
        json!(null),
    );
    assert_eq!(bad.status, 401);
    assert_eq!(bad.body["error"]["code"], "unauthorized");
    assert!(!bad.body.to_string().contains(TOKEN));
}

#[test]
fn operator_websocket_authenticates_before_any_upgrade() {
    let unconfigured = TestRuntime::start(false);
    let missing_config = websocket_handshake(unconfigured.port, None);
    assert_eq!(missing_config.status, 500, "{}", missing_config.head);
    assert!(!missing_config.head.contains("101 Switching Protocols"));
    assert_eq!(missing_config.body["error"]["code"], "auth_not_configured");

    let configured = TestRuntime::start(true);
    for token in [None, Some("wrong-token")] {
        let unauthorized = websocket_handshake(configured.port, token);
        assert_eq!(unauthorized.status, 401, "{}", unauthorized.head);
        assert!(!unauthorized.head.contains("101 Switching Protocols"));
        assert_eq!(unauthorized.body["error"]["code"], "unauthorized");
    }

    let authorized = websocket_handshake(configured.port, Some(TOKEN));
    assert_eq!(authorized.status, 101, "{}", authorized.head);
}

#[test]
fn authenticated_operator_routes_share_one_idempotent_runner() {
    let runtime = TestRuntime::start(true);

    let created = runtime.request(
        "POST",
        "/api/v1/operator/threads",
        Some(TOKEN),
        json!({"thread_id": "default"}),
    );
    assert_eq!(created.status, 200, "{}", created.body);
    assert_eq!(created.body["data"]["thread_id"], "default");
    let created_list = runtime.request("GET", "/api/v1/operator/threads", Some(TOKEN), json!(null));
    assert_eq!(created_list.status, 200, "{}", created_list.body);
    assert_eq!(
        created_list.body["data"]["threads"][0]["thread_id"], "default",
        "POST /threads must durably ensure the thread"
    );
    assert_eq!(created_list.body["data"]["threads"][0]["turn_count"], 0);

    let request_body = json!({
        "client_request_id": "api-idempotency-1",
        "prompt": "hi",
        "route_policy": {
            "mode": "auto",
            "minimum_quality_class": 1,
            "maximum_marginal_cost_usd": 0.0,
            "turn_budget_usd": 0.0,
            "privacy": "standard"
        }
    });
    let first = runtime.request(
        "POST",
        "/api/v1/operator/threads/default/turns",
        Some(TOKEN),
        request_body.clone(),
    );
    assert_eq!(first.status, 202, "{}", first.body);
    assert_eq!(first.body["data"]["thread_id"], "default");
    assert_eq!(first.body["data"]["duplicate"], false);
    assert!(first.body["data"]["stream_url"]
        .as_str()
        .unwrap()
        .starts_with("/ws/v1/operator?"));

    let second = runtime.request(
        "POST",
        "/api/v1/operator/threads/default/turns",
        Some(TOKEN),
        request_body,
    );
    assert_eq!(second.status, 202, "{}", second.body);
    assert_eq!(
        second.body["data"]["turn_id"],
        first.body["data"]["turn_id"]
    );
    assert_eq!(second.body["data"]["duplicate"], true);

    let turn_id = first.body["data"]["turn_id"].as_str().unwrap();
    let cancel = runtime.request(
        "POST",
        &format!("/api/v1/operator/turns/{turn_id}/cancel"),
        Some(TOKEN),
        json!(null),
    );
    assert!(
        matches!(cancel.status, 200 | 202),
        "authenticated cancel response: {}",
        cancel.body
    );
    assert_eq!(cancel.body["data"]["turn_id"], turn_id);
    assert!(cancel.body["data"]["cancel_requested"].is_boolean());

    let listed = runtime.request("GET", "/api/v1/operator/threads", Some(TOKEN), json!(null));
    assert_eq!(listed.status, 200, "{}", listed.body);
    assert_eq!(listed.body["data"]["threads"][0]["thread_id"], "default");

    let thread = runtime.request(
        "GET",
        "/api/v1/operator/threads/default",
        Some(TOKEN),
        json!(null),
    );
    assert_eq!(thread.status, 200, "{}", thread.body);
    assert_eq!(
        thread.body["data"]["thread"]["turns"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let events = runtime.request(
        "GET",
        "/api/v1/operator/threads/default/events?limit=100",
        Some(TOKEN),
        json!(null),
    );
    assert_eq!(events.status, 200, "{}", events.body);
    assert!(events.body["data"]["events"].as_array().unwrap().len() >= 3);
    assert!(events.body["data"]["next_cursor"].is_string());
}

#[test]
fn operator_boundary_rejects_bad_cursor_ids_and_turn_policy() {
    let runtime = TestRuntime::start(true);

    let bad_cursor = runtime.request(
        "GET",
        "/api/v1/operator/threads/default/events?after=not-a-cursor",
        Some(TOKEN),
        json!(null),
    );
    assert_eq!(bad_cursor.status, 400, "{}", bad_cursor.body);
    assert_eq!(bad_cursor.body["error"]["code"], "invalid_cursor");

    for hostile in ["..", "%2Ftmp", "hello%2Fworld", "a%5Cb"] {
        let response = runtime.request(
            "GET",
            &format!("/api/v1/operator/threads/{hostile}"),
            Some(TOKEN),
            json!(null),
        );
        assert_eq!(
            response.status, 400,
            "hostile id {hostile}: {}",
            response.body
        );
        assert_eq!(response.body["error"]["code"], "invalid_id");
    }
    let long_id = "a".repeat(129);
    let response = runtime.request(
        "GET",
        &format!("/api/v1/operator/threads/{long_id}"),
        Some(TOKEN),
        json!(null),
    );
    assert_eq!(response.status, 400);
    assert_eq!(response.body["error"]["code"], "invalid_id");

    for body in [json!([]), json!({"thread_id": 7})] {
        let response = runtime.request("POST", "/api/v1/operator/threads", Some(TOKEN), body);
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(response.body["error"]["code"], "invalid_request");
    }

    let sensitive_thread = runtime.request(
        "POST",
        "/api/v1/operator/threads",
        Some(TOKEN),
        json!({"thread_id": "ghp_live-token"}),
    );
    assert_eq!(sensitive_thread.status, 400, "{}", sensitive_thread.body);
    assert_eq!(sensitive_thread.body["error"]["code"], "invalid_id");

    let invalid_requests = [
        json!({"client_request_id": "", "prompt": "hi"}),
        json!({"client_request_id": "request-1", "prompt": ""}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"mode": "cheapest"}}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"mode": 7}}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"privacy": false}}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"mode": "auto", "minimum_quality_class": 0}}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"mode": "auto", "maximum_marginal_cost_usd": -0.01}}),
        json!({"client_request_id": "request-1", "prompt": "hi", "route_policy": {"mode": "auto", "turn_budget_usd": -0.01}}),
    ];
    for body in invalid_requests {
        let response = runtime.request(
            "POST",
            "/api/v1/operator/threads/default/turns",
            Some(TOKEN),
            body,
        );
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(response.body["error"]["code"], "invalid_request");
    }
}

#[test]
fn operator_stream_url_encodes_valid_reserved_thread_characters() {
    let runtime = TestRuntime::start(true);
    let created = runtime.request(
        "POST",
        "/api/v1/operator/threads",
        Some(TOKEN),
        json!({"thread_id": "team&special"}),
    );
    assert_eq!(created.status, 200, "{}", created.body);

    let submitted = runtime.request(
        "POST",
        "/api/v1/operator/threads/team%26special/turns",
        Some(TOKEN),
        json!({"client_request_id": "reserved-url-1", "prompt": "hi"}),
    );
    assert_eq!(submitted.status, 202, "{}", submitted.body);
    assert_eq!(submitted.body["data"]["thread_id"], "team&special");
    assert!(submitted.body["data"]["stream_url"]
        .as_str()
        .unwrap()
        .contains("thread_id=team%26special&"));
}

#[test]
fn model_submission_is_accepted_before_provider_preparation_and_fails_durably() {
    let runtime = TestRuntime::start_without_providers();
    let submitted = runtime.request(
        "POST",
        "/api/v1/operator/threads/default/turns",
        Some(TOKEN),
        json!({
            "client_request_id": "deferred-provider-preparation-1",
            "prompt": "Develop a detailed strategy for migrating a distributed database safely",
            "route_policy": {
                "mode": "explicit",
                "preferred_provider": "unavailable-test-provider"
            }
        }),
    );
    assert_eq!(submitted.status, 202, "{}", submitted.body);
    let turn_id = submitted.body["data"]["turn_id"].as_str().unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let events = runtime.request(
            "GET",
            "/api/v1/operator/threads/default/events?limit=100",
            Some(TOKEN),
            json!(null),
        );
        assert_eq!(events.status, 200, "{}", events.body);
        let turn_events = events.body["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["event"]["turn_id"].as_str() == Some(turn_id))
            .collect::<Vec<_>>();
        let event_types = turn_events
            .iter()
            .filter_map(|row| row["event"]["event_type"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            event_types.get(..2),
            Some(&["turn_started", "user_message"][..])
        );
        if let Some(terminal) = turn_events
            .iter()
            .find(|row| row["event"]["event_type"] == "turn_interrupted")
        {
            assert_eq!(terminal["event"]["payload"]["reason"], "EXECUTION_FAILED");
            assert!(
                terminal["event"]["payload"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("not available"),
                "{terminal}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "turn did not terminate: {event_types:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn reserve_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for_port(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("test runtime did not listen on port {port}");
}

fn request(port: u16, method: &str, target: &str, token: Option<&str>, body: &str) -> Response {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\nContent-Type: application/json\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response headers");
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap();
    let body = serde_json::from_slice(&raw[split + 4..]).unwrap_or_else(|error| {
        panic!(
            "response body was not JSON: {error}: {}",
            String::from_utf8_lossy(&raw[split + 4..])
        )
    });
    Response { status, body }
}

fn websocket_handshake(port: u16, token: Option<&str>) -> HandshakeResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET /ws/v1/operator HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n{authorization}\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();

    let mut raw = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                raw.extend_from_slice(&buffer[..read]);
                if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&raw[..split]);
                    let status = head
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1));
                    let content_length = head.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    });
                    if status == Some("101")
                        || content_length.is_some_and(|length| raw.len() >= split + 4 + length)
                    {
                        break;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => break,
            Err(error) => panic!("websocket handshake read failed: {error}"),
        }
    }
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("websocket response headers");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap();
    let body = if raw[split + 4..].is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&raw[split + 4..]).unwrap()
    };
    HandshakeResponse { status, head, body }
}
