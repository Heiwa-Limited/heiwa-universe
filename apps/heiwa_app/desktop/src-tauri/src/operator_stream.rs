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
    if !base_url.starts_with("ws://") && !base_url.starts_with("wss://") {
        return Err(OperatorStreamError::InvalidEndpoint);
    }
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

fn reconnect_delay(backoffs: &[Duration], attempt: usize) -> Duration {
    backoffs
        .get(attempt)
        .copied()
        .or_else(|| backoffs.last().copied())
        .unwrap_or(Duration::from_secs(3))
}

async fn subscribe_with_auth_and_backoff<F>(
    base_url: &str,
    thread_id: &str,
    after: Option<&str>,
    token: &str,
    mut forward: F,
    backoffs: &[Duration],
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

        match tokio_tungstenite::connect_async(request).await {
            Ok((mut websocket, _)) => {
                while let Some(message) = websocket.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            if text.contains(token) {
                                return Err(OperatorStreamError::InvalidFrame);
                            }
                            let frame: Value = serde_json::from_str(&text)
                                .map_err(|_| OperatorStreamError::InvalidFrame)?;
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
                            websocket
                                .send(Message::Pong(payload))
                                .await
                                .map_err(|_| OperatorStreamError::Unavailable)?;
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
            Err(_) => {}
        }

        let delay = reconnect_delay(backoffs, reconnect_attempt);
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
        &crate::proxy::runtime_websocket_base_url(),
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
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::Message;

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
    }
}
