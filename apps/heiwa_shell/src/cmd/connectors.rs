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
use heiwa_oauth::{
    build_authorization_request, exchange_code, merge_refreshed, refresh, to_secret,
    LoopbackListener, ProviderConfig,
};
use heiwa_vault::{OAuthSecret, Vault, VaultError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const CONNECTOR_OAUTH_SERVICE: &str = "heiwa-connector-oauth";
const GOOGLE_CLIENT_SCHEMA: &str = "heiwa_google_oauth_client_v1";
const APPLE_CALENDAR_ENROLLMENT_SCHEMA: &str = "heiwa_connector_enrollment_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppleCalendarEnrollment {
    schema_version: String,
    connector: String,
    installation_id: String,
    device_id: String,
    connected_at: String,
    scopes: Vec<String>,
}

/// Read-first scopes: syncs become read models before any external write lane.
fn google_scopes(connector: &str) -> Option<&'static str> {
    match connector {
        "google_calendar" => Some("https://www.googleapis.com/auth/calendar.readonly"),
        "gmail" => Some("https://www.googleapis.com/auth/gmail.send"),
        _ => None,
    }
}

fn client_config_path() -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("connectors")
        .join("google_oauth_client.json")
}

fn connector_account_id(connector: &str) -> String {
    format!("google:{connector}:default")
}

fn connector_vault() -> Vault {
    Vault::new(CONNECTOR_OAUTH_SERVICE)
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
    println!("  next: heiwa connect google-calendar --client-secret <downloaded-desktop-app.json>");
}

async fn connect(connector: &str, args: &[String]) -> Result<()> {
    let connector = normalize_connector_id(connector);
    match connector.as_str() {
        "google_calendar" | "gmail" => google_connect(&connector, args).await,
        "apple_calendar" => apple_calendar_connect(args),
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

fn apple_calendar_connect(args: &[String]) -> Result<()> {
    if has_flag(args, "--disconnect") {
        let payload = disconnect_apple_calendar()?;
        println!(
            "apple_calendar: {}",
            payload["status"].as_str().unwrap_or("disconnected")
        );
        println!("local read models were preserved; revoke macOS permission separately if wanted");
        return Ok(());
    }
    if has_flag(args, "--authorize") {
        let payload = connect_apple_calendar()?;
        println!("apple_calendar: connected");
        println!(
            "  resources: {}",
            payload["resource_count"].as_u64().unwrap_or(0)
        );
        println!("  permission owner: macOS");
        return Ok(());
    }

    let payload = apple_calendar_connection_payload();
    println!("apple_calendar");
    println!(
        "  status: {}",
        payload["status"].as_str().unwrap_or("config_error")
    );
    if payload["status"] != "connected" {
        println!("  next: heiwa connect apple-calendar --authorize");
    }
    Ok(())
}

fn apple_calendar_enrollment_path() -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("connectors")
        .join("apple_calendar.json")
}

fn load_apple_calendar_enrollment() -> Result<Option<AppleCalendarEnrollment>> {
    let path = apple_calendar_enrollment_path();
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read Apple Calendar enrollment: {}", path.display()))?;
    let enrollment: AppleCalendarEnrollment = serde_json::from_str(&raw)
        .with_context(|| format!("Apple Calendar enrollment is corrupt: {}", path.display()))?;
    if enrollment.schema_version != APPLE_CALENDAR_ENROLLMENT_SCHEMA {
        return Err(anyhow!(
            "Apple Calendar enrollment schema {} is unsupported; upgrade Heiwa rather than overwriting it",
            enrollment.schema_version
        ));
    }
    if enrollment.connector != "apple_calendar"
        || enrollment.installation_id.trim().is_empty()
        || enrollment.device_id.trim().is_empty()
    {
        return Err(anyhow!("Apple Calendar enrollment is incomplete"));
    }
    Ok(Some(enrollment))
}

fn current_apple_calendar_binding() -> Result<(String, String)> {
    let identity = heiwa_identity::load()?
        .ok_or_else(|| anyhow!("finish Heiwa first-run setup before connecting Apple Calendar"))?;
    let machine = heiwa_install::load_machine_manifest()
        .map_err(|error| anyhow!(error))?
        .ok_or_else(|| anyhow!("start Heiwa.app once before connecting Apple Calendar"))?;
    Ok((identity.installation_id, machine.device_id))
}

fn ensure_apple_calendar_binding_for_connect() -> Result<(String, String)> {
    let identity = heiwa_identity::load()?
        .ok_or_else(|| anyhow!("finish Heiwa first-run setup before connecting Apple Calendar"))?;
    let machine = match heiwa_install::load_machine_manifest().map_err(|error| anyhow!(error))? {
        Some(machine) => machine,
        None => {
            heiwa_install::refresh_machine_manifest_for_runtime(heiwa_install::MachineRuntime {
                version: env!("CARGO_PKG_VERSION").to_string(),
                channel: "local".to_string(),
                install_path: std::env::current_exe().context("resolve Heiwa executable")?,
            })?
        }
    };
    Ok((identity.installation_id, machine.device_id))
}

pub(crate) fn require_apple_calendar_connection() -> Result<()> {
    let enrollment = load_apple_calendar_enrollment()?
        .ok_or_else(|| anyhow!("Apple Calendar is not connected to this Heiwa profile"))?;
    let (installation_id, device_id) = current_apple_calendar_binding()?;
    if enrollment.installation_id != installation_id || enrollment.device_id != device_id {
        return Err(anyhow!(
            "Apple Calendar enrollment belongs to a different Heiwa installation or device; reconnect it"
        ));
    }
    Ok(())
}

pub(crate) fn apple_calendar_connection_payload() -> Value {
    match load_apple_calendar_enrollment() {
        Ok(None) => json!({
            "connector": "apple_calendar",
            "status": "disconnected",
            "detail": "Detected on this Mac, but not connected to this Heiwa profile.",
            "next_action": "heiwa connect apple-calendar --authorize",
        }),
        Ok(Some(enrollment)) => match current_apple_calendar_binding() {
            Ok((installation_id, device_id))
                if enrollment.installation_id == installation_id
                    && enrollment.device_id == device_id =>
            {
                json!({
                    "connector": "apple_calendar",
                    "status": "connected",
                    "detail": "Connected to this Heiwa profile on this device.",
                    "connected_at": enrollment.connected_at,
                    "scopes": enrollment.scopes,
                })
            }
            Ok(_) => json!({
                "connector": "apple_calendar",
                "status": "disconnected",
                "detail": "The saved enrollment belongs to another installation or device; reconnect it.",
                "next_action": "heiwa connect apple-calendar --authorize",
            }),
            Err(error) => json!({
                "connector": "apple_calendar",
                "status": "config_error",
                "detail": error.to_string(),
            }),
        },
        Err(error) => json!({
            "connector": "apple_calendar",
            "status": "config_error",
            "detail": error.to_string(),
        }),
    }
}

pub(crate) fn connect_apple_calendar() -> Result<Value> {
    // Parse any existing record before touching Calendar.app or writing. A
    // newer schema belongs to a newer Heiwa and must never be reset by this
    // build's reconnect path.
    let _existing = load_apple_calendar_enrollment()?;
    let (installation_id, device_id) = ensure_apple_calendar_binding_for_connect()?;
    let calendars = crate::cmd::calendar_apple::list_calendars()?;
    let enrollment = AppleCalendarEnrollment {
        schema_version: APPLE_CALENDAR_ENROLLMENT_SCHEMA.to_string(),
        connector: "apple_calendar".to_string(),
        installation_id,
        device_id,
        connected_at: chrono::Utc::now().to_rfc3339(),
        scopes: vec![
            "calendar.read".to_string(),
            "calendar.event.create_with_approval".to_string(),
        ],
    };
    write_owner_private_json(&apple_calendar_enrollment_path(), &enrollment)?;
    Ok(json!({
        "connector": "apple_calendar",
        "status": "connected",
        "resource_count": calendars.len(),
        "auth": {
            "mode": "macos_automation",
            "owner": "macOS",
            "secrets": "none",
        },
    }))
}

pub(crate) fn disconnect_apple_calendar() -> Result<Value> {
    let path = apple_calendar_enrollment_path();
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("remove Apple Calendar enrollment: {}", path.display()))?;
    }
    Ok(json!({
        "connector": "apple_calendar",
        "status": "disconnected",
        "read_models_preserved": true,
        "revoke": {
            "owner": "macOS",
            "path": "System Settings > Privacy & Security > Automation > heiwa > Calendar",
        },
    }))
}

fn write_owner_private_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("connector enrollment path has no parent"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = parent.join(format!(
        ".apple_calendar.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| -> Result<()> {
        let body = serde_json::to_vec_pretty(value)?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&body)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

async fn google_connect(connector: &str, args: &[String]) -> Result<()> {
    if let Some(path) = flag_value(args, "--client-secret") {
        return stage_client_secret(&path);
    }
    if has_flag(args, "--authorize") {
        if connector == "gmail" {
            return Err(anyhow!(
                "Gmail send authorization stays disabled until the approval-backed send executor exists"
            ));
        }
        return google_authorize(connector).await;
    }
    if has_flag(args, "--disconnect") {
        connector_vault()
            .delete(&connector_account_id(connector))
            .or_else(|error| match error {
                VaultError::NotFound { .. } => Ok(()),
                other => Err(other),
            })?;
        println!("{connector}: disconnected; local read models were preserved");
        return Ok(());
    }
    let client = google_client_probe();
    let lane_status = google_lane_status(connector);
    println!("{connector}");
    println!(
        "  client_id: {}",
        match client {
            ConnectorClientProbe::Configured => "staged",
            ConnectorClientProbe::Missing => "missing",
            ConnectorClientProbe::Invalid => "invalid",
        }
    );
    println!("  credential: {lane_status}");
    if client != ConnectorClientProbe::Configured {
        println!(
            "  next: heiwa connect {connector} --client-secret <downloaded-client-secret.json>"
        );
    } else if lane_status == "staged" {
        println!("  next: heiwa connect {connector} --authorize");
    } else if lane_status == "connected" {
        println!("  next: connected; read-model sync can use this token");
    }
    Ok(())
}

/// Extract the public client id from Google's downloaded desktop-app JSON.
/// The bundled client secret is not a secret for native apps and is neither
/// needed by PKCE nor persisted by Heiwa.
fn stage_client_secret(source: &str) -> Result<()> {
    let destination = client_config_path();
    stage_client_config_at(Path::new(source), &destination)?;
    println!("staged: {}", destination.display());
    println!("next: heiwa connect google-calendar --authorize");
    Ok(())
}

fn stage_client_config_at(source: &Path, destination: &Path) -> Result<()> {
    let raw = fs::read_to_string(source)
        .with_context(|| format!("cannot read client config file: {}", source.display()))?;
    let parsed: Value = serde_json::from_str(&raw).context("client secret is not valid JSON")?;
    let creds = parsed
        .get("installed")
        .ok_or_else(|| anyhow!("Google OAuth client must be created as a Desktop app"))?;
    let client_id = creds
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("client secret JSON missing client_id"))?;
    let bounded = json!({
        "schema_version": GOOGLE_CLIENT_SCHEMA,
        "client_id": client_id,
    });
    write_secret_file(
        destination,
        serde_json::to_string_pretty(&bounded)?.as_bytes(),
    )?;
    Ok(())
}

fn google_client_id() -> Result<String> {
    if let Ok(client_id) = std::env::var("HEIWA_GOOGLE_OAUTH_CLIENT_ID") {
        if !client_id.trim().is_empty() {
            return Ok(client_id);
        }
    }
    let raw = fs::read_to_string(client_config_path()).context(
        "no staged client secret; run: heiwa connect google-calendar --client-secret <path>",
    )?;
    let parsed: Value =
        serde_json::from_str(&raw).context("Google OAuth client config is corrupt")?;
    if parsed.get("schema_version").and_then(Value::as_str) != Some(GOOGLE_CLIENT_SCHEMA) {
        return Err(anyhow!("unsupported Google OAuth client config schema"));
    }
    parsed
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Google OAuth client config is missing client_id"))
}

fn google_provider_config(connector: &str) -> Result<ProviderConfig> {
    let scope = google_scopes(connector)
        .ok_or_else(|| anyhow!("no Google scope mapped for connector {connector}"))?;
    let mut config = ProviderConfig::google(google_client_id()?, vec![scope.to_string()]);
    if let Ok(endpoint) = std::env::var("HEIWA_GOOGLE_AUTH_URL") {
        if !endpoint.trim().is_empty() {
            config.auth_endpoint = endpoint;
        }
    }
    if let Ok(endpoint) = std::env::var("HEIWA_GOOGLE_TOKEN_URL") {
        if !endpoint.trim().is_empty() {
            config.token_endpoint = endpoint;
        }
    }
    Ok(config)
}

fn load_connector_oauth(connector: &str) -> std::result::Result<OAuthSecret, VaultError> {
    connector_vault().load_oauth(&connector_account_id(connector))
}

fn store_connector_oauth(connector: &str, secret: &OAuthSecret) -> Result<()> {
    connector_vault()
        .store_oauth(&connector_account_id(connector), secret)
        .context("store connector OAuth token in OS credential vault")
}

pub(crate) async fn connector_access_token(connector: &str) -> Result<String> {
    let secret = load_connector_oauth(connector)
        .with_context(|| format!("no usable {connector} credential in OS credential vault"))?;
    if heiwa_provider::needs_refresh(&secret, unix_now(), 120) {
        return refresh_connector_access_token(connector, &secret).await;
    }
    Ok(secret.access_token)
}

pub(crate) async fn force_refresh_connector_access_token(connector: &str) -> Result<String> {
    let existing = load_connector_oauth(connector)
        .with_context(|| format!("no usable {connector} credential in OS credential vault"))?;
    refresh_connector_access_token(connector, &existing).await
}

pub(crate) async fn refresh_connector_access_token(
    connector: &str,
    existing: &OAuthSecret,
) -> Result<String> {
    let refresh_token = existing.refresh_token.as_deref().ok_or_else(|| {
        anyhow!("{connector} credential has no refresh token; reconnect the account")
    })?;
    let config = google_provider_config(connector)?;
    let response = refresh(&reqwest::Client::new(), &config, refresh_token).await?;
    let merged = merge_refreshed(existing, &response, unix_now());
    let access_token = merged.access_token.clone();
    store_connector_oauth(connector, &merged)?;
    Ok(access_token)
}

/// Run the audited loopback PKCE flow from `heiwa_oauth` and store the token
/// only in the OS credential vault.
async fn google_authorize(connector: &str) -> Result<()> {
    let config = google_provider_config(connector)?;
    let listener = LoopbackListener::bind()?;
    let request = build_authorization_request(&config, listener.redirect_uri())?;

    let open_status = Command::new("open")
        .arg(&request.url)
        .status()
        .context("open Google consent page")?;
    if !open_status.success() {
        return Err(anyhow!("open Google consent page exited {open_status}"));
    }
    println!("waiting for Google redirect on {} …", request.redirect_uri);

    let expected_state = request.state.clone();
    let code = tokio::task::spawn_blocking(move || {
        listener.wait_for_code(&expected_state, Duration::from_secs(180))
    })
    .await
    .context("join OAuth callback listener")??;
    let response = exchange_code(
        &reqwest::Client::new(),
        &config,
        &code,
        request.pkce.verifier(),
        &request.redirect_uri,
    )
    .await?;
    store_connector_oauth(connector, &to_secret(&response, unix_now()))?;
    println!("{connector}: connected; token stored in OS credential vault");
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<()> {
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
    println!("  heiwa connect google-calendar --disconnect");
    println!();
    println!("Google Calendar uses loopback PKCE with calendar.readonly.");
    println!("The public client id lands in node config; tokens stay in the OS credential vault.");
}

// ---------------------------------------------------------------------------
// Read model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorCredentialProbe {
    Present,
    Missing,
    BackendError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorClientProbe {
    Configured,
    Missing,
    Invalid,
}

fn google_client_probe() -> ConnectorClientProbe {
    match std::env::var("HEIWA_GOOGLE_OAUTH_CLIENT_ID") {
        Ok(value) if !value.trim().is_empty() => return ConnectorClientProbe::Configured,
        Err(std::env::VarError::NotUnicode(_)) => return ConnectorClientProbe::Invalid,
        Ok(_) | Err(std::env::VarError::NotPresent) => {}
    }

    if !client_config_path().exists() {
        ConnectorClientProbe::Missing
    } else if google_client_id().is_ok() {
        ConnectorClientProbe::Configured
    } else {
        ConnectorClientProbe::Invalid
    }
}

fn connector_lane_status(
    credential: ConnectorCredentialProbe,
    client: ConnectorClientProbe,
) -> &'static str {
    match (credential, client) {
        (_, ConnectorClientProbe::Invalid) => "config_error",
        (ConnectorCredentialProbe::Present, _) => "connected",
        (ConnectorCredentialProbe::Missing, ConnectorClientProbe::Configured) => "staged",
        (ConnectorCredentialProbe::Missing, ConnectorClientProbe::Missing) => "needs_auth",
        (ConnectorCredentialProbe::BackendError, _) => "auth_error",
    }
}

/// Connector lane status used by calendar/mail summaries and /api/v1/connectors.
pub(crate) fn google_lane_status(connector: &str) -> &'static str {
    let credential = match load_connector_oauth(connector) {
        Ok(_) => ConnectorCredentialProbe::Present,
        Err(VaultError::NotFound { .. }) => ConnectorCredentialProbe::Missing,
        Err(_) => ConnectorCredentialProbe::BackendError,
    };
    connector_lane_status(credential, google_client_probe())
}

pub(crate) fn imap_configured() -> bool {
    crate::home::heiwa_home()
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
            "config_error" => Value::String("re-stage a valid Google desktop-app client config".into()),
            _ => Value::Null,
        },
    }));

    let apple_calendar = apple_calendar_connection_payload();
    let apple_calendar_status = apple_calendar
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("config_error");
    rows.push(json!({
        "id": "apple_calendar",
        "kind": "calendar",
        "display_name": "Apple Calendar",
        "status": apple_calendar_status,
        "auth_kind": "macos_automation",
        "scopes": "calendar.read calendar.event.create_with_approval",
        "detail": apple_calendar.get("detail").cloned().unwrap_or_else(|| Value::String(
            "Device-local Calendar.app bridge; external writes require approval.".into()
        )),
        "next_action": apple_calendar.get("next_action").cloned().unwrap_or(Value::Null),
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

    rows.push(json!({
        "id": "gmail",
        "kind": "mail",
        "display_name": "Gmail",
        "status": "planned",
        "auth_kind": "oauth_loopback_pkce",
        "scopes": google_scopes("gmail"),
        "detail": "Gmail reads are disabled. gmail.send remains ungranted until the approval-backed sender exists.",
        "next_action": Value::Null,
    }));

    let apple_mail_ready = crate::cmd::mail::apple_mail_accounts_present();
    rows.push(json!({
        "id": "apple_mail",
        "kind": "mail",
        "display_name": "Apple Mail",
        "status": if apple_mail_ready { "metadata" } else { "planned" },
        "auth_kind": "local_metadata",
        "detail": "Metadata-only lane (account, mailbox, sender, subject, date, unread); no body reads.",
        "next_action": if apple_mail_ready {
            Value::String("heiwa mail scan --source apple".into())
        } else {
            Value::Null
        },
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
            "OAuth tokens stay in the OS credential vault"
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_connector_accepts_dashes() {
        assert_eq!(normalize_connector_id("google-calendar"), "google_calendar");
        assert_eq!(normalize_connector_id("Apple-Mail"), "apple_mail");
    }

    #[test]
    fn gmail_connector_requests_send_only_never_restricted_read_access() {
        assert_eq!(
            google_scopes("gmail"),
            Some("https://www.googleapis.com/auth/gmail.send")
        );
    }

    #[test]
    fn staging_google_client_config_discards_downloaded_client_secret() {
        let root = tempfile::tempdir().expect("tempdir");
        let source = root.path().join("downloaded.json");
        let destination = root
            .path()
            .join("state/connectors/google_oauth_client.json");
        fs::write(
            &source,
            r#"{"installed":{"client_id":"public-id.apps.googleusercontent.com","client_secret":"must-not-persist"}}"#,
        )
        .expect("write source");

        stage_client_config_at(&source, &destination).expect("stage bounded config");

        let staged: Value = serde_json::from_slice(&fs::read(destination).expect("read staged"))
            .expect("staged JSON");
        assert_eq!(staged["schema_version"], "heiwa_google_oauth_client_v1");
        assert_eq!(staged["client_id"], "public-id.apps.googleusercontent.com");
        assert!(staged.get("client_secret").is_none());
        assert!(!staged.to_string().contains("must-not-persist"));
    }

    #[test]
    fn staging_rejects_a_google_web_client() {
        let root = tempfile::tempdir().expect("tempdir");
        let source = root.path().join("downloaded.json");
        let destination = root.path().join("google_oauth_client.json");
        fs::write(
            &source,
            r#"{"web":{"client_id":"wrong-client-class.apps.googleusercontent.com"}}"#,
        )
        .expect("write source");

        let error = stage_client_config_at(&source, &destination)
            .expect_err("web clients must not enter the native PKCE lane");

        assert!(error.to_string().contains("Desktop app"));
        assert!(!destination.exists());
    }

    #[test]
    fn connector_status_keeps_missing_distinct_from_vault_failure() {
        assert_eq!(
            connector_lane_status(
                ConnectorCredentialProbe::Missing,
                ConnectorClientProbe::Missing,
            ),
            "needs_auth"
        );
        assert_eq!(
            connector_lane_status(
                ConnectorCredentialProbe::Missing,
                ConnectorClientProbe::Configured,
            ),
            "staged"
        );
        assert_eq!(
            connector_lane_status(
                ConnectorCredentialProbe::BackendError,
                ConnectorClientProbe::Configured,
            ),
            "auth_error"
        );
        assert_eq!(
            connector_lane_status(
                ConnectorCredentialProbe::Missing,
                ConnectorClientProbe::Invalid,
            ),
            "config_error"
        );
    }
}
