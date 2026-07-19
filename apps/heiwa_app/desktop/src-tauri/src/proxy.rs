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

pub(crate) fn runtime_base_url() -> String {
    let port = env::var("HEIWA_APP_PORT").unwrap_or_else(|_| DEFAULT_RUNTIME_PORT.to_string());
    format!("http://127.0.0.1:{port}")
}

pub(crate) fn runtime_websocket_base_url() -> String {
    runtime_base_url().replacen("http://", "ws://", 1)
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

fn endpoint_url(base_url: &str, path: &str) -> Result<String, ProxyError> {
    if !path.starts_with("/api/v1/") && path != "/status/health" {
        return Err(ProxyError::InvalidPath(path.to_string()));
    }
    Ok(format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
            .chars()
            .fold(String::from("/"), |mut acc, c| {
                acc.push(c);
                acc
            })
    ))
}

pub(crate) async fn api_get_with_auth(
    base_url: &str,
    path: &str,
    token: &str,
) -> Result<Value, ProxyError> {
    validate_auth_token(token)?;
    let url = endpoint_url(base_url, path)?;
    let response = reqwest::Client::new()
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
    if body.contains(token) {
        return Err(ProxyError::Decode(
            "runtime response contained protected authentication material".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body,
        });
    }
    serde_json::from_str(&body).map_err(|error| ProxyError::Decode(error.to_string()))
}

pub(crate) async fn api_post_with_auth(
    base_url: &str,
    path: &str,
    body: Value,
    token: &str,
) -> Result<Value, ProxyError> {
    validate_auth_token(token)?;
    let url = endpoint_url(base_url, path)?;
    let client = reqwest::Client::new();
    let response = client
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
    if text.contains(token) {
        return Err(ProxyError::Decode(
            "runtime response contained protected authentication material".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body: text,
        });
    }
    serde_json::from_str(&text).map_err(|error| ProxyError::Decode(error.to_string()))
}

#[tauri::command]
pub async fn api_get(path: String) -> Result<Value, ApiErrorPayload> {
    let token = machine_auth_token().map_err(ApiErrorPayload::from)?;
    api_get_with_auth(&runtime_base_url(), &path, &token)
        .await
        .map_err(ApiErrorPayload::from)
}

#[tauri::command]
pub async fn api_post(path: String, body: Value) -> Result<Value, ApiErrorPayload> {
    let token = machine_auth_token().map_err(ApiErrorPayload::from)?;
    api_post_with_auth(&runtime_base_url(), &path, body, &token)
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
    match api_get_with_auth(&runtime_base_url(), "/api/v1/runtime/snapshot", &token).await {
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
}
