//! Shared mechanics for native Apple resources (Calendar, Reminders).
//!
//! One implementation of: discovering the EventKit helper and making a bounded
//! request to it, the installation- and device-bound enrollment that admits a
//! resource to this Heiwa profile, and the atomic selection of which resources
//! a profile may read. Reads, permission prompts, and effects stay with each
//! resource. Enrollment for one resource never admits another.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppleResource {
    Calendar,
    Reminders,
}

impl AppleResource {
    pub(crate) fn connector(self) -> &'static str {
        match self {
            Self::Calendar => "apple_calendar",
            Self::Reminders => "apple_reminders",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Calendar => "Apple Calendar",
            Self::Reminders => "Apple Reminders",
        }
    }

    pub(crate) fn cli_name(self) -> &'static str {
        match self {
            Self::Calendar => "apple-calendar",
            Self::Reminders => "apple-reminders",
        }
    }

    /// The System Settings privacy pane that owns this resource's permission.
    pub(crate) fn privacy_pane(self) -> &'static str {
        match self {
            Self::Calendar => "Calendars",
            Self::Reminders => "Reminders",
        }
    }
}

// ---------------------------------------------------------------- helper

pub(crate) fn helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HEIWA_APPLE_RESOURCES_HELPER").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(path));
    }
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("heiwa-apple-resources"))
        .filter(|path| path.is_file())
}

/// One bounded request to the EventKit helper over stdin.
pub(crate) fn helper_request(
    resource: AppleResource,
    request: &Value,
    timeout: std::time::Duration,
    max_output_bytes: usize,
    missing_message: &'static str,
    response_context: &'static str,
) -> Result<Value> {
    let helper = helper_path().ok_or_else(|| anyhow!(missing_message))?;
    let mut command = std::process::Command::new(helper);
    // launchd labels the always-on runtime with its XPC service identity. That
    // identity is not valid for the standalone EventKit reader and causes
    // TCC to reject an otherwise authorized read. Keep the service identity
    // on Heiwa itself, but do not pass it to the native helper.
    command.env_remove("XPC_SERVICE_NAME");
    let request = serde_json::to_vec(request).context("serialize Apple resource request")?;
    let bytes = heiwa_core::subprocess::bounded_output_with_input(
        &mut command,
        &request,
        timeout,
        max_output_bytes,
    )
    .map_err(|error| {
        anyhow!(
            "{error} Check Heiwa access in System Settings > Privacy & Security > {}.",
            resource.privacy_pane()
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).context(response_context)?;
    Ok(value)
}

// ------------------------------------------------------------ enrollment

pub(crate) const ENROLLMENT_SCHEMA: &str = "heiwa_connector_enrollment_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Enrollment {
    pub schema_version: String,
    pub connector: String,
    pub installation_id: String,
    pub device_id: String,
    pub connected_at: String,
    pub scopes: Vec<String>,
}

pub(crate) fn enrollment_path(resource: AppleResource) -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("connectors")
        .join(format!("{}.json", resource.connector()))
}

pub(crate) fn load_enrollment(resource: AppleResource) -> Result<Option<Enrollment>> {
    let label = resource.label();
    let path = enrollment_path(resource);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {label} enrollment: {}", path.display()))?;
    let enrollment: Enrollment = serde_json::from_str(&raw)
        .with_context(|| format!("{label} enrollment is corrupt: {}", path.display()))?;
    if enrollment.schema_version != ENROLLMENT_SCHEMA {
        return Err(anyhow!(
            "{label} enrollment schema {} is unsupported; upgrade Heiwa rather than overwriting it",
            enrollment.schema_version
        ));
    }
    if enrollment.connector != resource.connector()
        || enrollment.installation_id.trim().is_empty()
        || enrollment.device_id.trim().is_empty()
    {
        return Err(anyhow!("{label} enrollment is incomplete"));
    }
    Ok(Some(enrollment))
}

fn current_binding(resource: AppleResource) -> Result<(String, String)> {
    let label = resource.label();
    let identity = heiwa_identity::load()?
        .ok_or_else(|| anyhow!("finish Heiwa first-run setup before connecting {label}"))?;
    let machine = heiwa_install::load_machine_manifest()
        .map_err(|error| anyhow!(error))?
        .ok_or_else(|| anyhow!("start Heiwa.app once before connecting {label}"))?;
    Ok((identity.installation_id, machine.device_id))
}

fn ensure_binding_for_connect(resource: AppleResource) -> Result<(String, String)> {
    let label = resource.label();
    let identity = heiwa_identity::load()?
        .ok_or_else(|| anyhow!("finish Heiwa first-run setup before connecting {label}"))?;
    let machine = match heiwa_install::load_machine_manifest().map_err(|error| anyhow!(error))? {
        Some(machine) => machine,
        None => {
            heiwa_install::refresh_machine_manifest_for_runtime(heiwa_install::MachineRuntime {
                version: env!("CARGO_PKG_VERSION").to_string(),
                channel: super::app::runtime_channel(),
                install_path: std::env::current_exe().context("resolve Heiwa executable")?,
            })?
        }
    };
    Ok((identity.installation_id, machine.device_id))
}

/// Refuse unless this resource is enrolled for this installation and device.
pub(crate) fn require_connection(resource: AppleResource) -> Result<()> {
    let label = resource.label();
    let enrollment = load_enrollment(resource)?
        .ok_or_else(|| anyhow!("{label} is not connected to this Heiwa profile"))?;
    let (installation_id, device_id) = current_binding(resource)?;
    if enrollment.installation_id != installation_id || enrollment.device_id != device_id {
        return Err(anyhow!(
            "{label} enrollment belongs to a different Heiwa installation or device; reconnect it"
        ));
    }
    Ok(())
}

pub(crate) fn connection_payload(resource: AppleResource) -> Value {
    let connector = resource.connector();
    let next_action = format!("heiwa connect {} --authorize", resource.cli_name());
    match load_enrollment(resource) {
        Ok(None) => json!({
            "connector": connector,
            "status": "disconnected",
            "detail": "Detected on this Mac, but not connected to this Heiwa profile.",
            "next_action": next_action,
        }),
        Ok(Some(enrollment)) => match current_binding(resource) {
            Ok((installation_id, device_id))
                if enrollment.installation_id == installation_id
                    && enrollment.device_id == device_id =>
            {
                json!({
                    "connector": connector,
                    "status": "connected",
                    "detail": "Connected to this Heiwa profile on this device.",
                    "connected_at": enrollment.connected_at,
                    "scopes": enrollment.scopes,
                })
            }
            Ok(_) => json!({
                "connector": connector,
                "status": "disconnected",
                "detail": "The saved enrollment belongs to another installation or device; reconnect it.",
                "next_action": next_action,
            }),
            Err(error) => json!({
                "connector": connector,
                "status": "config_error",
                "detail": error.to_string(),
            }),
        },
        Err(error) => json!({
            "connector": connector,
            "status": "config_error",
            "detail": error.to_string(),
        }),
    }
}

/// Enroll a resource for this installation and device. `authorize` asks the
/// helper for this resource's own macOS permission and returns how many
/// resources it can see; nothing is written unless it succeeds.
pub(crate) fn connect(
    resource: AppleResource,
    scopes: &[&str],
    authorize: impl FnOnce() -> Result<usize>,
) -> Result<usize> {
    // Parse any existing record before touching the resource or writing. A
    // newer schema belongs to a newer Heiwa and must never be reset by this
    // build's reconnect path.
    let _existing = load_enrollment(resource)?;
    let (installation_id, device_id) = ensure_binding_for_connect(resource)?;
    let resource_count = authorize()?;
    let enrollment = Enrollment {
        schema_version: ENROLLMENT_SCHEMA.to_string(),
        connector: resource.connector().to_string(),
        installation_id,
        device_id,
        connected_at: chrono::Utc::now().to_rfc3339(),
        scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
    };
    super::connectors::write_owner_private_json(&enrollment_path(resource), &enrollment)?;
    Ok(resource_count)
}

pub(crate) fn remove_enrollment(resource: AppleResource) -> Result<()> {
    let path = enrollment_path(resource);
    if path.exists() {
        fs::remove_file(&path).with_context(|| {
            format!("remove {} enrollment: {}", resource.label(), path.display())
        })?;
    }
    Ok(())
}

// ------------------------------------------------------------- selection

/// Selected resource ids stored as `{"schema_version":1,"<field>":[...]}`.
/// A missing file is an empty selection; callers must refuse to read with
/// one rather than treat it as "everything".
pub(crate) fn load_selection(path: &Path, field: &str, label: &str) -> Result<Vec<String>> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let value: Value =
        serde_json::from_slice(&raw).with_context(|| format!("read {label} selection"))?;
    let object = value
        .as_object()
        .filter(|object| {
            object.len() == 2 && object.contains_key("schema_version") && object.contains_key(field)
        })
        .ok_or_else(|| anyhow!("read {label} selection: unexpected fields"))?;
    if object["schema_version"] != 1 {
        bail!("Unsupported {label} selection version");
    }
    object[field]
        .as_array()
        .and_then(|ids| {
            ids.iter()
                .map(|id| id.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| anyhow!("read {label} selection: ids must be strings"))
}

pub(crate) fn store_selection(path: &Path, field: &str, ids: &[String]) -> Result<()> {
    // Field order matches the original Calendar selection file byte for byte.
    let body = format!(
        "{{\"schema_version\":1,{}:{}}}",
        serde_json::to_string(field)?,
        serde_json::to_string(ids)?
    );
    atomic_write(path, body.as_bytes())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("missing state directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
