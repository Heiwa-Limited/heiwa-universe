//! Selected EventKit resources projected into the profile's local calendar view.
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadRequest {
    pub calendar_ids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    calendar_ids: Vec<String>,
}

fn selection_path() -> PathBuf {
    super::calendar::calendar_state_dir().join("apple_selection.json")
}

pub(crate) fn selected_ids() -> Result<Vec<String>> {
    let raw = match fs::read(selection_path()) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let selection: Selection = serde_json::from_slice(&raw).context("read calendar selection")?;
    if selection.schema_version != 1 {
        bail!("Unsupported calendar selection version");
    }
    Ok(selection.calendar_ids)
}

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

fn call(request: &Value) -> Result<Value> {
    let helper = helper_path().ok_or_else(|| {
        anyhow!(
            "The Apple resource reader is missing. Install the complete Heiwa app and reopen it."
        )
    })?;
    let mut command = std::process::Command::new(helper);
    command.arg(request.to_string());
    let bytes = heiwa_core::subprocess::bounded_output(
        &mut command,
        std::time::Duration::from_secs(45),
        4 * 1024 * 1024,
    )
    .map_err(|error| {
        anyhow!("{error} Check Heiwa access in System Settings > Privacy & Security > Calendars.")
    })?;
    let value: Value = serde_json::from_slice(&bytes).context("read Apple resource response")?;
    if value["schema_version"] != 1 || value.get("error").is_some() {
        bail!("The Apple resource reader returned an unsupported or failed response");
    }
    Ok(value)
}

pub(crate) fn resources(request_access: bool) -> Result<Vec<Value>> {
    let result = call(&json!({"operation":"list", "request_access": request_access}))?;
    let resources = result["calendars"]
        .as_array()
        .context("invalid calendar resource list")?;
    let mut ids = HashSet::new();
    for resource in resources {
        let id = resource["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context("missing calendar identity")?;
        if !ids.insert(id)
            || resource["name"].as_str().is_none()
            || resource["writable"].as_bool().is_none()
        {
            bail!("Invalid calendar resource identity");
        }
    }
    Ok(resources.clone())
}

pub(crate) fn read_selected(request: ReadRequest) -> Result<Value> {
    let state = super::calendar::calendar_state_dir();
    fs::create_dir_all(&state)?;
    let _lock = snapshot_lock(&state.join("events.jsonl"))?;
    super::connectors::require_apple_calendar_connection()?;
    let _previous_selection = selected_ids()?;
    let existing = read_snapshot(&state.join("events.jsonl"))?;
    let ids = request.calendar_ids;
    if ids.is_empty()
        || ids.len() > 100
        || ids.iter().any(|id| id.is_empty() || id.len() > 1024)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        bail!("Select between 1 and 100 distinct calendars");
    }
    let now = Utc::now();
    let start = (now - Duration::days(31)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let end = (now + Duration::days(90)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let result = call(
        &json!({"operation":"scan", "calendar_ids":ids, "start":start, "end":end, "limit":500}),
    )?;
    if result["calendar_ids"] != json!(ids) || result["start"] != start || result["end"] != end {
        bail!("Calendar response did not match the selected resources and range");
    }
    let rows = result["events"]
        .as_array()
        .context("missing calendar events")?;
    let truncated = result["truncated"]
        .as_bool()
        .context("missing calendar completeness")?;
    validate_rows(rows, &ids)?;
    // Recheck the enrollment after reading; a disconnect must not admit a late result.
    super::connectors::require_apple_calendar_connection()?;
    let next = reconcile(existing, rows, &ids, &start, &end, !truncated);
    atomic_write(&state.join("events.jsonl"), &serialize_rows(&next))?;
    atomic_write(
        &selection_path(),
        &serde_json::to_vec(&Selection {
            schema_version: 1,
            calendar_ids: ids.clone(),
        })?,
    )?;
    let receipt = json!({"schema_version":1, "id":uuid::Uuid::new_v4().to_string(),
        "kind":"calendar_read", "source":"apple_calendar", "calendar_ids":ids,
        "fetched":rows.len(), "truncated":truncated, "start":start, "end":end,
        "read_at":Utc::now().to_rfc3339(), "external_writes":[]});
    let receipt_dir = state.join("receipts");
    fs::create_dir_all(&receipt_dir)?;
    atomic_write(
        &receipt_dir.join(format!("{}.json", receipt["id"].as_str().unwrap())),
        &serde_json::to_vec(&receipt)?,
    )?;
    Ok(receipt)
}

fn validate_rows(rows: &[Value], ids: &[String]) -> Result<()> {
    if rows.len() > 500 {
        bail!("Calendar event limit exceeded");
    }
    for row in rows {
        if row["source"] != "apple_calendar"
            || !row["calendar_id"]
                .as_str()
                .is_some_and(|id| ids.iter().any(|selected| selected == id))
            || row["external_id"].as_str().is_none_or(|id| id.is_empty())
            || row["title"].as_str().is_none()
            || row["occurrence"].as_str().is_none()
            || row["start"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .is_none()
            || row["end"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .is_none()
        {
            bail!("Calendar response contained an invalid or unselected event");
        }
    }
    Ok(())
}

fn reconcile(
    existing: Vec<Value>,
    rows: &[Value],
    ids: &[String],
    start: &str,
    end: &str,
    complete: bool,
) -> Vec<Value> {
    let normalized: Vec<Value> = rows
        .iter()
        .cloned()
        .map(super::calendar::ensure_event_identity)
        .collect();
    let replaced: HashSet<&str> = normalized
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    let mut next: Vec<Value> = existing
        .into_iter()
        .filter(|row| {
            if row["id"].as_str().is_some_and(|id| replaced.contains(id)) {
                return false;
            }
            let selected = row["source"] == "apple_calendar"
                && row["calendar_id"]
                    .as_str()
                    .is_some_and(|id| ids.iter().any(|v| v == id));
            let in_range = row["start"].as_str().is_some_and(|value| value < end)
                && row["end"].as_str().is_some_and(|value| value > start);
            !(complete && selected && in_range)
        })
        .collect();
    next.extend(normalized);
    next
}

pub(super) fn snapshot_lock(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(path.with_extension("lock"))?;
    lock.lock()?;
    Ok(lock)
}

pub(super) fn read_snapshot(path: &Path) -> Result<Vec<Value>> {
    if path.exists() && fs::metadata(path)?.len() > 16 * 1024 * 1024 {
        bail!("Calendar snapshot exceeds the read bound; existing data was preserved");
    }
    match fs::read_to_string(path) {
        Ok(raw) => raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(Into::into))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn serialize_rows(rows: &[Value]) -> Vec<u8> {
    rows.iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>()
        .into_bytes()
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("missing state directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: &str, calendar: &str, title: &str) -> Value {
        super::super::calendar::ensure_event_identity(
            json!({"source":"apple_calendar", "calendar_id":calendar,
            "external_id":id, "occurrence":"", "title":title, "start":"2026-09-12T12:00:00Z", "end":"2026-09-12T13:00:00Z"}),
        )
    }
    #[test]
    fn complete_refresh_updates_and_deletes_only_the_selected_window() {
        let old = event("one", "selected", "Old title");
        let changed = event("one", "selected", "New title");
        assert_eq!(old["id"], changed["id"]);
        let other = event("private", "other", "Other source");
        let next = reconcile(
            vec![old, event("deleted", "selected", "Deleted"), other.clone()],
            std::slice::from_ref(&changed),
            &["selected".into()],
            "2026-09-01",
            "2026-10-01",
            true,
        );
        assert_eq!(next, vec![other, changed]);
    }
    #[test]
    fn partial_refresh_cannot_infer_deletion_or_import_another_calendar() {
        let old = event("one", "selected", "Existing");
        let next = reconcile(
            vec![old.clone()],
            &[],
            &["selected".into()],
            "2026-09-01",
            "2026-10-01",
            false,
        );
        assert_eq!(next, vec![old]);
        assert!(
            validate_rows(&[event("two", "other", "Unselected")], &["selected".into()]).is_err()
        );
    }
    #[test]
    fn damaged_snapshot_is_preserved_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        fs::write(&path, "not json\n").unwrap();
        assert!(read_snapshot(&path).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "not json\n");
    }
}
