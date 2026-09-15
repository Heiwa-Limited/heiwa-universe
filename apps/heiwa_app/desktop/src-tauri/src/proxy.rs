use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const DEFAULT_RUNTIME_PORT: &str = "7474";
const MAX_RUNTIME_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

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
    #[error("runtime response too large")]
    ResponseTooLarge,
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
            ProxyError::ResponseTooLarge => Self::Decode("runtime response too large".to_string()),
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
    Ok(format!("http://127.0.0.1:{}", runtime_port()?))
}

pub(crate) fn runtime_port() -> Result<u16, ProxyError> {
    let configured = match env::var("HEIWA_APP_PORT") {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => return Err(ProxyError::InvalidEndpoint),
    };
    parse_runtime_port(configured.as_deref())
}

#[cfg(test)]
fn runtime_base_url_from_port(configured: Option<&str>) -> Result<String, ProxyError> {
    Ok(format!(
        "http://127.0.0.1:{}",
        parse_runtime_port(configured)?
    ))
}

fn parse_runtime_port(configured: Option<&str>) -> Result<u16, ProxyError> {
    let raw = configured.unwrap_or(DEFAULT_RUNTIME_PORT);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProxyError::InvalidEndpoint);
    }
    let port = raw
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or(ProxyError::InvalidEndpoint)?;
    Ok(port)
}

pub(crate) fn runtime_websocket_base_url() -> Result<String, ProxyError> {
    Ok(runtime_base_url()?.replacen("http://", "ws://", 1))
}

pub(crate) fn machine_auth_token() -> Result<String, ProxyError> {
    let token = heiwa_core::config::RuntimeConfig::from_env().machine_auth_token;
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

/// A `.`/`..` path segment would be resolved away by [`reqwest::Url::join`]
/// before the request is ever sent (RFC 3986 dot-segment removal), which
/// could let a path like `/api/v1/../../admin` slip past the `/api/v1/`
/// prefix check below (and past [`post_path_allowed`]'s allowlist) even
/// though neither check ever looks unsafe on its own — both check the raw
/// string, not the string `Url::join` actually resolves to. Rejecting any
/// dot segment up front keeps the raw string and the resolved request path
/// identical, so checking one is checking both.
///
/// This only catches a *literal* `.`/`..`. See [`has_disallowed_path_byte`]
/// for percent-encoded spellings of the same thing.
fn has_dot_segment(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment == "." || segment == "..")
}

/// Bytes that change how the rest of the URL machinery interprets `path`,
/// none of which any path this proxy allowlists ever needs:
///
/// - `%` triggers percent-decoding. Critically, `reqwest`/`url` implement
///   the WHATWG URL Standard, not bare RFC 3986: its path parser treats a
///   percent-encoded `%2e`/`%2E` as equivalent to a literal `.` *before*
///   dot-segment removal runs, so `%2e%2e`, `.%2e`, and `%2E.` all collapse
///   exactly like `..` — confirmed empirically
///   (`Url::parse(base).join("/api/v1/operator/threads/%2e%2e/turns")`
///   resolves to `/api/v1/operator/turns`), not just inferred from the
///   spec. Banning `%` outright closes every encoded spelling at once,
///   including `%2f` (encoded slash), rather than pattern-matching each
///   known-bad decoding one at a time.
/// - `?` and `#` start the URL's query and fragment components. A path
///   like `/api/v1/approvals/x?evil=1/approve` allowlist-checks as one
///   string but resolves to a request against `/api/v1/approvals/x` with
///   query `evil=1/approve` — a different, unchecked route (also confirmed
///   empirically).
/// - `\` is a path separator for "special" schemes (http/https/ws/wss) per
///   the WHATWG URL Standard, so an id containing it can introduce an extra
///   path segment the allowlist's "no `/` in the id" check never saw:
///   joining `.../a\../turns` resolves to `.../turns`, silently dropping
///   the `a` segment entirely (also confirmed empirically).
fn has_disallowed_path_byte(path: &str) -> bool {
    path.bytes()
        .any(|byte| matches!(byte, b'?' | b'#' | b'%' | b'\\'))
}

fn endpoint_url(base_url: &str, path: &str) -> Result<reqwest::Url, ProxyError> {
    if !path.starts_with("/api/v1/") && path != "/status/health" {
        return Err(ProxyError::InvalidPath(path.to_string()));
    }
    if has_dot_segment(path) || has_disallowed_path_byte(path) {
        return Err(ProxyError::InvalidPath(path.to_string()));
    }
    let base = validate_loopback_url(base_url, "http")?;
    if base.path() != "/" || base.query().is_some() {
        return Err(ProxyError::InvalidEndpoint);
    }
    let final_url = base.join(path).map_err(|_| ProxyError::InvalidEndpoint)?;
    validate_loopback_url(final_url.as_str(), "http")?;
    Ok(final_url)
}

/// Endpoints the desktop UI is allowed to reach through the write proxy.
/// `api_get` stays passthrough-by-prefix (read-only; the same rigor for GET
/// is called out as an open question in `reports/L-009-desktop-ipc-least-privilege.md`
/// rather than silently folded into this task). Keep this list in sync with
/// the mirrored allowlist in `src/state/api-post-allowlist.test.ts`, which
/// scans the frontend for every literal POST path it actually calls and
/// fails if either side has an entry the other does not.
const ALLOWED_POST_EXACT: &[&str] = &[
    "/api/v1/agents/dispatch",
    "/api/v1/calendar/sync",
    "/api/v1/calendar/read",
    "/api/v1/calendar/holds",
    "/api/v1/connectors/apple_calendar/connect",
    "/api/v1/connectors/apple_calendar/disconnect",
    "/api/v1/operator/threads",
    "/api/v1/operator/projects",
];

/// A POST path shaped `prefix + "<one id segment>" + suffix`. This layer's
/// job is narrower than fully revalidating the id — the runtime behind this
/// proxy still does that (real ids seen from the desktop are alphanumeric
/// plus `-_.`: approval request ids are validated with exactly that charset
/// by `validate_request_id`, apps/heiwa_shell/src/cmd/approvals.rs:277, and
/// thread/project ids are `thread-`/`project-` plus a hyphenated UUID,
/// apps/heiwa_shell/src/cmd/work.rs:274 and
/// apps/heiwa_shell/src/cmd/app.rs:2574) — but it must still stop an id from
/// changing the *shape* of the request: see [`is_valid_post_id_segment`].
struct AllowedPostIdRoute {
    prefix: &'static str,
    suffix: &'static str,
}

const ALLOWED_POST_ID_PATTERNS: &[AllowedPostIdRoute] = &[
    AllowedPostIdRoute {
        prefix: "/api/v1/approvals/",
        suffix: "/approve",
    },
    AllowedPostIdRoute {
        prefix: "/api/v1/approvals/",
        suffix: "/deny",
    },
    AllowedPostIdRoute {
        prefix: "/api/v1/operator/threads/",
        suffix: "/metadata",
    },
    AllowedPostIdRoute {
        prefix: "/api/v1/operator/threads/",
        suffix: "/turns",
    },
    AllowedPostIdRoute {
        prefix: "/api/v1/operator/projects/",
        suffix: "/metadata",
    },
];

/// RFC 3986 "unreserved" characters, plus `:`. A strict superset of every
/// real id this proxy forwards today (see the file:line citations on
/// [`AllowedPostIdRoute`]), chosen instead of matching those real ids
/// exactly so this stays a structural safety net rather than a second copy
/// of the runtime's own id-format rules. Excluding `%` specifically means a
/// percent-encoded escape can never appear in an id this allowlist accepts
/// — closing the dot-segment-normalization and encoded-separator bypass
/// classes at this layer too, not just in [`has_disallowed_path_byte`].
/// Excluding `?`/`#`/`/`/`\` means an id can never change how many path
/// segments, or which URL component, the rest of the string parses as.
fn is_valid_post_id_segment(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b':' | b'-')
        })
}

fn post_path_allowed(path: &str) -> bool {
    if ALLOWED_POST_EXACT.contains(&path) {
        return true;
    }
    ALLOWED_POST_ID_PATTERNS.iter().any(|route| {
        path.strip_prefix(route.prefix)
            .and_then(|rest| rest.strip_suffix(route.suffix))
            .is_some_and(is_valid_post_id_segment)
    })
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
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(60))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ProxyError::Offline("runtime client unavailable".to_string()))
}

pub(crate) fn signed_local_request(
    method: &str,
    url: &reqwest::Url,
    body: &[u8],
    token: &str,
) -> Result<heiwa_core::auth::LocalRequestSignature, ProxyError> {
    validate_auth_token(token)?;
    let port = url.port().ok_or(ProxyError::InvalidEndpoint)?;
    let target = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    let timestamp = unix_timestamp_now();
    heiwa_core::auth::sign_local_request(
        heiwa_core::auth::LocalRequestParts {
            method,
            port,
            target: &target,
            body,
        },
        timestamp,
        &uuid::Uuid::new_v4().simple().to_string(),
        token,
    )
    .map_err(|_| ProxyError::AuthNotConfigured)
}

pub(crate) fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn add_signed_headers(
    request: reqwest::RequestBuilder,
    signed: &heiwa_core::auth::LocalRequestSignature,
) -> Result<reqwest::RequestBuilder, ProxyError> {
    let mut signature = reqwest::header::HeaderValue::from_str(&signed.signature)
        .map_err(|_| ProxyError::AuthNotConfigured)?;
    signature.set_sensitive(true);
    Ok(request
        .header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_VERSION_HEADER,
            &signed.version,
        )
        .header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_TIMESTAMP_HEADER,
            &signed.timestamp,
        )
        .header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_NONCE_HEADER,
            &signed.nonce,
        )
        .header(
            heiwa_core::auth::LOCAL_REQUEST_AUTH_SIGNATURE_HEADER,
            signature,
        ))
}

async fn bounded_response_text(response: reqwest::Response) -> Result<String, ProxyError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RUNTIME_RESPONSE_BYTES as u64)
    {
        return Err(ProxyError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
        let chunk =
            chunk.map_err(|_| ProxyError::Offline("runtime response failed".to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RUNTIME_RESPONSE_BYTES {
            return Err(ProxyError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|error| ProxyError::Decode(error.to_string()))
}

pub(crate) async fn api_get_with_auth(
    base_url: &str,
    path: &str,
    token: &str,
) -> Result<Value, ProxyError> {
    validate_auth_token(token)?;
    let url = endpoint_url(base_url, path)?;
    let signed = signed_local_request("GET", &url, b"", token)?;
    let response = add_signed_headers(authenticated_http_client()?.get(url), &signed)?
        .send()
        .await
        .map_err(|_| ProxyError::Offline("runtime request failed".to_string()))?;
    let status = response.status();
    let body = bounded_response_text(response).await?;
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
    if !post_path_allowed(path) {
        return Err(ProxyError::InvalidPath(path.to_string()));
    }
    let url = endpoint_url(base_url, path)?;
    let body = serde_json::to_vec(&body).map_err(|error| ProxyError::Decode(error.to_string()))?;
    let signed = signed_local_request("POST", &url, &body, token)?;
    let response = add_signed_headers(
        authenticated_http_client()?
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body),
        &signed,
    )?
    .send()
    .await
    .map_err(|_| ProxyError::Offline("runtime request failed".to_string()))?;
    let status = response.status();
    let text = bounded_response_text(response).await?;
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

/// Whether *something* holds the configured runtime port.
///
/// A bare TCP connect, not a health request: the supervisor polls this in a
/// loop while a runtime it started comes up, and an HTTP round trip per poll
/// would be both slower and misleading — a runtime that is listening but not
/// yet serving is still the one we started.
///
/// This is liveness only, never identity. Any listener answers it, so it must
/// not decide whether to adopt a process; see [`runtime_identity_confirmed`].
pub fn runtime_is_reachable() -> bool {
    let Ok(base) = runtime_base_url() else {
        return false;
    };
    let Some(authority) = base.strip_prefix("http://") else {
        return false;
    };
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = authority.to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| {
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400)).is_ok()
    })
}

/// Whether the process on the runtime port is a Heiwa runtime this app can
/// drive.
///
/// Adoption cannot be decided by a TCP connect. Any unrelated listener on the
/// port — another product, a tunnel, a stale process — would then stop the
/// bundled runtime from ever starting and leave the window bound to a server
/// that cannot answer a single Heiwa call. So adoption costs one signed
/// request to the runtime's own snapshot endpoint: a foreign listener cannot
/// return that payload, and a Heiwa runtime this app is not authorized
/// against does not count as adoptable either.
pub fn runtime_identity_confirmed() -> bool {
    let Ok(token) = machine_auth_token() else {
        return false;
    };
    let Ok(base) = runtime_base_url() else {
        return false;
    };
    tauri::async_runtime::block_on(async move { confirm_runtime_identity(&base, &token).await })
}

pub(crate) async fn confirm_runtime_identity(base_url: &str, token: &str) -> bool {
    match tokio::time::timeout(
        std::time::Duration::from_millis(1500),
        api_get_with_auth(base_url, "/api/v1/runtime/snapshot", token),
    )
    .await
    {
        Ok(Ok(snapshot)) => is_heiwa_runtime_snapshot(&snapshot),
        _ => false,
    }
}

/// The runtime identity in a snapshot response: `data.runtime.version`.
///
/// Checked rather than trusting a 200, because an unrelated local service can
/// answer any path with valid JSON.
fn is_heiwa_runtime_snapshot(snapshot: &Value) -> bool {
    snapshot
        .get("data")
        .and_then(|data| data.get("runtime"))
        .and_then(|runtime| runtime.get("version"))
        .and_then(Value::as_str)
        .is_some_and(|version| !version.trim().is_empty())
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
            // Oversized-response tests intentionally let the bounded client
            // close before the stub finishes writing.
            let _ = stream.write_all(response.as_bytes());
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
    async fn native_get_and_post_send_signed_machine_requests_without_bearer() {
        let (get_base, get_request) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        api_get_with_auth(&get_base, "/api/v1/operator/threads", "desktop-token")
            .await
            .expect("authorized get");
        let get_request = get_request.recv().expect("get request");
        assert!(!get_request.to_ascii_lowercase().contains("authorization:"));
        assert!(get_request.contains("x-heiwa-local-auth-version: 1"));
        assert!(get_request.contains("x-heiwa-local-auth-signature:"));

        let (post_base, post_request) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        api_post_with_auth(
            &post_base,
            "/api/v1/operator/threads",
            json!({"thread_id":"default"}),
            "desktop-token",
        )
        .await
        .expect("authorized post");
        let post_request = post_request.recv().expect("post request");
        assert!(!post_request.to_ascii_lowercase().contains("authorization:"));
        assert!(post_request.contains("x-heiwa-local-auth-version: 1"));
        assert!(post_request.contains("x-heiwa-local-auth-signature:"));
    }

    #[tokio::test]
    async fn authenticated_http_rejects_response_bodies_over_two_mibibytes() {
        let body = serde_json::to_string(&"x".repeat(2 * 1024 * 1024)).unwrap();
        let base = owned_stub_server("200 OK", body);
        let error = api_get_with_auth(&base, "/api/v1/operator/threads", "bounded-response-token")
            .await
            .expect_err("oversized response must be rejected before JSON projection");
        assert_eq!(error.to_string(), "runtime response too large");
    }

    #[tokio::test]
    async fn missing_native_auth_fails_before_network_without_token_leakage() {
        let error = api_get_with_auth("http://127.0.0.1:1", "/api/v1/operator/threads", "  ")
            .await
            .expect_err("empty auth must fail before connect");
        assert!(matches!(error, ProxyError::AuthNotConfigured));
        assert!(!error.to_string().contains("Bearer"));
    }

    #[tokio::test]
    async fn runtime_identity_requires_a_heiwa_snapshot_not_merely_a_200() {
        // An unrelated local service can answer any path with valid JSON. If
        // that counted as identity, the supervisor would adopt it and never
        // start the runtime the app shipped with.
        let foreign = stub_server("200 OK", r#"{"ok":true,"data":{"status":"fine"}}"#);
        assert!(!confirm_runtime_identity(&foreign, "identity-token").await);

        let dead = confirm_runtime_identity("http://127.0.0.1:1", "identity-token").await;
        assert!(!dead, "an unreachable port is not an adoptable runtime");

        let heiwa = stub_server(
            "200 OK",
            r#"{"ok":true,"data":{"runtime":{"status":"ok","version":"0.1.0"}}}"#,
        );
        assert!(confirm_runtime_identity(&heiwa, "identity-token").await);
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
    async fn authenticated_http_never_follows_redirects_with_machine_auth() {
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
        let request = direct_request
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("direct server request");
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(request.contains("x-heiwa-local-auth-signature:"));
        assert!(
            !decoy_rx.recv().unwrap(),
            "system proxy received a connection"
        );
    }

    #[test]
    fn post_path_allowed_accepts_every_exact_route_the_ui_calls() {
        for path in ALLOWED_POST_EXACT {
            assert!(post_path_allowed(path), "{path} should be allowed");
        }
    }

    #[test]
    fn post_path_allowed_accepts_id_patterns_with_a_single_clean_segment() {
        assert!(post_path_allowed("/api/v1/approvals/abc123/approve"));
        assert!(post_path_allowed("/api/v1/approvals/abc123/deny"));
        assert!(post_path_allowed(
            "/api/v1/operator/threads/thread-1/metadata"
        ));
        assert!(post_path_allowed("/api/v1/operator/threads/thread-1/turns"));
        assert!(post_path_allowed(
            "/api/v1/operator/projects/proj-1/metadata"
        ));
    }

    #[test]
    fn post_path_allowed_rejects_unknown_and_malformed_paths() {
        assert!(!post_path_allowed("/api/v1/agents/dispatch/extra"));
        assert!(!post_path_allowed("/api/v1/unknown/route"));
        assert!(
            !post_path_allowed("/api/v1/approvals//approve"),
            "empty id segment"
        );
        assert!(
            !post_path_allowed("/api/v1/approvals/a/b/approve"),
            "id segment must not smuggle an extra path component"
        );
        assert!(
            !post_path_allowed("/api/v1/approvals/abc123/revoke"),
            "suffix not allowlisted"
        );
        assert!(
            !post_path_allowed("/status/health"),
            "GET-only path, never a POST target"
        );
    }

    #[tokio::test]
    async fn api_post_rejects_a_disallowed_path_before_any_network_request() {
        let (base, request_rx) = inspecting_stub_server("200 OK", r#"{"ok":true}"#);
        let error = api_post_with_auth(
            &base,
            "/api/v1/not/allowlisted",
            json!({}),
            "post-allowlist-token",
        )
        .await
        .expect_err("disallowed path must be rejected");
        assert!(matches!(error, ProxyError::InvalidPath(_)));
        assert!(
            request_rx
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "the stub server must never see a request for a disallowed path"
        );
    }

    #[test]
    fn endpoint_url_rejects_dot_segments_that_url_join_would_normalize_away() {
        // Without the dot-segment guard, `Url::join` would resolve this down
        // to `/admin`, escaping both the `/api/v1/` prefix check and the
        // POST allowlist, which only ever inspect the raw string.
        let error = endpoint_url("http://127.0.0.1:9", "/api/v1/../../admin")
            .expect_err("a dot segment must be rejected before it can be normalized away");
        assert!(matches!(error, ProxyError::InvalidPath(_)));
    }

    #[test]
    fn has_dot_segment_flags_single_and_double_dot_segments_only() {
        assert!(has_dot_segment("/api/v1/../evil"));
        assert!(has_dot_segment("/api/v1/./evil"));
        assert!(!has_dot_segment("/api/v1/operator/threads"));
        // A segment that merely contains dots (not equal to "." or "..") is
        // a legitimate id shape (e.g. a version string) and must pass.
        assert!(!has_dot_segment("/api/v1/operator/threads/v1.2.3/metadata"));
    }

    /// Review round 1 (Opus, on PR #131): `has_dot_segment` alone missed
    /// every percent-encoded spelling of a dot segment, and neither it nor
    /// the old id check (`!id.is_empty() && !id.contains('/')`) accounted
    /// for `?`, `#`, or `\` changing how the joined URL parses. These are
    /// exactly the hostile strings that were empirically confirmed (against
    /// this crate's pinned `url`/`reqwest` version) to resolve outside the
    /// path either check validated, before `has_disallowed_path_byte` and
    /// `is_valid_post_id_segment` existed.
    const HOSTILE_ID_SEGMENTS: &[&str] = &[
        "%2e%2e", ".%2e", "%2E.", "%2f", "a%2fb", "x?evil=1", "x#frag", "a\\b", "a\\..", "",
    ];

    #[test]
    fn post_path_allowed_rejects_every_hostile_id_on_an_id_pattern_route() {
        for id in HOSTILE_ID_SEGMENTS {
            let approve = format!("/api/v1/approvals/{id}/approve");
            let turns = format!("/api/v1/operator/threads/{id}/turns");
            assert!(!post_path_allowed(&approve), "{approve:?} must be rejected");
            assert!(!post_path_allowed(&turns), "{turns:?} must be rejected");
        }
    }

    #[test]
    fn is_valid_post_id_segment_rejects_hostile_bytes_and_dot_segments() {
        for id in HOSTILE_ID_SEGMENTS {
            assert!(!is_valid_post_id_segment(id), "{id:?} must be rejected");
        }
        assert!(!is_valid_post_id_segment("."));
        assert!(!is_valid_post_id_segment(".."));
        assert!(is_valid_post_id_segment("thread-abc123"));
        assert!(is_valid_post_id_segment("req_alpha"));
        assert!(is_valid_post_id_segment(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn endpoint_url_never_resolves_to_a_path_other_than_the_one_checked() {
        // For every hostile string that (pre-fix) either bypass could have
        // accepted, endpoint_url must now reject it outright. Where it
        // doesn't reject, the resolved URL's path must be byte-identical to
        // the input -- i.e. never silently normalized to something else.
        let hostile_paths = [
            "/api/v1/operator/threads/%2e%2e/turns",
            "/api/v1/operator/threads/.%2e/turns",
            "/api/v1/operator/threads/%2E./turns",
            "/api/v1/operator/threads/../turns",
            "/api/v1/approvals/x?evil=1/approve",
            "/api/v1/approvals/x#frag/approve",
            "/api/v1/operator/threads/a%2fb/turns",
            "/api/v1/operator/threads/a\\../turns",
            "/api/v1/operator/threads/a\\b/turns",
        ];
        for path in hostile_paths {
            match endpoint_url("http://127.0.0.1:9", path) {
                Err(_) => {}
                Ok(url) => assert_eq!(
                    url.path(),
                    path,
                    "endpoint_url silently resolved {path:?} to {:?} instead of rejecting it",
                    url.path()
                ),
            }
        }
    }

    #[test]
    fn endpoint_url_still_accepts_ordinary_paths_unchanged() {
        for path in [
            "/api/v1/operator/threads",
            "/api/v1/operator/threads/thread-1/turns",
            "/api/v1/approvals/req_alpha/approve",
            "/status/health",
        ] {
            let url =
                endpoint_url("http://127.0.0.1:9", path).expect("ordinary path must be accepted");
            assert_eq!(url.path(), path);
        }
    }

    #[test]
    fn has_disallowed_path_byte_flags_percent_query_fragment_and_backslash() {
        assert!(has_disallowed_path_byte("/api/v1/x%2e"));
        assert!(has_disallowed_path_byte("/api/v1/x?y"));
        assert!(has_disallowed_path_byte("/api/v1/x#y"));
        assert!(has_disallowed_path_byte("/api/v1/x\\y"));
        assert!(!has_disallowed_path_byte("/api/v1/operator/threads/v1.2.3"));
    }
}
