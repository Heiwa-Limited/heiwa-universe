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
}

#[derive(Debug, Error)]
pub enum ProxyError {
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

fn runtime_base_url() -> String {
    let port = env::var("HEIWA_APP_PORT").unwrap_or_else(|_| DEFAULT_RUNTIME_PORT.to_string());
    format!("http://127.0.0.1:{port}")
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

pub async fn api_get_with_base(base_url: &str, path: &str) -> Result<Value, ProxyError> {
    let url = endpoint_url(base_url, path)?;
    let response = reqwest::get(&url)
        .await
        .map_err(|error| ProxyError::Offline(error.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| ProxyError::Offline(error.to_string()))?;
    if !status.is_success() {
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body,
        });
    }
    serde_json::from_str(&body).map_err(|error| ProxyError::Decode(error.to_string()))
}

pub async fn api_post_with_base(
    base_url: &str,
    path: &str,
    body: Value,
) -> Result<Value, ProxyError> {
    let url = endpoint_url(base_url, path)?;
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|error| ProxyError::Offline(error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ProxyError::Offline(error.to_string()))?;
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
    api_get_with_base(&runtime_base_url(), &path)
        .await
        .map_err(ApiErrorPayload::from)
}

#[tauri::command]
pub async fn api_post(path: String, body: Value) -> Result<Value, ApiErrorPayload> {
    api_post_with_base(&runtime_base_url(), &path, body)
        .await
        .map_err(ApiErrorPayload::from)
}

#[tauri::command]
pub async fn runtime_health() -> RuntimeHealth {
    match api_get_with_base(&runtime_base_url(), "/api/v1/runtime/snapshot").await {
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

    #[tokio::test]
    async fn api_get_with_base_returns_runtime_json() {
        let base = stub_server(
            "200 OK",
            r#"{"ok":true,"data":{"runtime_version":"0.1.0"}}"#,
        );
        let value = api_get_with_base(&base, "/api/v1/runtime/snapshot")
            .await
            .expect("json response");
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["data"]["runtime_version"], json!("0.1.0"));
    }

    #[tokio::test]
    async fn api_get_with_base_rejects_non_runtime_paths() {
        let error = api_get_with_base("http://127.0.0.1:1", "/etc/passwd")
            .await
            .expect_err("invalid path rejected before network");
        assert!(matches!(error, ProxyError::InvalidPath(_)));
    }

    #[tokio::test]
    async fn api_get_with_base_reports_http_errors() {
        let base = stub_server("503 Service Unavailable", r#"{"error":"down"}"#);
        let error = api_get_with_base(&base, "/api/v1/runtime/snapshot")
            .await
            .expect_err("http error");
        assert!(matches!(error, ProxyError::Http { status: 503, .. }));
    }
}
