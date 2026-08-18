//! The whole authorization-code flow, end to end, against a mock provider.
//!
//! This is the contract the L3 spec commits to: the flow is proven in CI with
//! no Google account, no client id, and no network. Everything the real
//! provider does is stood up locally.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use heiwa_oauth::{
    build_authorization_request, exchange_code, LoopbackListener, OAuthError, ProviderConfig,
};

/// Minimal token endpoint. Captures the form body it received so the test can
/// assert the client sent the verifier, then answers with a token set.
fn spawn_token_endpoint(response_body: &'static str, status: u16) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&mut stream);

        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }

        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).unwrap();
        tx.send(String::from_utf8_lossy(&body).into_owned()).unwrap();

        let reason = if status == 200 { "OK" } else { "Bad Request" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    (format!("http://127.0.0.1:{port}"), rx)
}

fn config(token_endpoint: String) -> ProviderConfig {
    ProviderConfig {
        auth_endpoint: "https://provider.test/authorize".into(),
        token_endpoint,
        client_id: "test-client".into(),
        scopes: vec!["https://www.googleapis.com/auth/calendar.readonly".into()],
    }
}

#[tokio::test]
async fn authorization_code_flow_completes_without_a_real_provider() {
    let (token_endpoint, received) = spawn_token_endpoint(
        r#"{"access_token":"access-1","refresh_token":"refresh-1","expires_in":3599,"token_type":"Bearer"}"#,
        200,
    );
    let config = config(token_endpoint);

    let listener = LoopbackListener::bind().unwrap();
    let redirect_uri = listener.redirect_uri().to_string();
    let request = build_authorization_request(&config, &redirect_uri).unwrap();

    // Stand in for the browser: the provider redirects here after consent.
    let callback_url = format!("{}/?code=auth-code-1&state={}", redirect_uri, request.state);
    let browser = thread::spawn(move || {
        let mut attempts = 0;
        loop {
            match reqwest::blocking::get(&callback_url) {
                Ok(_) => break,
                Err(_) if attempts < 50 => {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("callback never reached the listener: {error}"),
            }
        }
    });

    let code = listener
        .wait_for_code(&request.state, Duration::from_secs(5))
        .expect("listener should accept the matching callback");
    browser.join().unwrap();
    assert_eq!(code, "auth-code-1");

    let client = reqwest::Client::new();
    let tokens = exchange_code(
        &client,
        &config,
        &code,
        request.pkce.verifier(),
        &redirect_uri,
    )
    .await
    .expect("exchange should succeed");

    assert_eq!(tokens.access_token, "access-1");
    assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-1"));

    // The verifier must actually reach the token endpoint. Without this the
    // flow would still pass every other assertion while PKCE did nothing.
    // Decode rather than substring-match: the body is form-encoded, so the
    // verifier's `~` arrives as %7E and a raw comparison would fail on a
    // request that was in fact correct.
    let body = received.recv_timeout(Duration::from_secs(5)).unwrap();
    let fields: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(body.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

    assert_eq!(
        fields.get("code_verifier").map(String::as_str),
        Some(request.pkce.verifier()),
        "exchange body did not carry the verifier: {body}"
    );
    assert_eq!(
        fields.get("grant_type").map(String::as_str),
        Some("authorization_code")
    );
    assert_eq!(fields.get("code").map(String::as_str), Some("auth-code-1"));
    assert_eq!(
        fields.get("redirect_uri").map(String::as_str),
        Some(redirect_uri.as_str())
    );
    // A public client must not be sending a secret it does not have.
    assert!(!fields.contains_key("client_secret"));
}

#[tokio::test]
async fn a_rejected_exchange_reports_the_status_not_a_parse_failure() {
    let (token_endpoint, _received) = spawn_token_endpoint(
        r#"{"error":"invalid_grant","error_description":"Bad Request"}"#,
        400,
    );
    let config = config(token_endpoint);

    let error = exchange_code(
        &reqwest::Client::new(),
        &config,
        "expired-code",
        "verifier",
        "http://127.0.0.1:1",
    )
    .await
    .expect_err("a 400 must not be treated as a token set");

    match error {
        OAuthError::TokenEndpoint { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("invalid_grant"));
        }
        other => panic!("expected a classified endpoint error, got {other:?}"),
    }
}
