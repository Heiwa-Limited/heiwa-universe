use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use thiserror::Error;

const DEFAULT_RUNTIME_PORT: &str = "7474";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail")]
pub enum ApiErrorPayload {
    Offline(String),
    Http { status: u16, body: String },
    Decode(String),
    InvalidPath(String),
    AuthNotConfigured,
}

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("runtime authentication is not configured")]
    AuthNotConfigured,
    #[error("invalid local runtime endpoint")]
    InvalidEndpoint,
    #[error("runtime response contained protected authentication material")]
    ProtectedMaterial,
    #[error("runtime offline: {0}")]
    Offline(String),
    #[error("runtime returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("could not decode runtime response: {0}")]
    Decode(String),
    #[error("invalid runtime API path: {0}")]
    InvalidPath(String),
}

impl From<ProxyError> for ApiErrorPayload {
    fn from(error: ProxyError) -> Self {
        match error {
            ProxyError::AuthNotConfigured => Self::AuthNotConfigured,
            ProxyError::InvalidEndpoint => {
                Self::InvalidPath("invalid local runtime endpoint".to_string())
            }
            ProxyError::ProtectedMaterial => Self::Decode(
                "runtime response contained protected authentication material".to_string(),
            ),
            ProxyError::Offline(message) => Self::Offline(message),
            ProxyError::Http { status, body } => Self::Http { status, body },
            ProxyError::Decode(message) => Self::Decode(message),
            ProxyError::InvalidPath(message) => Self::InvalidPath(message),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub reachable: bool,
    pub snapshot: Option<Value>,
    pub error: Option<ApiErrorPayload>,
}

pub(crate) fn runtime_base_url() -> Result<String, ProxyError> {
    let configured = match env::var("HEIWA_APP_PORT") {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => return Err(ProxyError::InvalidEndpoint),
    };
    runtime_base_url_from_port(configured.as_deref())
}

fn runtime_base_url_from_port(configured: Option<&str>) -> Result<String, ProxyError> {
    let raw = configured.unwrap_or(DEFAULT_RUNTIME_PORT);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProxyError::InvalidEndpoint);
    }
    let port = raw
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or(ProxyError::InvalidEndpoint)?;
    Ok(format!("http://127.0.0.1:{port}"))
}

pub(crate) fn runtime_websocket_base_url() -> Result<String, ProxyError> {
    Ok(runtime_base_url()?.replacen("http://", "ws://", 1))
}

pub(crate) fn machine_auth_token() -> Result<String, ProxyError> {
    let token = env::var("HEIWA_MACHINE_AUTH_TOKEN")
        .or_else(|_| env::var("HEIWA_AUTH_TOKEN"))
        .unwrap_or_default();
    validate_auth_token(&token)?;
    Ok(token)
}

fn validate_auth_token(token: &str) -> Result<(), ProxyError> {
    if token.trim().is_empty() {
        return Err(ProxyError::AuthNotConfigured);
    }
    Ok(())
}

pub(crate) fn validate_loopback_url(
    raw: &str,
    expected_scheme: &str,
) -> Result<reqwest::Url, ProxyError> {
    let url = reqwest::Url::parse(raw).map_err(|_| ProxyError::InvalidEndpoint)?;
    if url.scheme() != expected_scheme
        || url.host_str() != Some("127.0.0.1")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_none()
        || url.fragment().is_some()
    {
        return Err(ProxyError::InvalidEndpoint);
    }
    Ok(url)
}

fn endpoint_url(base_url: &str, path: &str) -> Result<String, ProxyError> {
    if !path.starts_with("/api/v1/") && path != "/status/health" {
        return Err(ProxyError::InvalidPath(path.to_string()));
    }
    let base = validate_loopback_url(base_url, "http")?;
    if base.path() != "/" || base.query().is_some() {
        return Err(ProxyError::InvalidEndpoint);
    }
    let final_url = base.join(path).map_err(|_| ProxyError::InvalidEndpoint)?;
    validate_loopback_url(final_url.as_str(), "http")?;
    Ok(final_url.to_string())
}

pub(crate) fn value_contains_secret(value: &Value, secret: &str) -> bool {
    match value {
        Value::String(value) => value.contains(secret),
        Value::Array(values) => values
            .iter()
            .any(|value| value_contains_secret(value, secret)),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.contains(secret) || value_contains_secret(value, secret)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn decode_protected_json(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

fn reject_protected_response(
    text: &str,
    decoded: Option<&Value>,
    token: &str,
) -> Result<(), ProxyError> {
    if text.contains(token) || decoded.is_some_and(|value| value_contains_secret(value, token)) {
        return Err(ProxyError::ProtectedMaterial);
    }
    Ok(())
}

fn authenticated_http_client() -> Result<reqwest::Client, ProxyError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ProxyError::Offline("runtime client unavailable".to_string()))
}

pub(crate) async fn api_get_with_auth(
    base_url: &str,
    path: &str,
    token: &str,
) -> Result<Value, ProxyError> {
    validate_auth_token(token)?;
    let url = endpoint_url(base_url, path)?;
    let response = authenticated_http_client()?
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| ProxyError::Offline("runtime request failed".to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|_| ProxyError::Offline("runtime response failed".to_string()))?;
    let decoded = decode_protected_json(&body);
    reject_protected_response(&body, decoded.as_ref().ok(), token)?;
    if !status.is_success() {
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body,
        });
    }
    decoded.map_err(|error| ProxyError::Decode(error.to_string()))
}

pub(crate) async fn api_post_with_auth(
    base_url: &str,
    path: &str,
    body: Value,
    token: &str,
) -> Result<Value, ProxyError> {
    validate_auth_token(token)?;
    let url = endpoint_url(base_url, path)?;
    let response = authenticated_http_client()?
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|_| ProxyError::Offline("runtime request failed".to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|_| ProxyError::Offline("runtime response failed".to_string()))?;
    let decoded = decode_protected_json(&text);
    reject_protected_response(&text, decoded.as_ref().ok(), token)?;
    if !status.is_success() {
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body: text,
        });
    }
    decoded.map_err(|error| ProxyError::Decode(error.to_string()))
}

#[tauri::command]
pub async fn api_get(path: String) -> Result<Value, ApiErrorPayload> {
    let token = machine_auth_token().map_err(ApiErrorPayload::from)?;
    let base = runtime_base_url().map_err(ApiErrorPayload::from)?;
    api_get_with_auth(&base, &path, &token)
        .await
        .map_err(ApiErrorPayload::from)
}

#[tauri::command]
pub async fn api_post(path: String, body: Value) -> Result<Value, ApiErrorPayload> {
    let token = machine_auth_token().map_err(ApiErrorPayload::from)?;
    let base = runtime_base_url().map_err(ApiErrorPayload::from)?;
    api_post_with_auth(&base, &path, body, &token)
        .await
        .map_err(ApiErrorPayload::from)
}

#[tauri::command]
pub async fn runtime_health() -> RuntimeHealth {
    let token = match machine_auth_token() {
        Ok(token) => token,
        Err(error) => {
            return RuntimeHealth {
                reachable: false,
                snapshot: None,
                error: Some(ApiErrorPayload::from(error)),
            };
        }
    };
    let base = match runtime_base_url() {
        Ok(base) => base,
        Err(error) => {
            return RuntimeHealth {
                reachable: false,
                snapshot: None,
                error: Some(ApiErrorPayload::from(error)),
            };
        }
    };
    match api_get_with_auth(&base, "/api/v1/runtime/snapshot", &token).await {
        Ok(snapshot) => RuntimeHealth {
            reachable: true,
            snapshot: Some(snapshot),
            error: None,
        },
        Err(error) => RuntimeHealth {
            reachable: false,
            snapshot: None,
            error: Some(ApiErrorPayload::from(error)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn owned_stub_server(status: &str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub addr");
        let status = status.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{addr}")
    }

    fn stub_server(status: &str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub addr");
        let status = status.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        format!("http://{}", addr)
    }

    fn inspecting_stub_server(
        status: &str,
        body: &'static str,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub addr");
        let status = status.to_string();
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).expect("read request");
            request_tx
                .send(String::from_utf8_lossy(&buffer[..read]).to_string())
                .expect("capture request");
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        (format!("http://{}", addr), request_rx)
    }

    #[tokio::test]
    async fn api_get_with_base_returns_runtime_json() {
        let base = stub_server(
            "200 OK",
            r#"{"ok":true,"data":{"runtime_version":"0.1.0"}}"#,
        );
        let value = api_get_with_auth(&base, "/api/v1/runtime/snapshot", "desktop-token")
            .await
            .expect("json response");
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["data"]["runtime_version"], json!("0.1.0"));
    }

    #[tokio::test]
    async fn api_get_with_base_rejects_non_runtime_paths() {
        let error = api_get_with_auth("http://127.0.0.1:1", "/etc/passwd", "desktop-token")
            .await
            .expect_err("invalid path rejected before network");
        assert!(matches!(error, ProxyError::InvalidPath(_)));
    }

    #[tokio::test]
    async fn api_get_with_base_reports_http_errors() {
        let base = stub_server("503 Service Unavailable", r#"{"error":"down"}"#);
        let error = api_get_with_auth(&base, "/api/v1/runtime/snapshot", "desktop-token")
            .await
            .expect_err("http error");
        assert!(matches!(error, ProxyError::Http { status: 503, .. }));
    }

    #[tokio::test]
    async fn native_get_and_post_send_machine_authorization() {
        let (get_base, get_request) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        api_get_with_auth(&get_base, "/api/v1/operator/threads", "desktop-token")
            .await
            .expect("authorized get");
        assert!(get_request
            .recv()
            .expect("get request")
            .contains("authorization: Bearer desktop-token"));

        let (post_base, post_request) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        api_post_with_auth(
            &post_base,
            "/api/v1/operator/threads",
            json!({"thread_id":"default"}),
            "desktop-token",
        )
        .await
        .expect("authorized post");
        assert!(post_request
            .recv()
            .expect("post request")
            .contains("authorization: Bearer desktop-token"));
    }

    #[tokio::test]
    async fn missing_native_auth_fails_before_network_without_token_leakage() {
        let error = api_get_with_auth("http://127.0.0.1:1", "/api/v1/operator/threads", "  ")
            .await
            .expect_err("empty auth must fail before connect");
        assert!(matches!(error, ProxyError::AuthNotConfigured));
        assert!(!error.to_string().contains("Bearer"));
    }

    #[test]
    fn runtime_port_is_strictly_parsed_before_url_construction() {
        assert_eq!(
            runtime_base_url_from_port(None).unwrap(),
            "http://127.0.0.1:7474"
        );
        assert_eq!(
            runtime_base_url_from_port(Some("7475")).unwrap(),
            "http://127.0.0.1:7475"
        );
        for hostile in ["7474@evil.example", "0", "-1", "65536", " 7474"] {
            assert!(matches!(
                runtime_base_url_from_port(Some(hostile)),
                Err(ProxyError::InvalidEndpoint)
            ));
        }
    }

    #[tokio::test]
    async fn authenticated_http_rejects_external_final_host_before_token_egress() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener as TokioTcpListener;

        let listener = TokioTcpListener::bind("127.0.0.1:0").await.unwrap();
        let decoy = listener.local_addr().unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let server_observed = observed.clone();
        let server = tokio::spawn(async move {
            if let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept()).await
            {
                server_observed.store(true, Ordering::SeqCst);
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                    .await;
            }
        });
        let hostile = format!("http://127.0.0.1:7474@{decoy}");
        let error = api_get_with_auth(&hostile, "/api/v1/operator/threads", "egress-token")
            .await
            .expect_err("external final host rejected");
        assert!(matches!(error, ProxyError::InvalidEndpoint));
        server.await.unwrap();
        assert!(!observed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn authenticated_http_scans_decoded_json_for_escaped_token() {
        let token = "token\\with\"quote";
        for status in ["200 OK", "401 Unauthorized"] {
            let body = if status.starts_with("200") {
                serde_json::to_string(&json!({"nested": {"value": token}})).unwrap()
            } else {
                let mut protected = serde_json::Map::new();
                protected.insert(token.to_string(), json!("value"));
                serde_json::to_string(&json!({"nested": protected})).unwrap()
            };
            assert!(
                !body.contains(token),
                "fixture must require decoded scanning"
            );
            let base = owned_stub_server(status, body);
            let error = api_get_with_auth(&base, "/api/v1/operator/threads", token)
                .await
                .expect_err("decoded token rejected");
            assert!(matches!(error, ProxyError::ProtectedMaterial));
            assert!(!error.to_string().contains(token));
        }
    }

    #[tokio::test]
    async fn authenticated_http_never_follows_redirects_with_bearer_token() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener as TokioTcpListener;

        for method in ["GET", "POST"] {
            let decoy = TokioTcpListener::bind("127.0.0.1:0").await.unwrap();
            let decoy_address = decoy.local_addr().unwrap();
            let decoy_task = tokio::spawn(async move {
                tokio::time::timeout(std::time::Duration::from_millis(100), decoy.accept())
                    .await
                    .is_ok()
            });

            let origin = TokioTcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin_address = origin.local_addr().unwrap();
            let origin_task = tokio::spawn(async move {
                let (mut stream, _) = origin.accept().await.unwrap();
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await.unwrap();
                let response = format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{decoy_address}/capture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });

            let base = format!("http://{origin_address}");
            let error = if method == "GET" {
                api_get_with_auth(&base, "/api/v1/operator/threads", "redirect-secret")
                    .await
                    .expect_err("302 is terminal")
            } else {
                api_post_with_auth(
                    &base,
                    "/api/v1/operator/threads",
                    json!({"thread_id":"default"}),
                    "redirect-secret",
                )
                .await
                .expect_err("302 is terminal")
            };
            origin_task.await.unwrap();
            assert!(matches!(error, ProxyError::Http { status: 302, .. }));
            assert!(!decoy_task.await.unwrap(), "{method} redirect was followed");
        }
    }

    #[tokio::test]
    async fn authenticated_http_ignores_system_proxy_in_isolated_child() {
        const CHILD_FLAG: &str = "HEIWA_DESKTOP_PROXY_TEST_CHILD";
        const BASE_ENV: &str = "HEIWA_DESKTOP_PROXY_TEST_BASE";

        if env::var_os(CHILD_FLAG).is_some() {
            let base = env::var(BASE_ENV).expect("child direct base");
            let value = api_get_with_auth(&base, "/api/v1/operator/threads", "proxy-secret")
                .await
                .expect("child connects directly");
            assert_eq!(value["ok"], json!(true));
            return;
        }

        let (base, direct_request) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        let decoy = TcpListener::bind("127.0.0.1:0").expect("bind proxy decoy");
        decoy.set_nonblocking(true).unwrap();
        let decoy_address = decoy.local_addr().unwrap();
        let (decoy_tx, decoy_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
            loop {
                match decoy.accept() {
                    Ok(_) => {
                        decoy_tx.send(true).unwrap();
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            decoy_tx.send(false).unwrap();
                            return;
                        }
                        thread::sleep(std::time::Duration::from_millis(2));
                    }
                    Err(error) => panic!("proxy decoy accept failed: {error}"),
                }
            }
        });

        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("proxy::tests::authenticated_http_ignores_system_proxy_in_isolated_child")
            .arg("--nocapture")
            .env(CHILD_FLAG, "1")
            .env(BASE_ENV, &base)
            .env("HTTP_PROXY", format!("http://{decoy_address}"))
            .env("HTTPS_PROXY", format!("http://{decoy_address}"))
            .env("http_proxy", format!("http://{decoy_address}"))
            .env("https_proxy", format!("http://{decoy_address}"))
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .status()
            .expect("run isolated proxy child");
        assert!(status.success(), "isolated proxy child failed");
        assert!(direct_request
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("direct server request")
            .contains("authorization: Bearer proxy-secret"));
        assert!(
            !decoy_rx.recv().unwrap(),
            "system proxy received a connection"
        );
    }
}
