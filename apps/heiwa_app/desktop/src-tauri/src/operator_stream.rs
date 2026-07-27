use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
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

#[cfg(test)]
#[derive(Debug)]
struct TestTransportDropState {
    id: u64,
    base_url: String,
    thread_id: String,
    armed: bool,
    fired: bool,
}

#[cfg(test)]
static TEST_TRANSPORT_DROP_STATE: std::sync::Mutex<Option<TestTransportDropState>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
static NEXT_TEST_TRANSPORT_DROP_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(test)]
struct TestTransportDropGuard {
    id: u64,
}

#[cfg(test)]
impl TestTransportDropGuard {
    fn install(base_url: &str, thread_id: &str) -> Self {
        use std::sync::atomic::Ordering;

        let id = NEXT_TEST_TRANSPORT_DROP_ID.fetch_add(1, Ordering::Relaxed);
        let mut state = TEST_TRANSPORT_DROP_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.is_some() {
            drop(state);
            panic!("only one native transport-drop test seam may be installed");
        }
        *state = Some(TestTransportDropState {
            id,
            base_url: base_url.to_string(),
            thread_id: thread_id.to_string(),
            armed: false,
            fired: false,
        });
        Self { id }
    }

    fn arm(&self) {
        let mut state = TEST_TRANSPORT_DROP_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = state.as_mut().filter(|state| state.id == self.id) else {
            drop(state);
            panic!("native transport-drop test seam is not installed");
        };
        assert!(!state.armed, "native transport-drop seam is already armed");
        assert!(!state.fired, "native transport-drop seam already fired");
        state.armed = true;
    }

    fn fired(&self) -> bool {
        TEST_TRANSPORT_DROP_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|state| state.id == self.id && state.fired)
    }
}

#[cfg(test)]
impl Drop for TestTransportDropGuard {
    fn drop(&mut self) {
        let mut state = TEST_TRANSPORT_DROP_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.as_ref().is_some_and(|state| state.id == self.id) {
            *state = None;
        }
    }
}

#[cfg(test)]
fn take_armed_test_transport_drop(base_url: &str, thread_id: &str) -> bool {
    let mut state = TEST_TRANSPORT_DROP_STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(state) = state.as_mut() else {
        return false;
    };
    if state.base_url != base_url || state.thread_id != thread_id || !state.armed || state.fired {
        return false;
    }
    state.fired = true;
    true
}

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
        let parsed_url = reqwest::Url::parse(request.uri().to_string().as_str())
            .map_err(|_| OperatorStreamError::InvalidEndpoint)?;
        let signed = crate::proxy::signed_local_request("GET", &parsed_url, b"", token)
            .map_err(|_| OperatorStreamError::AuthNotConfigured)?;
        let mut insert_signed_header = |name: &'static str, value: &str| {
            let mut value =
                HeaderValue::from_str(value).map_err(|_| OperatorStreamError::AuthNotConfigured)?;
            value.set_sensitive(true);
            request.headers_mut().insert(name, value);
            Ok::<(), OperatorStreamError>(())
        };
        insert_signed_header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_VERSION_HEADER,
            &signed.version,
        )?;
        insert_signed_header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_TIMESTAMP_HEADER,
            &signed.timestamp,
        )?;
        insert_signed_header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_NONCE_HEADER,
            &signed.nonce,
        )?;
        insert_signed_header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_SIGNATURE_HEADER,
            &signed.signature,
        )?;

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
                        let durable_event =
                            frame.get("type").and_then(Value::as_str) == Some("event");
                        if durable_event {
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
                        #[cfg(test)]
                        if durable_event && take_armed_test_transport_drop(base_url, thread_id) {
                            break 'connection;
                        }
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

    #[derive(Clone, Debug)]
    struct ObservedDurableEvent {
        event_id: String,
        cursor: String,
        turn_id: String,
        event_type: String,
        payload: Value,
    }

    #[derive(Default)]
    struct ExternalOperatorObservation {
        caught_up_count: usize,
        seen_event_ids: HashSet<String>,
        post_catch_up_events: Vec<ObservedDurableEvent>,
    }

    impl ExternalOperatorObservation {
        fn should_stop(&self) -> bool {
            if self.caught_up_count < 2 {
                return false;
            }
            let Some(live_turn_id) = self
                .post_catch_up_events
                .first()
                .map(|event| event.turn_id.as_str())
            else {
                return false;
            };
            self.post_catch_up_events
                .iter()
                .any(|event| event.turn_id == live_turn_id && event.event_type == "turn_completed")
        }

        fn has_exact_deterministic_route_pair(&self, turn_id: &str) -> bool {
            ["route_planned", "route_completed"]
                .into_iter()
                .all(|event_type| {
                    let mut matching = self
                        .post_catch_up_events
                        .iter()
                        .filter(|event| event.turn_id == turn_id && event.event_type == event_type);
                    matching.next().is_some_and(|event| {
                        event.payload.get("mode").and_then(Value::as_str) == Some("deterministic")
                    }) && matching.next().is_none()
                })
        }
    }

    #[test]
    fn external_operator_stop_gate_requires_reconnect_and_terminal_in_either_order() {
        let event = |event_type: &str| ObservedDurableEvent {
            event_id: format!("event-{event_type}"),
            cursor: format!("cursor-{event_type}"),
            turn_id: "turn-live".to_string(),
            event_type: event_type.to_string(),
            payload: json!({}),
        };

        let mut reconnect_first = ExternalOperatorObservation {
            caught_up_count: 2,
            ..Default::default()
        };
        reconnect_first
            .post_catch_up_events
            .push(event("turn_started"));
        assert!(!reconnect_first.should_stop());
        reconnect_first
            .post_catch_up_events
            .push(event("turn_completed"));
        assert!(reconnect_first.should_stop());

        let mut terminal_first = ExternalOperatorObservation {
            caught_up_count: 1,
            post_catch_up_events: vec![event("turn_started"), event("turn_completed")],
            ..Default::default()
        };
        assert!(!terminal_first.should_stop());
        terminal_first.caught_up_count = 2;
        assert!(terminal_first.should_stop());
    }

    #[test]
    fn external_operator_route_proof_requires_one_planned_and_one_completed() {
        let event = |event_type: &str| ObservedDurableEvent {
            event_id: format!("event-{event_type}"),
            cursor: format!("cursor-{event_type}"),
            turn_id: "turn-live".to_string(),
            event_type: event_type.to_string(),
            payload: json!({"mode": "deterministic"}),
        };
        let duplicate_planned = ExternalOperatorObservation {
            post_catch_up_events: vec![event("route_planned"), event("route_planned")],
            ..Default::default()
        };
        assert!(!duplicate_planned.has_exact_deterministic_route_pair("turn-live"));

        let complete_pair = ExternalOperatorObservation {
            post_catch_up_events: vec![event("route_planned"), event("route_completed")],
            ..Default::default()
        };
        assert!(complete_pair.has_exact_deterministic_route_pair("turn-live"));
    }

    #[tokio::test]
    #[ignore = "requires an explicitly isolated external Heiwa runtime"]
    async fn native_operator_external_runtime_replays_then_resumes_without_duplicates() {
        const WS_BASE_URL_ENV: &str = "HEIWA_OPERATOR_E2E_WS_BASE_URL";
        const TOKEN_ENV: &str = "HEIWA_OPERATOR_E2E_TOKEN";
        const THREAD_ID_ENV: &str = "HEIWA_OPERATOR_E2E_THREAD_ID";
        const START_CURSOR_ENV: &str = "HEIWA_OPERATOR_E2E_START_CURSOR";

        tokio::time::timeout(Duration::from_secs(10), async {
            let base_url = required_external_runtime_env(WS_BASE_URL_ENV);
            let token = required_external_runtime_env(TOKEN_ENV);
            let thread_id = required_external_runtime_env(THREAD_ID_ENV);
            let starting_cursor = required_external_runtime_env(START_CURSOR_ENV);
            let parsed_base = crate::proxy::validate_loopback_url(&base_url, "ws")
                .expect("external operator runtime must use an explicit loopback WS URL");
            assert_eq!(
                parsed_base.path(),
                "/",
                "external operator WS base must not contain a path"
            );
            assert!(
                parsed_base.query().is_none(),
                "external operator WS base must not contain a query"
            );
            let port = parsed_base
                .port()
                .expect("external operator WS base must specify a port");
            assert_ne!(
                port, 7474,
                "ignored external operator test must never target the installed runtime port"
            );
            operator_stream_url(&base_url, &thread_id, Some(&starting_cursor))
                .expect("external operator inputs must form a valid native stream URL");
            let http_base = format!("http://127.0.0.1:{port}");
            let transport_drop = TestTransportDropGuard::install(&base_url, &thread_id);
            let observation = Arc::new(Mutex::new(ExternalOperatorObservation::default()));
            let first_caught_up = Arc::new(tokio::sync::Notify::new());
            let first_live_event = Arc::new(tokio::sync::Notify::new());

            let stream_base = base_url.clone();
            let stream_token = token.clone();
            let stream_thread = thread_id.clone();
            let stream_cursor = starting_cursor.clone();
            let stream_observation = observation.clone();
            let stream_caught_up = first_caught_up.clone();
            let stream_live_event = first_live_event.clone();
            let subscription = tokio::spawn(async move {
                subscribe_with_auth_and_policy(
                    &stream_base,
                    &stream_thread,
                    Some(&stream_cursor),
                    &stream_token,
                    |frame| {
                        assert!(
                            !frame.to_string().contains(&stream_token),
                            "operator stream frame must not contain the bearer token"
                        );
                        match frame.get("type").and_then(Value::as_str) {
                            Some("event") => {
                                let event_id = frame
                                    .pointer("/event/event_id")
                                    .and_then(Value::as_str)
                                    .expect("durable event frame must carry event.event_id")
                                    .to_string();
                                let cursor = frame
                                    .get("cursor")
                                    .and_then(Value::as_str)
                                    .expect("durable event frame must carry cursor")
                                    .to_string();
                                let turn_id = frame
                                    .pointer("/event/turn_id")
                                    .and_then(Value::as_str)
                                    .expect("durable event frame must carry event.turn_id")
                                    .to_string();
                                let event_type = frame
                                    .pointer("/event/event_type")
                                    .and_then(Value::as_str)
                                    .expect("durable event frame must carry event.event_type")
                                    .to_string();
                                let payload = frame
                                    .pointer("/event/payload")
                                    .cloned()
                                    .expect("durable event frame must carry event.payload");
                                let mut observed = stream_observation.lock().unwrap();
                                assert!(
                                    observed.seen_event_ids.insert(event_id.clone()),
                                    "native reconnect must not redeliver a durable event_id"
                                );
                                if observed.caught_up_count > 0 {
                                    let unexpected_terminal = matches!(
                                        event_type.as_str(),
                                        "turn_failed"
                                            | "turn_cancelled"
                                            | "turn_interrupted"
                                            | "blocker"
                                    );
                                    observed.post_catch_up_events.push(ObservedDurableEvent {
                                        event_id,
                                        cursor,
                                        turn_id,
                                        event_type,
                                        payload,
                                    });
                                    if observed.post_catch_up_events.len() == 1 {
                                        stream_live_event.notify_one();
                                    }
                                    if unexpected_terminal {
                                        drop(observed);
                                        panic!(
                                            "deterministic external operator turn ended unexpectedly"
                                        );
                                    }
                                }
                                let should_stop = observed.should_stop();
                                drop(observed);
                                if should_stop {
                                    Err(OperatorStreamError::ReceiverClosed)
                                } else {
                                    Ok(())
                                }
                            }
                            Some("caught_up") => {
                                let mut observed = stream_observation.lock().unwrap();
                                observed.caught_up_count += 1;
                                if observed.caught_up_count == 1 {
                                    stream_caught_up.notify_one();
                                }
                                if observed.should_stop() {
                                    Err(OperatorStreamError::ReceiverClosed)
                                } else {
                                    Ok(())
                                }
                            }
                            Some("invalid_cursor") => {
                                panic!("external operator runtime rejected a native cursor")
                            }
                            Some("error") => {
                                panic!(
                                    "external operator runtime emitted an unexpected error frame"
                                )
                            }
                            _ => Ok(()),
                        }
                    },
                    &[Duration::from_millis(50), Duration::from_millis(100)],
                    OperatorStreamTimeouts {
                        connect: Duration::from_secs(2),
                        read_idle: Duration::from_secs(3),
                        pong_write: Duration::from_secs(1),
                    },
                )
                .await
            });

            tokio::time::timeout(Duration::from_secs(3), first_caught_up.notified())
                .await
                .expect("native subscription must reach initial caught_up");
            transport_drop.arm();

            let unique_request_id = format!(
                "native-live-tail-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock must follow the Unix epoch")
                    .as_nanos()
            );
            let submission = tokio::time::timeout(
                Duration::from_secs(3),
                crate::proxy::api_post_with_auth(
                    &http_base,
                    &format!(
                        "/api/v1/operator/threads/{}/turns",
                        percent_encode_query_component(&thread_id)
                    ),
                    json!({
                        "client_request_id": unique_request_id,
                        "prompt": "hi",
                        "route_policy": {
                            "mode": "explicit",
                            "preferred_provider": "heiwa-e2e-no-provider",
                            "maximum_marginal_cost_usd": 0.0,
                            "turn_budget_usd": 0.0
                        }
                    }),
                    &token,
                ),
            )
            .await
            .expect("native authenticated turn submission must finish")
            .unwrap_or_else(|_| panic!("native authenticated turn submission must succeed"));
            let submitted_turn_id = submission
                .pointer("/data/turn_id")
                .and_then(Value::as_str)
                .expect("turn submission must return data.turn_id")
                .to_string();

            tokio::time::timeout(Duration::from_secs(3), first_live_event.notified())
                .await
                .expect("open native subscription must receive a live durable event");
            {
                let observed = observation.lock().unwrap();
                let first_live = observed
                    .post_catch_up_events
                    .first()
                    .expect("post-catch-up durable event must be recorded");
                assert_eq!(first_live.turn_id, submitted_turn_id);
                assert_eq!(first_live.event_type, "turn_started");
                assert!(!first_live.event_id.is_empty());
                assert!(!first_live.cursor.is_empty());
            }

            let subscription_result = subscription
                .await
                .expect("native subscription task must not panic");
            assert!(
                matches!(
                    subscription_result,
                    Err(OperatorStreamError::ReceiverClosed)
                ),
                "native subscription must stop intentionally after reconnect caught_up"
            );
            assert!(
                transport_drop.fired(),
                "test seam must drop the first live durable-event transport"
            );
            let observed = observation.lock().unwrap();
            assert!(
                observed.caught_up_count >= 2,
                "native stream must reconnect and catch up before exiting"
            );
            let first_live = observed.post_catch_up_events.first().unwrap();
            assert_eq!(
                observed
                    .post_catch_up_events
                    .iter()
                    .filter(|event| event.event_id == first_live.event_id)
                    .count(),
                1,
                "reconnect must resume after the last forwarded durable cursor"
            );
            assert!(
                observed
                    .post_catch_up_events
                    .iter()
                    .skip(1)
                    .any(|event| event.turn_id == submitted_turn_id),
                "reconnected stream must deliver later durable events for the submitted turn"
            );
            assert!(
                observed.post_catch_up_events.iter().any(|event| {
                    event.turn_id == submitted_turn_id && event.event_type == "turn_completed"
                }),
                "submitted deterministic turn must reach durable turn_completed"
            );
            assert!(
                observed.has_exact_deterministic_route_pair(&submitted_turn_id),
                "deterministic turn must have exactly one planned and one completed route event"
            );
        })
        .await
        .expect("external native operator test must finish within ten seconds");
    }

    #[tokio::test]
    async fn native_operator_bridge_authenticates_and_resumes_only_durable_cursor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<(
            String,
            String,
            heiwa_core::auth::LocalRequestSignature,
        )>::new()));
        let server_requests = requests.clone();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let captured = server_requests.clone();
                let mut websocket = tokio_tungstenite::accept_hdr_async(
                    stream,
                    move |request: &Request, response: Response| {
                        let header = |name: &str| {
                            request
                                .headers()
                                .get(name)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_string()
                        };
                        captured.lock().unwrap().push((
                            request.uri().to_string(),
                            header("authorization"),
                            heiwa_core::auth::LocalRequestSignature {
                                version: header(
                                    heiwa_core::auth::LOCAL_REQUEST_AUTH_VERSION_HEADER,
                                ),
                                timestamp: header(
                                    heiwa_core::auth::LOCAL_REQUEST_AUTH_TIMESTAMP_HEADER,
                                ),
                                nonce: header(heiwa_core::auth::LOCAL_REQUEST_AUTH_NONCE_HEADER),
                                signature: header(
                                    heiwa_core::auth::LOCAL_REQUEST_AUTH_SIGNATURE_HEADER,
                                ),
                            },
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
        assert!(requests.iter().all(|request| request.1.is_empty()));
        assert_ne!(requests[0].2.nonce, requests[1].2.nonce);
        for (target, _, signed) in requests.iter() {
            heiwa_core::auth::verify_local_request(
                heiwa_core::auth::LocalRequestParts {
                    method: "GET",
                    port: address.port(),
                    target,
                    body: b"",
                },
                signed,
                "native-secret-token",
                crate::proxy::unix_timestamp_now(),
            )
            .expect("native WebSocket handshake signature must verify");
        }
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
