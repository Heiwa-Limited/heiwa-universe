use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

const RECONNECT_BACKOFF: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(3),
];

#[derive(Clone, Copy)]
struct OperatorStreamTimeouts {
    connect: Duration,
    read_idle: Duration,
    pong_write: Duration,
}

const OPERATOR_STREAM_TIMEOUTS: OperatorStreamTimeouts = OperatorStreamTimeouts {
    connect: Duration::from_secs(10),
    read_idle: Duration::from_secs(45),
    pong_write: Duration::from_secs(10),
};

#[derive(Debug, Error)]
enum OperatorStreamError {
    #[error("runtime authentication is not configured")]
    AuthNotConfigured,
    #[error("invalid operator stream endpoint")]
    InvalidEndpoint,
    #[error("invalid operator thread identifier")]
    InvalidThread,
    #[error("operator stream unavailable")]
    Unavailable,
    #[error("operator stream returned an invalid frame")]
    InvalidFrame,
    #[error("operator stream handshake rejected with HTTP {status}")]
    HandshakeRejected { status: u16 },
    #[error("operator stream receiver closed")]
    ReceiverClosed,
}

fn operator_stream_url(
    base_url: &str,
    thread_id: &str,
    after: Option<&str>,
) -> Result<String, OperatorStreamError> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty()
        || thread_id.len() > 128
        || thread_id.contains("..")
        || thread_id.contains('/')
        || thread_id.contains('\\')
        || thread_id.chars().any(char::is_control)
    {
        return Err(OperatorStreamError::InvalidThread);
    }
    crate::proxy::validate_loopback_url(base_url, "ws")
        .map_err(|_| OperatorStreamError::InvalidEndpoint)?;
    if after.is_some_and(|cursor| cursor.len() > 8 * 1024 || cursor.chars().any(char::is_control)) {
        return Err(OperatorStreamError::InvalidEndpoint);
    }
    let mut url = format!(
        "{}/ws/v1/operator?thread_id={}",
        base_url.trim_end_matches('/'),
        percent_encode_query_component(thread_id)
    );
    if let Some(after) = after.filter(|cursor| !cursor.is_empty()) {
        url.push_str("&after=");
        url.push_str(&percent_encode_query_component(after));
    }
    crate::proxy::validate_loopback_url(&url, "ws")
        .map_err(|_| OperatorStreamError::InvalidEndpoint)?;
    Ok(url)
}

fn percent_encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

async fn subscribe_with_auth_and_backoff<F>(
    base_url: &str,
    thread_id: &str,
    after: Option<&str>,
    token: &str,
    forward: F,
    backoffs: &[Duration],
) -> Result<(), OperatorStreamError>
where
    F: FnMut(Value) -> Result<(), OperatorStreamError>,
{
    subscribe_with_auth_and_policy(
        base_url,
        thread_id,
        after,
        token,
        forward,
        backoffs,
        OPERATOR_STREAM_TIMEOUTS,
    )
    .await
}

async fn subscribe_with_auth_and_policy<F>(
    base_url: &str,
    thread_id: &str,
    after: Option<&str>,
    token: &str,
    mut forward: F,
    backoffs: &[Duration],
    timeouts: OperatorStreamTimeouts,
) -> Result<(), OperatorStreamError>
where
    F: FnMut(Value) -> Result<(), OperatorStreamError>,
{
    if token.trim().is_empty() {
        return Err(OperatorStreamError::AuthNotConfigured);
    }
    let mut cursor = after.map(str::to_string);
    let mut reconnect_attempt = 0usize;

    loop {
        let url = operator_stream_url(base_url, thread_id, cursor.as_deref())?;
        let mut request = url
            .into_client_request()
            .map_err(|_| OperatorStreamError::InvalidEndpoint)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| OperatorStreamError::AuthNotConfigured)?;
        authorization.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, authorization);

        match tokio::time::timeout(timeouts.connect, tokio_tungstenite::connect_async(request))
            .await
        {
            Ok(Ok((mut websocket, _))) => 'connection: loop {
                let message = match tokio::time::timeout(timeouts.read_idle, websocket.next()).await
                {
                    Ok(Some(message)) => message,
                    Ok(None) | Err(_) => break,
                };
                match message {
                    Ok(Message::Text(text)) => {
                        let frame: Value = serde_json::from_str(&text)
                            .map_err(|_| OperatorStreamError::InvalidFrame)?;
                        if text.contains(token)
                            || crate::proxy::value_contains_secret(&frame, token)
                        {
                            return Err(OperatorStreamError::InvalidFrame);
                        }
                        if frame.get("type").and_then(Value::as_str) == Some("event") {
                            let next_cursor = frame
                                .get("cursor")
                                .and_then(Value::as_str)
                                .filter(|cursor| !cursor.is_empty())
                                .ok_or(OperatorStreamError::InvalidFrame)?;
                            cursor = Some(next_cursor.to_string());
                        }
                        let invalid_cursor =
                            frame.get("type").and_then(Value::as_str) == Some("invalid_cursor");
                        let caught_up =
                            frame.get("type").and_then(Value::as_str) == Some("caught_up");
                        forward(frame)?;
                        if invalid_cursor {
                            return Ok(());
                        }
                        if caught_up {
                            reconnect_attempt = 0;
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        if !matches!(
                            tokio::time::timeout(
                                timeouts.pong_write,
                                websocket.send(Message::Pong(payload)),
                            )
                            .await,
                            Ok(Ok(()))
                        ) {
                            break 'connection;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            },
            Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
                return Err(OperatorStreamError::HandshakeRejected {
                    status: response.status().as_u16(),
                });
            }
            Ok(Err(
                tokio_tungstenite::tungstenite::Error::Url(_)
                | tokio_tungstenite::tungstenite::Error::HttpFormat(_),
            )) => {
                return Err(OperatorStreamError::InvalidEndpoint);
            }
            Ok(Err(_)) | Err(_) => {}
        }

        let Some(delay) = backoffs.get(reconnect_attempt).copied() else {
            return Err(OperatorStreamError::Unavailable);
        };
        reconnect_attempt = reconnect_attempt.saturating_add(1);
        tokio::time::sleep(delay).await;
    }
}

fn stream_api_error(error: OperatorStreamError) -> crate::proxy::ApiErrorPayload {
    match error {
        OperatorStreamError::AuthNotConfigured => crate::proxy::ApiErrorPayload::AuthNotConfigured,
        OperatorStreamError::InvalidThread | OperatorStreamError::InvalidEndpoint => {
            crate::proxy::ApiErrorPayload::InvalidPath(error.to_string())
        }
        OperatorStreamError::ReceiverClosed => {
            crate::proxy::ApiErrorPayload::Offline("operator stream receiver closed".to_string())
        }
        OperatorStreamError::Unavailable | OperatorStreamError::InvalidFrame => {
            crate::proxy::ApiErrorPayload::Offline("operator stream unavailable".to_string())
        }
        OperatorStreamError::HandshakeRejected { status } => crate::proxy::ApiErrorPayload::Http {
            status,
            body: "operator websocket handshake rejected".to_string(),
        },
    }
}

#[tauri::command]
pub async fn operator_subscribe(
    thread_id: String,
    after: Option<String>,
    on_event: tauri::ipc::Channel<Value>,
) -> Result<(), crate::proxy::ApiErrorPayload> {
    let token = crate::proxy::machine_auth_token().map_err(crate::proxy::ApiErrorPayload::from)?;
    subscribe_with_auth_and_backoff(
        &crate::proxy::runtime_websocket_base_url().map_err(crate::proxy::ApiErrorPayload::from)?,
        &thread_id,
        after.as_deref(),
        &token,
        move |frame| {
            on_event
                .send(frame)
                .map_err(|_| OperatorStreamError::ReceiverClosed)
        },
        &RECONNECT_BACKOFF,
    )
    .await
    .map_err(stream_api_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::Message;

    fn required_external_runtime_env(name: &str) -> String {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                panic!("{name} must be set for ignored external operator runtime test")
            })
    }

    #[tokio::test]
    #[ignore = "requires an explicitly isolated external Heiwa runtime"]
    async fn native_operator_external_runtime_replays_then_resumes_without_duplicates() {
        const WS_BASE_URL_ENV: &str = "HEIWA_OPERATOR_E2E_WS_BASE_URL";
        const TOKEN_ENV: &str = "HEIWA_OPERATOR_E2E_TOKEN";
        const THREAD_ID_ENV: &str = "HEIWA_OPERATOR_E2E_THREAD_ID";
        const START_CURSOR_ENV: &str = "HEIWA_OPERATOR_E2E_START_CURSOR";

        let base_url = required_external_runtime_env(WS_BASE_URL_ENV);
        let token = required_external_runtime_env(TOKEN_ENV);
        let thread_id = required_external_runtime_env(THREAD_ID_ENV);
        let starting_cursor = required_external_runtime_env(START_CURSOR_ENV);
        let timeouts = OperatorStreamTimeouts {
            connect: Duration::from_secs(2),
            read_idle: Duration::from_secs(2),
            pong_write: Duration::from_secs(1),
        };
        let backoffs = [Duration::from_millis(50)];

        let mut first_event_ids = HashSet::new();
        let mut first_durable_cursors = Vec::new();
        let mut first_caught_up = false;
        let first_result = tokio::time::timeout(
            Duration::from_secs(5),
            subscribe_with_auth_and_policy(
                &base_url,
                &thread_id,
                Some(&starting_cursor),
                &token,
                |frame| {
                    assert!(
                        !frame.to_string().contains(&token),
                        "operator stream frame must not contain the bearer token"
                    );
                    match frame.get("type").and_then(Value::as_str) {
                        Some("event") => {
                            let event_id = frame
                                .pointer("/event/event_id")
                                .and_then(Value::as_str)
                                .expect("durable event frame must carry event.event_id");
                            let cursor = frame
                                .get("cursor")
                                .and_then(Value::as_str)
                                .expect("durable event frame must carry cursor");
                            assert!(
                                first_event_ids.insert(event_id.to_string()),
                                "first replay must not contain duplicate event_id values"
                            );
                            first_durable_cursors.push(cursor.to_string());
                            Ok(())
                        }
                        Some("caught_up") => {
                            first_caught_up = true;
                            Err(OperatorStreamError::ReceiverClosed)
                        }
                        Some("invalid_cursor") => {
                            panic!("supplied external runtime cursor must be valid")
                        }
                        _ => Ok(()),
                    }
                },
                &backoffs,
                timeouts,
            ),
        )
        .await
        .expect("first external operator subscription must finish within five seconds");
        assert!(
            matches!(&first_result, Err(OperatorStreamError::ReceiverClosed)),
            "first subscription must stop intentionally after caught_up, got {first_result:?}"
        );
        assert!(
            first_caught_up,
            "first subscription must authenticate and catch up"
        );
        assert!(
            !first_event_ids.is_empty(),
            "first subscription must replay at least one durable event"
        );
        let resume_cursor = first_durable_cursors
            .last()
            .expect("first replay must produce a resume cursor")
            .clone();

        let mut resumed_event_ids = HashSet::new();
        let mut second_caught_up = false;
        let second_result = tokio::time::timeout(
            Duration::from_secs(5),
            subscribe_with_auth_and_policy(
                &base_url,
                &thread_id,
                Some(&resume_cursor),
                &token,
                |frame| {
                    assert!(
                        !frame.to_string().contains(&token),
                        "operator stream frame must not contain the bearer token"
                    );
                    match frame.get("type").and_then(Value::as_str) {
                        Some("event") => {
                            let event_id = frame
                                .pointer("/event/event_id")
                                .and_then(Value::as_str)
                                .expect("durable event frame must carry event.event_id");
                            resumed_event_ids.insert(event_id.to_string());
                            Ok(())
                        }
                        Some("caught_up") => {
                            second_caught_up = true;
                            Err(OperatorStreamError::ReceiverClosed)
                        }
                        Some("invalid_cursor") => {
                            panic!("native resume cursor must remain valid")
                        }
                        _ => Ok(()),
                    }
                },
                &backoffs,
                timeouts,
            ),
        )
        .await
        .expect("second external operator subscription must finish within five seconds");
        assert!(
            matches!(&second_result, Err(OperatorStreamError::ReceiverClosed)),
            "second subscription must stop intentionally after caught_up, got {second_result:?}"
        );
        assert!(
            second_caught_up,
            "second subscription must authenticate and catch up"
        );
        assert!(
            resumed_event_ids.is_empty(),
            "resuming from the last durable cursor must replay zero durable events"
        );
    }

    #[tokio::test]
    async fn native_operator_bridge_authenticates_and_resumes_only_durable_cursor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let server_requests = requests.clone();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let captured = server_requests.clone();
                let mut websocket = tokio_tungstenite::accept_hdr_async(
                    stream,
                    move |request: &Request, response: Response| {
                        captured.lock().unwrap().push((
                            request.uri().to_string(),
                            request
                                .headers()
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_string(),
                        ));
                        Ok(response)
                    },
                )
                .await
                .unwrap();
                if attempt == 0 {
                    websocket
                        .send(Message::Text(
                            json!({"type":"assistant_delta","thread_id":"team&special","turn_id":"t1","text":"x"})
                                .to_string(),
                        ))
                        .await
                        .unwrap();
                    websocket
                        .send(Message::Text(
                            json!({"type":"event","cursor":"cursor&A","event":{"event_id":"e1"}})
                                .to_string(),
                        ))
                        .await
                        .unwrap();
                    websocket.close(None).await.unwrap();
                } else {
                    websocket
                        .send(Message::Text(json!({"type":"caught_up"}).to_string()))
                        .await
                        .unwrap();
                    websocket
                        .send(Message::Text(
                            json!({"type":"invalid_cursor","code":"invalid_cursor"}).to_string(),
                        ))
                        .await
                        .unwrap();
                    websocket.close(None).await.unwrap();
                }
            }
        });

        let forwarded = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = forwarded.clone();
        subscribe_with_auth_and_backoff(
            &format!("ws://{address}"),
            "team&special",
            None,
            "native-secret-token",
            move |frame| {
                sink.lock().unwrap().push(frame);
                Ok(())
            },
            &[Duration::from_millis(1)],
        )
        .await
        .expect("bridge stops cleanly on invalid cursor");
        server.await.unwrap();

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].1, "Bearer native-secret-token");
        assert_eq!(requests[1].1, "Bearer native-secret-token");
        assert!(requests[0].0.contains("thread_id=team%26special"));
        assert!(!requests[0].0.contains("after="));
        assert!(requests[1].0.contains("after=cursor%26A"));

        let forwarded = forwarded.lock().unwrap();
        assert_eq!(forwarded[0]["type"], "assistant_delta");
        assert_eq!(forwarded[1]["cursor"], "cursor&A");
        assert_eq!(forwarded.last().unwrap()["type"], "invalid_cursor");
    }

    #[tokio::test]
    async fn native_operator_bridge_rejects_token_echo_before_channel() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();
            websocket
                .send(Message::Text(
                    json!({"type":"error","message":"native-secret-token"}).to_string(),
                ))
                .await
                .unwrap();
        });
        let forwarded = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = forwarded.clone();
        let error = subscribe_with_auth_and_backoff(
            &format!("ws://{address}"),
            "default",
            None,
            "native-secret-token",
            move |frame| {
                sink.lock().unwrap().push(frame);
                Ok(())
            },
            &[Duration::from_millis(1)],
        )
        .await
        .expect_err("token echo must be rejected");
        server.await.unwrap();
        assert!(matches!(error, OperatorStreamError::InvalidFrame));
        assert!(forwarded.lock().unwrap().is_empty());
        assert!(!error.to_string().contains("native-secret-token"));
    }

    #[test]
    fn native_operator_url_rejects_hostile_ids_and_encodes_opaque_values() {
        assert!(operator_stream_url("ws://127.0.0.1:7474", "../escape", None).is_err());
        let url = operator_stream_url(
            "ws://127.0.0.1:7474",
            "team&special",
            Some("opaque+/=&cursor"),
        )
        .expect("safe encoded URL");
        assert_eq!(
            url,
            "ws://127.0.0.1:7474/ws/v1/operator?thread_id=team%26special&after=opaque%2B%2F%3D%26cursor"
        );
        assert!(operator_stream_url("ws://evil.example:7474", "default", None).is_err());
    }

    #[tokio::test]
    async fn native_operator_handshake_rejection_is_terminal_and_safe() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).await.unwrap();
            let body = r#"{"error":"native-secret-token"}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let error = tokio::time::timeout(
            Duration::from_millis(100),
            subscribe_with_auth_and_backoff(
                &format!("ws://{address}"),
                "default",
                None,
                "native-secret-token",
                |_| Ok(()),
                &[Duration::from_millis(1)],
            ),
        )
        .await
        .expect("handshake rejection must not retry")
        .expect_err("401 is terminal");
        server.await.unwrap();
        assert!(matches!(
            error,
            OperatorStreamError::HandshakeRejected { status: 401 }
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(!error.to_string().contains("native-secret-token"));
    }

    #[tokio::test]
    async fn native_operator_offline_retries_are_bounded() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                server_attempts.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        let error = tokio::time::timeout(
            Duration::from_millis(100),
            subscribe_with_auth_and_backoff(
                &format!("ws://{address}"),
                "default",
                None,
                "native-token",
                |_| Ok(()),
                &[Duration::from_millis(1), Duration::from_millis(1)],
            ),
        )
        .await
        .expect("retry lifecycle is bounded")
        .expect_err("offline retries exhaust");
        server.await.unwrap();
        assert!(matches!(error, OperatorStreamError::Unavailable));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn native_operator_scans_decoded_json_before_channel_delivery() {
        let token = "native\\token\"quote";
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();
            websocket
                .send(Message::Text(
                    serde_json::to_string(&json!({"type":"error","nested":{"value":token}}))
                        .unwrap(),
                ))
                .await
                .unwrap();
        });
        let forwarded = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = forwarded.clone();
        let error = subscribe_with_auth_and_backoff(
            &format!("ws://{address}"),
            "default",
            None,
            token,
            move |frame| {
                sink.lock().unwrap().push(frame);
                Ok(())
            },
            &[Duration::from_millis(1)],
        )
        .await
        .expect_err("decoded token echo rejected");
        server.await.unwrap();
        assert!(matches!(error, OperatorStreamError::InvalidFrame));
        assert!(forwarded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn native_operator_stalled_handshakes_exhaust_finite_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();
        let server = tokio::spawn(async move {
            let mut held = Vec::new();
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                server_attempts.fetch_add(1, Ordering::SeqCst);
                held.push(stream);
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        });
        let token = "stalled-handshake-secret";
        let error = tokio::time::timeout(
            Duration::from_millis(100),
            subscribe_with_auth_and_policy(
                &format!("ws://{address}"),
                "default",
                None,
                token,
                |_| Ok(()),
                &[Duration::from_millis(1), Duration::from_millis(1)],
                OperatorStreamTimeouts {
                    connect: Duration::from_millis(5),
                    read_idle: Duration::from_millis(20),
                    pong_write: Duration::from_millis(5),
                },
            ),
        )
        .await
        .expect("stalled handshakes terminate")
        .expect_err("connect budget exhausts");
        server.await.unwrap();
        assert!(matches!(error, OperatorStreamError::Unavailable));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(!error.to_string().contains(token));
    }

    #[tokio::test]
    async fn native_operator_idle_connection_reconnects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();
        let server = tokio::spawn(async move {
            let (first, _) = listener.accept().await.unwrap();
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let first = tokio_tungstenite::accept_async(first).await.unwrap();
            let held = tokio::spawn(async move {
                let _first = first;
                tokio::time::sleep(Duration::from_millis(30)).await;
            });

            let (second, _) = listener.accept().await.unwrap();
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let mut second = tokio_tungstenite::accept_async(second).await.unwrap();
            second
                .send(Message::Text(json!({"type":"invalid_cursor"}).to_string()))
                .await
                .unwrap();
            held.await.unwrap();
        });
        tokio::time::timeout(
            Duration::from_millis(100),
            subscribe_with_auth_and_policy(
                &format!("ws://{address}"),
                "default",
                None,
                "idle-secret",
                |_| Ok(()),
                &[Duration::from_millis(1)],
                OperatorStreamTimeouts {
                    connect: Duration::from_millis(20),
                    read_idle: Duration::from_millis(5),
                    pong_write: Duration::from_millis(5),
                },
            ),
        )
        .await
        .expect("idle reconnect terminates")
        .expect("invalid cursor ends subscription safely");
        server.await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
