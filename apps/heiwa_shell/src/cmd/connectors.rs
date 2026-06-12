//! Unified connector registry: provider CLIs plus life connectors
//! (calendar/mail) with real local probes and an OAuth staging flow.
//!
//! Status semantics are deliberately honest:
//! - `connected`   — credential or runtime probe succeeded
//! - `staged`      — client credentials staged, user consent not yet granted
//! - `needs_auth`  — connector is wired but has no usable credential
//! - `metadata`    — limited metadata-only lane is available (no body reads)
//! - `planned`     — direction is committed, no working bridge yet

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Read-first scopes: syncs become read models before any external write lane.
fn google_scopes(connector: &str) -> Option<&'static str> {
    match connector {
        "google_calendar" => Some("https://www.googleapis.com/auth/calendar.readonly"),
        "gmail" => Some("https://www.googleapis.com/auth/gmail.readonly"),
        _ => None,
    }
}

pub fn secrets_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".heiwa")
        .join("secrets")
}

fn client_secret_path() -> PathBuf {
    secrets_dir().join("google_oauth_client.json")
}

fn token_path(connector: &str) -> PathBuf {
    secrets_dir().join(format!("{connector}_token.json"))
}

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => {
            status(args);
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(connector) => connect(connector, &args[1..]).await,
    }
}

fn status(args: &[String]) {
    let payload = connectors_payload();
    if has_flag(args, "--json") {
        println!("{payload}");
        return;
    }
    println!("connectors");
    if let Some(rows) = payload.get("connectors").and_then(Value::as_array) {
        for row in rows {
            let id = row.get("id").and_then(Value::as_str).unwrap_or("?");
            let status = row.get("status").and_then(Value::as_str).unwrap_or("?");
            let kind = row.get("kind").and_then(Value::as_str).unwrap_or("?");
            println!("  {id:<18} {status:<12} ({kind})");
        }
    }
    println!("  next: heiwa connect google-calendar --client-secret <path>");
}

async fn connect(connector: &str, args: &[String]) -> Result<()> {
    let connector = normalize_connector_id(connector);
    match connector.as_str() {
        "google_calendar" | "gmail" => google_connect(&connector, args).await,
        "apple_calendar" => {
            println!("apple_calendar: EventKit bridge is planned; no connect step exists yet.");
            println!("Direction: device-local EventKit bridge with Calendar permission.");
            Ok(())
        }
        "apple_mail" => {
            println!("apple_mail: metadata-only lane; no connect step required.");
            println!("Snapshot target: ~/.heiwa/state/mail/headers.jsonl");
            Ok(())
        }
        "imap" => {
            println!("imap: configure Himalaya at ~/.config/himalaya/config.toml; Heiwa probes it read-only.");
            Ok(())
        }
        other => Err(anyhow!(
            "unknown connector: {other} (try: google-calendar, gmail, apple-calendar, apple-mail, imap)"
        )),
    }
}

async fn google_connect(connector: &str, args: &[String]) -> Result<()> {
    if let Some(path) = flag_value(args, "--client-secret") {
        return stage_client_secret(&path);
    }
    if has_flag(args, "--authorize") {
        return google_authorize(connector).await;
    }
    let staged = client_secret_path().exists();
    let token = token_path(connector).exists();
    println!("{connector}");
    println!(
        "  client_secret: {}",
        if staged { "staged" } else { "missing" }
    );
    println!("  token: {}", if token { "present" } else { "absent" });
    if !staged {
        println!(
            "  next: heiwa connect {connector} --client-secret <downloaded-client-secret.json>"
        );
    } else if !token {
        println!("  next: heiwa connect {connector} --authorize");
    } else {
        println!("  next: connected; read-model sync can use this token");
    }
    Ok(())
}

/// Copy a Google OAuth client secret into ~/.heiwa/secrets with 0600 perms.
fn stage_client_secret(source: &str) -> Result<()> {
    let raw = fs::read_to_string(source)
        .with_context(|| format!("cannot read client secret file: {source}"))?;
    let parsed: Value = serde_json::from_str(&raw).context("client secret is not valid JSON")?;
    let creds = parsed
        .get("installed")
        .or_else(|| parsed.get("web"))
        .ok_or_else(|| anyhow!("client secret JSON missing 'installed' or 'web' key"))?;
    if creds.get("client_id").and_then(Value::as_str).is_none() {
        return Err(anyhow!("client secret JSON missing client_id"));
    }

    let dir = secrets_dir();
    fs::create_dir_all(&dir)?;
    let dest = client_secret_path();
    write_secret_file(&dest, raw.as_bytes())?;
    println!("staged: {}", dest.display());
    println!("next: heiwa connect google-calendar --authorize");
    Ok(())
}

/// Run the loopback PKCE consent flow: open browser, catch the redirect on
/// 127.0.0.1, exchange the code via curl, store the token read-only for owner.
async fn google_authorize(connector: &str) -> Result<()> {
    let scope = google_scopes(connector)
        .ok_or_else(|| anyhow!("no Google scope mapped for connector {connector}"))?;
    let raw = fs::read_to_string(client_secret_path()).context(
        "no staged client secret; run: heiwa connect google-calendar --client-secret <path>",
    )?;
    let parsed: Value = serde_json::from_str(&raw)?;
    let creds = parsed
        .get("installed")
        .or_else(|| parsed.get("web"))
        .ok_or_else(|| anyhow!("client secret JSON missing 'installed' or 'web' key"))?;
    let client_id = creds
        .get("client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing client_id"))?
        .to_string();
    let client_secret = creds
        .get("client_secret")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    let (verifier, challenge) = pkce_pair();
    let consent_url = format!(
        "{GOOGLE_AUTH_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent",
        url_encode(&client_id),
        url_encode(&redirect_uri),
        url_encode(scope),
        challenge,
    );

    println!("consent URL:");
    println!("  {consent_url}");
    let _ = Command::new("open").arg(&consent_url).status();
    println!("waiting for Google redirect on {redirect_uri} …");

    let code = wait_for_oauth_code(listener).await?;
    let token_json =
        exchange_code_via_curl(&code, &client_id, &client_secret, &redirect_uri, &verifier)?;

    let token_value: Value =
        serde_json::from_str(&token_json).context("token endpoint returned non-JSON")?;
    if token_value
        .get("access_token")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(anyhow!("token exchange failed: {token_json}"));
    }

    let dest = token_path(connector);
    let stored = json!({
        "connector": connector,
        "scope": scope,
        "obtained_at": chrono::Utc::now().to_rfc3339(),
        "token": token_value,
    });
    write_secret_file(&dest, stored.to_string().as_bytes())?;
    println!("token stored: {}", dest.display());
    Ok(())
}

async fn wait_for_oauth_code(listener: tokio::net::TcpListener) -> Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut stream, _) = listener.accept().await?;
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    let code = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| path.split("code=").nth(1))
        .map(|rest| rest.split('&').next().unwrap_or(rest).to_string())
        .filter(|code| !code.is_empty())
        .ok_or_else(|| anyhow!("no authorization code in redirect"))?;
    let body = "<html><body><p>Heiwa received the authorization. You can close this tab.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    Ok(code)
}

fn exchange_code_via_curl(
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<String> {
    let token_url =
        std::env::var("HEIWA_GOOGLE_TOKEN_URL").unwrap_or_else(|_| GOOGLE_TOKEN_URL.to_string());
    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-X")
        .arg("POST")
        .arg(token_url)
        .arg("--data-urlencode")
        .arg(format!("code={code}"))
        .arg("--data-urlencode")
        .arg(format!("client_id={client_id}"))
        .arg("--data-urlencode")
        .arg(format!("redirect_uri={redirect_uri}"))
        .arg("--data-urlencode")
        .arg("grant_type=authorization_code")
        .arg("--data-urlencode")
        .arg(format!("code_verifier={verifier}"));
    if !client_secret.is_empty() {
        cmd.arg("--data-urlencode")
            .arg(format!("client_secret={client_secret}"));
    }
    let output = cmd
        .output()
        .context("failed to run curl for token exchange")?;
    if !output.status.success() {
        return Err(anyhow!(
            "curl token exchange failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn pkce_pair() -> (String, String) {
    // Two v4 UUIDs give 244 bits of entropy in 64 unreserved chars.
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

fn write_secret_file(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn normalize_connector_id(raw: &str) -> String {
    raw.trim().replace('-', "_").to_ascii_lowercase()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let idx = args.iter().position(|arg| arg == flag)?;
    args.get(idx + 1).cloned()
}

fn print_help() {
    println!("heiwa connect");
    println!();
    println!("Usage:");
    println!("  heiwa connect status [--json]");
    println!("  heiwa connect google-calendar --client-secret <path>");
    println!("  heiwa connect google-calendar --authorize");
    println!("  heiwa connect gmail --authorize");
    println!();
    println!("Google flows use loopback PKCE with read-only scopes.");
    println!("Secrets land in ~/.heiwa/secrets with owner-only permissions.");
}

// ---------------------------------------------------------------------------
// Read model
// ---------------------------------------------------------------------------

/// Connector lane status used by calendar/mail summaries and /api/v1/connectors.
pub(crate) fn google_lane_status(connector: &str) -> &'static str {
    if token_path(connector).exists() {
        "connected"
    } else if client_secret_path().exists() {
        "staged"
    } else {
        "needs_auth"
    }
}

pub(crate) fn imap_configured() -> bool {
    dirs::home_dir()
        .map(|home| home.join(".config").join("himalaya").join("config.toml"))
        .is_some_and(|path| path.exists())
}

pub(crate) fn connectors_payload() -> Value {
    let mut rows = Vec::new();

    for provider in ["ollama", "gemini", "antigravity", "claude", "codex"] {
        if let Some(account) = heiwa_provider::get_auth_status(provider) {
            let status = match account.status.as_str() {
                "active" | "available" | "connected" => "connected",
                other => {
                    if other.contains("error") {
                        "error"
                    } else {
                        "needs_auth"
                    }
                }
            };
            let auth_kind = crate::cmd::app::auth_kind_label(&account.auth_kind);
            rows.push(json!({
                "id": account.provider_id,
                "kind": "provider",
                "display_name": account.provider_id,
                "status": status,
                "auth_kind": auth_kind,
                "rate_group": account.rate_group,
                "detail": format!("provider CLI lane ({auth_kind})"),
                "next_action": Value::Null,
            }));
        }
    }

    let google_calendar = google_lane_status("google_calendar");
    rows.push(json!({
        "id": "google_calendar",
        "kind": "calendar",
        "display_name": "Google Calendar",
        "status": google_calendar,
        "auth_kind": "oauth_loopback_pkce",
        "scopes": google_scopes("google_calendar"),
        "detail": "OAuth read-only sync; incremental sync tokens come after first full sync.",
        "next_action": match google_calendar {
            "needs_auth" => Value::String("heiwa connect google-calendar --client-secret <path>".into()),
            "staged" => Value::String("heiwa connect google-calendar --authorize".into()),
            _ => Value::Null,
        },
    }));

    rows.push(json!({
        "id": "apple_calendar",
        "kind": "calendar",
        "display_name": "Apple Calendar",
        "status": "planned",
        "auth_kind": "eventkit_bridge",
        "detail": "Device-local EventKit bridge with Calendar permission; not wired yet.",
        "next_action": Value::Null,
    }));

    rows.push(json!({
        "id": "heiwa_holds",
        "kind": "calendar",
        "display_name": "Heiwa Holds",
        "status": "connected",
        "auth_kind": "local_state",
        "detail": "Local-first focus/travel/soft holds under ~/.heiwa/state/calendar.",
        "next_action": Value::Null,
    }));

    let gmail = google_lane_status("gmail");
    rows.push(json!({
        "id": "gmail",
        "kind": "mail",
        "display_name": "Gmail",
        "status": gmail,
        "auth_kind": "oauth_loopback_pkce",
        "scopes": google_scopes("gmail"),
        "detail": "Read-only priority scan via scheduled local pull; send stays approval-gated.",
        "next_action": match gmail {
            "needs_auth" => Value::String("heiwa connect gmail --client-secret <path>".into()),
            "staged" => Value::String("heiwa connect gmail --authorize".into()),
            _ => Value::Null,
        },
    }));

    let mail_probe = crate::cmd::mail::mail_data_present();
    rows.push(json!({
        "id": "apple_mail",
        "kind": "mail",
        "display_name": "Apple Mail",
        "status": if mail_probe { "metadata" } else { "planned" },
        "auth_kind": "local_metadata",
        "detail": "Metadata-only lane (account, mailbox, sender, subject, date, unread); no body reads.",
        "next_action": Value::Null,
    }));

    rows.push(json!({
        "id": "imap",
        "kind": "mail",
        "display_name": "IMAP / Himalaya",
        "status": if imap_configured() { "staged" } else { "planned" },
        "auth_kind": "himalaya_config",
        "detail": "Portable fallback for user-owned IMAP accounts.",
        "next_action": if imap_configured() { Value::Null } else {
            Value::String("configure ~/.config/himalaya/config.toml".into())
        },
    }));

    let mut counts = serde_json::Map::new();
    for row in &rows {
        if let Some(status) = row.get("status").and_then(Value::as_str) {
            let entry = counts.entry(status.to_string()).or_insert(json!(0));
            *entry = json!(entry.as_u64().unwrap_or(0) + 1);
        }
    }

    json!({
        "connectors": rows,
        "counts": Value::Object(counts),
        "policy": [
            "read models before external writes",
            "external writes stage through approvals + receipts",
            "secrets stay local under ~/.heiwa/secrets (0600)"
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_is_urlsafe_and_long_enough() {
        let (verifier, challenge) = pkce_pair();
        assert!(verifier.len() >= 43 && verifier.len() <= 128);
        assert!(!challenge.contains('='));
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
    }

    #[test]
    fn url_encode_escapes_reserved() {
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(url_encode("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn normalize_connector_accepts_dashes() {
        assert_eq!(normalize_connector_id("google-calendar"), "google_calendar");
        assert_eq!(normalize_connector_id("Apple-Mail"), "apple_mail");
    }
}
