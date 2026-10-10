//! Selected EventKit resources projected into the profile's local calendar view.
//!
//! A read covers a fixed window around now. The helper answers at most
//! `SCAN_PAGE_LIMIT` events per request, in start order, so a busy calendar is
//! read in pages: each truncated page proves everything starting before its
//! last event was seen, and the next page resumes there. Only a range proven
//! complete may delete rows, so a partial read never erases unseen events.
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use super::apple_resources::{helper_request, load_selection, store_selection, AppleResource};
// Calendar callers keep their paths; the mechanics live in `apple_resources`.
pub(super) use super::apple_resources::atomic_write;
pub(crate) use super::apple_resources::helper_path;

/// Events the helper returns per request; its protocol maximum.
const SCAN_PAGE_LIMIT: usize = 500;
/// Pages one read may take. Bounds a pathological calendar, not a busy one.
const MAX_SCAN_PAGES: usize = 12;
/// How old a read may be before a sync re-reads, unless the caller says.
const DEFAULT_SYNC_MAX_AGE_SECONDS: u64 = 120;
/// Least wait before a sync relaunches a reader that just failed.
const FAILED_SYNC_BACKOFF_SECONDS: i64 = 60;
const MAX_CATALOG_RESOURCES: usize = 1000;
const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
const CATALOG_MAX_AGE_SECONDS: i64 = 300;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadRequest {
    pub calendar_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectRequest {
    pub calendar_ids: Vec<String>,
    pub catalog_revision: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceCatalog {
    schema_version: u32,
    connected_at: String,
    observed_at: DateTime<Utc>,
    revision: String,
    calendars: Vec<Value>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SyncRequest {
    #[serde(default)]
    pub max_age_seconds: Option<u64>,
    #[serde(default)]
    pub force: bool,
}

/// The outcome of the last read attempt, for freshness and backoff.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyncState {
    schema_version: u32,
    last_attempt_at: Option<DateTime<Utc>>,
    last_read_at: Option<DateTime<Utc>>,
    complete: Option<bool>,
    fetched: Option<usize>,
    error: Option<String>,
    #[serde(default)]
    calendar_ids: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum SyncDecision {
    Fresh,
    BackOff,
    Read,
}

fn selection_path() -> PathBuf {
    super::calendar::calendar_state_dir().join("apple_selection.json")
}

fn sync_state_path(state: &Path) -> PathBuf {
    state.join("apple_sync.json")
}

fn load_sync_state(state: &Path) -> SyncState {
    // Sync state is a cache of the last outcome. A missing or damaged file only
    // costs one extra read, so it never blocks one.
    fs::read(sync_state_path(state))
        .ok()
        .and_then(|raw| serde_json::from_slice::<SyncState>(&raw).ok())
        .filter(|saved| saved.schema_version == 1)
        .unwrap_or_default()
}

fn record_sync_outcome(
    state: &Path,
    attempted_at: DateTime<Utc>,
    outcome: &Result<Value>,
    ids: &[String],
) {
    let previous = sync_state_for_selection(state, ids);
    let next = match outcome {
        Ok(receipt) => SyncState {
            schema_version: 1,
            last_attempt_at: Some(attempted_at),
            last_read_at: Some(attempted_at),
            complete: receipt["complete"].as_bool(),
            fetched: receipt["fetched"].as_u64().map(|count| count as usize),
            error: None,
            calendar_ids: serde_json::from_value(receipt["calendar_ids"].clone())
                .unwrap_or_default(),
        },
        Err(error) => SyncState {
            schema_version: 1,
            last_attempt_at: Some(attempted_at),
            error: Some(error.to_string()),
            calendar_ids: ids.to_vec(),
            ..previous
        },
    };
    if let Ok(bytes) = serde_json::to_vec(&next) {
        let _ = atomic_write(&sync_state_path(state), &bytes);
    }
}

fn sync_decision(
    saved: &SyncState,
    now: DateTime<Utc>,
    max_age: Duration,
    force: bool,
) -> SyncDecision {
    if force {
        return SyncDecision::Read;
    }
    let failed_last = saved.error.is_some()
        && saved.last_attempt_at.is_some()
        && saved.last_attempt_at >= saved.last_read_at;
    if failed_last {
        let backoff = max_age.min(Duration::seconds(FAILED_SYNC_BACKOFF_SECONDS));
        if saved
            .last_attempt_at
            .is_some_and(|attempt| now - attempt < backoff)
        {
            return SyncDecision::BackOff;
        }
        return SyncDecision::Read;
    }
    match saved.last_read_at {
        Some(read_at) if now - read_at < max_age => SyncDecision::Fresh,
        _ => SyncDecision::Read,
    }
}

fn status_payload(status: &str, saved: &SyncState, selected_count: usize) -> Value {
    json!({
        "status": status,
        "last_read_at": saved.last_read_at,
        "last_attempt_at": saved.last_attempt_at,
        "complete": saved.complete,
        "fetched": saved.fetched,
        "selected_count": selected_count,
        "error": if status == "error" { saved.error.clone() } else { None },
    })
}

/// Freshness of the selected Apple calendars, for read models to report.
pub(crate) fn sync_status() -> Value {
    if super::connectors::require_apple_calendar_connection().is_err() {
        return json!({"status": "not_connected"});
    }
    let ids = match selected_ids() {
        Ok(ids) => ids,
        Err(error) => return json!({"status": "error", "error": error.to_string()}),
    };
    if ids.is_empty() {
        return json!({"status": "no_selection", "selected_count": 0});
    }
    let saved = sync_state_for_selection(&super::calendar::calendar_state_dir(), &ids);
    let status = if saved.error.is_some() && saved.last_attempt_at >= saved.last_read_at {
        "error"
    } else if saved.last_read_at.is_some() {
        "fresh"
    } else {
        "never"
    };
    status_payload(status, &saved, ids.len())
}

/// Re-read the saved selection when the last read is older than the caller
/// allows. Concurrent callers queue on the snapshot lock and the later ones
/// find the read the first one just made, so a burst costs one EventKit scan.
pub(crate) fn sync_selected(request: SyncRequest) -> Result<Value> {
    if super::connectors::require_apple_calendar_connection().is_err() {
        return Ok(json!({"status": "not_connected"}));
    }
    let state = super::calendar::calendar_state_dir();
    fs::create_dir_all(&state)?;
    let _lock = snapshot_lock(&state.join("events.jsonl"))?;
    // Selection, read and disconnect share this lock. A queued sync must use
    // the selection current when it acquires the lock, not a previous tick.
    let ids = selected_ids()?;
    if ids.is_empty() {
        return Ok(json!({"status": "no_selection", "selected_count": 0}));
    }
    let max_age = Duration::seconds(
        request
            .max_age_seconds
            .unwrap_or(DEFAULT_SYNC_MAX_AGE_SECONDS)
            .min(86_400) as i64,
    );
    let saved = sync_state_for_selection(&state, &ids);
    match sync_decision(&saved, Utc::now(), max_age, request.force) {
        SyncDecision::Fresh => return Ok(status_payload("fresh", &saved, ids.len())),
        SyncDecision::BackOff => return Ok(status_payload("error", &saved, ids.len())),
        SyncDecision::Read => {}
    }
    let count = ids.len();
    let outcome = read_and_record(&state, ids);
    let saved = load_sync_state(&state);
    Ok(match outcome {
        Ok(_) => status_payload("synced", &saved, count),
        Err(_) => status_payload("error", &saved, count),
    })
}

pub(crate) fn selected_ids() -> Result<Vec<String>> {
    load_selection(&selection_path(), "calendar_ids", "calendar")
}

fn sync_state_for_selection(state: &Path, ids: &[String]) -> SyncState {
    let saved = load_sync_state(state);
    if saved.calendar_ids.iter().collect::<HashSet<_>>() == ids.iter().collect::<HashSet<_>>() {
        saved
    } else {
        SyncState::default()
    }
}

fn catalog_path(state: &Path) -> PathBuf {
    state.join("apple_catalog.json")
}

fn enrollment_time() -> Result<String> {
    super::connectors::require_apple_calendar_connection()?;
    super::connectors::apple_calendar_connection_payload()["connected_at"]
        .as_str()
        .map(str::to_owned)
        .context("Calendar enrollment has no connection identity")
}

fn cache_catalog(state: &Path, calendars: Vec<Value>, connected_at: String) -> Result<()> {
    let mut identities: Vec<&str> = calendars
        .iter()
        .filter_map(|calendar| calendar["id"].as_str())
        .collect();
    identities.sort_unstable();
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!([connected_at, identities]))?)
    );
    let catalog = ResourceCatalog {
        schema_version: 1,
        connected_at,
        observed_at: Utc::now(),
        revision,
        calendars,
    };
    let bytes = serde_json::to_vec(&catalog)?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        bail!("Calendar resource catalog exceeds its storage bound");
    }
    atomic_write(&catalog_path(state), &bytes)
}

fn load_catalog(state: &Path, connected_at: &str, now: DateTime<Utc>) -> Result<ResourceCatalog> {
    let path = catalog_path(state);
    if fs::metadata(&path)?.len() > MAX_CATALOG_BYTES {
        bail!("Calendar resource catalog exceeds its read bound");
    }
    let catalog: ResourceCatalog = serde_json::from_slice(&fs::read(path)?)?;
    validate_resources(&catalog.calendars)?;
    if catalog.schema_version != 1
        || catalog.connected_at != connected_at
        || catalog.observed_at > now + Duration::seconds(60)
        || now - catalog.observed_at > Duration::seconds(CATALOG_MAX_AGE_SECONDS)
    {
        bail!("Calendar resource catalog is stale; refresh the calendar list and retry");
    }
    Ok(catalog)
}

/// The last successful bounded inventory; exposing its revision performs no
/// EventKit read. A disconnected or never-listed profile has no catalog.
pub(crate) fn selection_catalog_revision() -> Result<Option<String>> {
    let state = super::calendar::calendar_state_dir();
    if !catalog_path(&state).exists() {
        return Ok(None);
    }
    Ok(Some(
        load_catalog(&state, &enrollment_time()?, Utc::now())?.revision,
    ))
}

/// Save an explicit local choice independently of event importing. Empty
/// choices disable future syncs; they never mean every calendar.
pub(crate) fn select_calendars(request: SelectRequest) -> Result<Value> {
    super::connectors::require_apple_calendar_connection()?;
    let state = super::calendar::calendar_state_dir();
    fs::create_dir_all(&state)?;
    let _lock = snapshot_lock(&state.join("events.jsonl"))?;
    save_selection_locked(&state, request, &enrollment_time()?, Utc::now())
}

fn save_selection_locked(
    state: &Path,
    request: SelectRequest,
    connected_at: &str,
    now: DateTime<Utc>,
) -> Result<Value> {
    validate_selection_ids(&request.calendar_ids)?;
    let catalog = load_catalog(state, connected_at, now)
        .context("Refresh the calendar list before saving choices")?;
    if request.catalog_revision != catalog.revision {
        bail!("Calendar resource catalog changed; refresh the calendar list and retry");
    }
    let known: HashSet<_> = catalog
        .calendars
        .iter()
        .filter_map(|calendar| calendar["id"].as_str())
        .collect();
    if request
        .calendar_ids
        .iter()
        .any(|id| !known.contains(id.as_str()))
    {
        bail!("A selected calendar is no longer in the resource catalog; refresh and retry");
    }
    let selection = state.join("apple_selection.json");
    // A newer or damaged selection belongs to recovery, not silent replacement.
    let _previous = load_selection(&selection, "calendar_ids", "calendar")?;
    store_selection(&selection, "calendar_ids", &request.calendar_ids)?;
    Ok(json!({"selected_ids":request.calendar_ids,"catalog_revision":catalog.revision}))
}

fn call(request: &Value) -> Result<Value> {
    let value = helper_request(
        AppleResource::Calendar,
        request,
        std::time::Duration::from_secs(45),
        4 * 1024 * 1024,
        "The Apple resource reader is missing. Install the complete Heiwa app and reopen it.",
        "read Apple resource response",
    )?;
    if value["schema_version"] != 1 || value.get("error").is_some() {
        bail!("The Apple resource reader returned an unsupported or failed response");
    }
    Ok(value)
}

pub(crate) fn resources(request_access: bool) -> Result<Vec<Value>> {
    let outcome = (|| {
        let result = call(&json!({"operation":"list", "request_access": request_access}))?;
        let resources = result["calendars"]
            .as_array()
            .context("invalid calendar resource list")?;
        validate_resources(resources)?;
        Ok(resources.clone())
    })();
    if !request_access {
        let state = super::calendar::calendar_state_dir();
        fs::create_dir_all(&state)?;
        let _lock = snapshot_lock(&state.join("events.jsonl"))?;
        match (&outcome, enrollment_time()) {
            (Ok(calendars), Ok(connected_at)) => {
                cache_catalog(&state, calendars.clone(), connected_at)?
            }
            _ if catalog_path(&state).exists() => fs::remove_file(catalog_path(&state))?,
            _ => {}
        }
    }
    outcome
}

fn validate_resources(resources: &[Value]) -> Result<()> {
    if resources.len() > MAX_CATALOG_RESOURCES {
        bail!("Calendar resource catalog exceeds its resource bound");
    }
    let mut ids = HashSet::new();
    for resource in resources {
        let id = resource["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .context("missing calendar identity")?;
        if !ids.insert(id)
            || resource["name"].as_str().is_none()
            || resource["writable"].as_bool().is_none()
        {
            bail!("Invalid calendar resource identity");
        }
    }
    Ok(())
}

pub(crate) fn read_selected(request: ReadRequest) -> Result<Value> {
    read_selection(request, false)
}

/// HTTP importing uses only choices the client already acknowledged. A later
/// save by another client must not be rolled back by a queued, older read.
pub(crate) fn read_saved_selection(request: ReadRequest) -> Result<Value> {
    read_selection(request, true)
}

fn read_selection(request: ReadRequest, require_saved: bool) -> Result<Value> {
    let state = super::calendar::calendar_state_dir();
    fs::create_dir_all(&state)?;
    let _lock = snapshot_lock(&state.join("events.jsonl"))?;
    // Refusals are answered before anything is recorded: a malformed request
    // or a disconnected profile is not a failed sync, and recording one would
    // hold the next background sync in backoff.
    super::connectors::require_apple_calendar_connection()?;
    validate_selection(&request.calendar_ids)?;
    if require_saved && request.calendar_ids != selected_ids()? {
        bail!("Calendar choices changed; refresh the saved selection before syncing events");
    }
    read_and_record(&state, request.calendar_ids)
}

fn validate_selection(ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        bail!("Select between 1 and 100 distinct calendars");
    }
    validate_selection_ids(ids)
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
}

fn validate_selection_ids(ids: &[String]) -> Result<()> {
    if ids.len() > 100
        || ids.iter().any(|id| !valid_id(id))
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        bail!("Select at most 100 distinct calendars with valid identities");
    }
    Ok(())
}

/// Read under the caller's snapshot lock and remember the outcome.
fn read_and_record(state: &Path, ids: Vec<String>) -> Result<Value> {
    let attempted_at = Utc::now();
    let outcome = read_locked(state, ids.clone());
    record_sync_outcome(state, attempted_at, &outcome, &ids);
    outcome
}

fn read_locked(state: &Path, ids: Vec<String>) -> Result<Value> {
    super::connectors::require_apple_calendar_connection()?;
    let _previous_selection = selected_ids()?;
    let existing = read_snapshot(&state.join("events.jsonl"))?;
    validate_selection(&ids)?;
    let now = whole_second(Utc::now());
    let start = now - Duration::days(31);
    let end = now + Duration::days(90);
    let scan = scan_window(&ids, start, end, SCAN_PAGE_LIMIT, call)?;
    let complete = scan.covered_until >= end;
    // Recheck the enrollment after reading; a disconnect must not admit a late result.
    super::connectors::require_apple_calendar_connection()?;
    let (next, fetched) = reconcile(existing, &scan.rows, &ids, start, scan.covered_until);
    atomic_write(&state.join("events.jsonl"), &serialize_rows(&next))?;
    store_selection(&selection_path(), "calendar_ids", &ids)?;
    let receipt = json!({"schema_version":1, "id":uuid::Uuid::new_v4().to_string(),
        "kind":"calendar_read", "source":"apple_calendar", "calendar_ids":ids,
        "fetched":fetched, "truncated":!complete, "complete":complete, "pages":scan.pages,
        "start":rfc3339(start), "end":rfc3339(end), "covered_until":rfc3339(scan.covered_until),
        "read_at":Utc::now().to_rfc3339(), "external_writes":[]});
    let receipt_dir = state.join("receipts");
    fs::create_dir_all(&receipt_dir)?;
    atomic_write(
        &receipt_dir.join(format!("{}.json", receipt["id"].as_str().unwrap())),
        &serde_json::to_vec(&receipt)?,
    )?;
    Ok(receipt)
}

fn whole_second(instant: DateTime<Utc>) -> DateTime<Utc> {
    instant.with_nanosecond(0).unwrap_or(instant)
}

fn rfc3339(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn instant(value: &Value) -> Option<DateTime<Utc>> {
    value
        .as_str()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| parsed.with_timezone(&Utc))
}

struct Scan {
    rows: Vec<Value>,
    /// Every event overlapping `[start, covered_until)` was returned.
    covered_until: DateTime<Utc>,
    pages: usize,
}

fn scan_window(
    ids: &[String],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: usize,
    reader: impl Fn(&Value) -> Result<Value>,
) -> Result<Scan> {
    let mut rows = Vec::new();
    let mut cursor = start;
    for page in 1..=MAX_SCAN_PAGES {
        let (from, to) = (rfc3339(cursor), rfc3339(end));
        let result = reader(
            &json!({"operation":"scan", "calendar_ids":ids, "start":from, "end":to, "limit":limit}),
        )?;
        if result["calendar_ids"] != json!(ids) || result["start"] != from || result["end"] != to {
            bail!("Calendar response did not match the selected resources and range");
        }
        let events = result["events"]
            .as_array()
            .context("missing calendar events")?;
        let truncated = result["truncated"]
            .as_bool()
            .context("missing calendar completeness")?;
        if events.len() > limit {
            bail!("Calendar event limit exceeded");
        }
        validate_rows(events, ids)?;
        rows.extend(events.iter().cloned());
        if !truncated {
            return Ok(Scan {
                rows,
                covered_until: end,
                pages: page,
            });
        }
        // Events arrive in start order, so anything omitted starts at or after
        // the last one returned: everything before it is proven seen.
        let last_start = events.iter().filter_map(|row| instant(&row["start"])).max();
        match last_start.map(whole_second) {
            Some(next) if next > cursor => cursor = next,
            // No progress is possible (an empty page, or more simultaneous
            // starts than one page holds). Claim only what was covered.
            _ => {
                return Ok(Scan {
                    rows,
                    covered_until: cursor,
                    pages: page,
                })
            }
        }
    }
    Ok(Scan {
        rows,
        covered_until: cursor,
        pages: MAX_SCAN_PAGES,
    })
}

fn validate_rows(rows: &[Value], ids: &[String]) -> Result<()> {
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

/// Merge a read into the snapshot. Returns the next rows and how many distinct
/// events the read returned.
///
/// Rows the read returned replace their previous versions. A selected row the
/// read did not return is deleted only when it overlaps `[start,
/// covered_until)`, the range the read proved complete; elsewhere absence
/// proves nothing and the row is kept.
fn reconcile(
    existing: Vec<Value>,
    rows: &[Value],
    ids: &[String],
    start: DateTime<Utc>,
    covered_until: DateTime<Utc>,
) -> (Vec<Value>, usize) {
    // Pages overlap at their boundaries, so one event can arrive twice.
    let mut seen = HashSet::new();
    let mut normalized: Vec<Value> = Vec::with_capacity(rows.len());
    for row in rows
        .iter()
        .cloned()
        .map(super::calendar::ensure_event_identity)
    {
        if row["id"]
            .as_str()
            .is_some_and(|id| seen.insert(id.to_string()))
        {
            normalized.push(row);
        }
    }
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
            let proven_absent = covered_until > start
                && instant(&row["start"]).is_some_and(|value| value < covered_until)
                && instant(&row["end"]).is_some_and(|value| value > start);
            !(selected && proven_absent)
        })
        .collect();
    let fetched = normalized.len();
    next.extend(normalized);
    (next, fetched)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn selection_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        cache_catalog(
            dir.path(),
            vec![
                json!({"id":"work","name":"Work","writable":true}),
                json!({"id":"life","name":"Life","writable":true}),
            ],
            "connected-1".into(),
        )
        .unwrap();
        store_selection(
            &dir.path().join("apple_selection.json"),
            "calendar_ids",
            &["work".into()],
        )
        .unwrap();
        dir
    }

    fn save_choices(state: &Path, ids: Vec<String>) -> Result<Value> {
        let catalog = load_catalog(state, "connected-1", Utc::now())?;
        save_selection_locked(
            state,
            SelectRequest {
                calendar_ids: ids,
                catalog_revision: catalog.revision,
            },
            "connected-1",
            Utc::now(),
        )
    }

    #[test]
    fn choices_persist_without_importing_and_empty_disables_future_reads() {
        let dir = selection_fixture();
        let snapshot = dir.path().join("events.jsonl");
        fs::write(&snapshot, "old event evidence\n").unwrap();
        let receipt = save_choices(dir.path(), vec!["life".into(), "work".into()]).unwrap();
        assert_eq!(receipt["selected_ids"], json!(["life", "work"]));
        assert_eq!(
            load_selection(
                &dir.path().join("apple_selection.json"),
                "calendar_ids",
                "calendar"
            )
            .unwrap(),
            ["life", "work"]
        );
        assert_eq!(
            fs::read_to_string(&snapshot).unwrap(),
            "old event evidence\n"
        );
        assert!(!dir.path().join("receipts").exists());
        save_choices(dir.path(), vec![]).unwrap();
        assert!(load_selection(
            &dir.path().join("apple_selection.json"),
            "calendar_ids",
            "calendar"
        )
        .unwrap()
        .is_empty());
        assert!(
            validate_selection(&[]).is_err(),
            "reading with no selection must never mean everything"
        );
    }

    #[test]
    fn invalid_or_unknown_choices_preserve_the_prior_selection() {
        let dir = selection_fixture();
        let path = dir.path().join("apple_selection.json");
        let before = fs::read(&path).unwrap();
        for ids in [
            vec!["unknown".into()],
            vec!["work".into(), "work".into()],
            vec!["".into()],
            vec![" ".into()],
            vec!["work\0".into()],
            vec!["x".repeat(1025)],
            (0..101).map(|id| format!("{id}")).collect(),
        ] {
            assert!(save_choices(dir.path(), ids).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn replaced_expired_or_previous_enrollment_catalogs_preserve_choices() {
        let dir = selection_fixture();
        let path = dir.path().join("apple_selection.json");
        let before = fs::read(&path).unwrap();
        let old_revision = load_catalog(dir.path(), "connected-1", Utc::now())
            .unwrap()
            .revision;
        cache_catalog(
            dir.path(),
            vec![json!({"id":"life","name":"Life","writable":true})],
            "connected-1".into(),
        )
        .unwrap();
        assert!(save_selection_locked(
            dir.path(),
            SelectRequest {
                calendar_ids: vec!["life".into()],
                catalog_revision: old_revision
            },
            "connected-1",
            Utc::now()
        )
        .is_err());
        let catalog = load_catalog(dir.path(), "connected-1", Utc::now()).unwrap();
        assert!(save_selection_locked(
            dir.path(),
            SelectRequest {
                calendar_ids: vec!["life".into()],
                catalog_revision: catalog.revision.clone()
            },
            "connected-2",
            Utc::now()
        )
        .is_err());
        assert!(save_selection_locked(
            dir.path(),
            SelectRequest {
                calendar_ids: vec![],
                catalog_revision: catalog.revision
            },
            "connected-1",
            Utc::now() + Duration::seconds(301)
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn malformed_catalog_or_selection_is_not_silently_overwritten() {
        let dir = selection_fixture();
        let path = dir.path().join("apple_selection.json");
        let before = fs::read(&path).unwrap();
        fs::write(catalog_path(dir.path()), "{}").unwrap();
        assert!(save_choices(dir.path(), vec!["work".into()]).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        cache_catalog(
            dir.path(),
            vec![json!({"id":"work","name":"Work","writable":true})],
            "connected-1".into(),
        )
        .unwrap();
        fs::write(&path, r#"{"schema_version":2,"calendar_ids":["work"]}"#).unwrap();
        let future = fs::read(&path).unwrap();
        assert!(save_choices(dir.path(), vec!["work".into()]).is_err());
        assert_eq!(fs::read(&path).unwrap(), future);
    }

    #[test]
    fn catalog_bounds_and_identity_validation_refuse_incomplete_projections() {
        assert!(validate_resources(&vec![
            json!({"id":"work","name":"Work","writable":true});
            MAX_CATALOG_RESOURCES + 1
        ])
        .is_err());
        assert!(validate_resources(&[
            json!({"id":"work","name":"Work","writable":true}),
            json!({"id":"work","name":"Another","writable":true})
        ])
        .is_err());
        assert!(validate_resources(&[json!({"id":"work","name":"Work"})]).is_err());
        let dir = selection_fixture();
        fs::write(
            catalog_path(dir.path()),
            vec![b' '; (MAX_CATALOG_BYTES + 1) as usize],
        )
        .unwrap();
        assert!(load_catalog(dir.path(), "connected-1", Utc::now()).is_err());
    }

    #[test]
    fn changed_selection_does_not_borrow_freshness_or_backoff_from_old_calendars() {
        let dir = selection_fixture();
        let now = Utc::now();
        let successful = Ok(json!({"calendar_ids":["work"],"complete":true,"fetched":3}));
        record_sync_outcome(dir.path(), now, &successful, &["work".into()]);
        assert!(sync_state_for_selection(dir.path(), &["work".into()])
            .last_read_at
            .is_some());
        assert!(sync_state_for_selection(dir.path(), &["life".into()])
            .last_read_at
            .is_none());
        record_sync_outcome(
            dir.path(),
            now,
            &Err(anyhow::anyhow!("Reader unavailable")),
            &["life".into()],
        );
        let saved = sync_state_for_selection(dir.path(), &["life".into()]);
        assert!(saved.last_read_at.is_none());
        assert_eq!(
            sync_decision(&saved, now, Duration::seconds(120), false),
            SyncDecision::BackOff
        );
    }

    #[test]
    fn selection_request_rejects_unexpected_fields_and_nonstring_ids() {
        for value in [
            json!({"calendar_ids":["work"],"catalog_revision":"a","read":true}),
            json!({"calendar_ids":[null],"catalog_revision":"a"}),
            json!({"calendar_ids":["work"]}),
        ] {
            assert!(serde_json::from_value::<SelectRequest>(value).is_err());
        }
    }

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn event_at(id: &str, calendar: &str, title: &str, start: &str, end: &str) -> Value {
        super::super::calendar::ensure_event_identity(
            json!({"source":"apple_calendar", "calendar_id":calendar,
            "external_id":id, "occurrence":"", "title":title, "start":start, "end":end}),
        )
    }

    fn event(id: &str, calendar: &str, title: &str) -> Value {
        event_at(
            id,
            calendar,
            title,
            "2026-09-12T12:00:00Z",
            "2026-09-12T13:00:00Z",
        )
    }

    /// Answers scans the way the helper does: overlapping events in start
    /// order, at most `limit`, flagged truncated when more remained.
    fn reader<'a>(
        source: &'a [Value],
        requests: &'a RefCell<Vec<(String, String)>>,
    ) -> impl Fn(&Value) -> Result<Value> + 'a {
        move |request| {
            let (from, to) = (
                instant(&request["start"]).unwrap(),
                instant(&request["end"]).unwrap(),
            );
            let limit = request["limit"].as_u64().unwrap() as usize;
            requests.borrow_mut().push((
                request["start"].as_str().unwrap().to_string(),
                request["end"].as_str().unwrap().to_string(),
            ));
            let mut overlapping: Vec<&Value> = source
                .iter()
                .filter(|row| {
                    instant(&row["start"]).unwrap() < to && instant(&row["end"]).unwrap() > from
                })
                .collect();
            overlapping.sort_by_key(|row| {
                (
                    instant(&row["start"]),
                    row["external_id"].as_str().map(str::to_string),
                )
            });
            Ok(
                json!({"schema_version":1, "calendar_ids":request["calendar_ids"],
                "start":request["start"], "end":request["end"],
                "truncated": overlapping.len() > limit,
                "events": overlapping.into_iter().take(limit).collect::<Vec<_>>()}),
            )
        }
    }

    #[test]
    fn complete_refresh_updates_and_deletes_only_the_selected_window() {
        let old = event("one", "selected", "Old title");
        let changed = event("one", "selected", "New title");
        assert_eq!(old["id"], changed["id"]);
        let other = event("private", "other", "Other source");
        let (next, fetched) = reconcile(
            vec![old, event("deleted", "selected", "Deleted"), other.clone()],
            std::slice::from_ref(&changed),
            &["selected".into()],
            at("2026-09-01T00:00:00Z"),
            at("2026-10-01T00:00:00Z"),
        );
        assert_eq!(next, vec![other, changed]);
        assert_eq!(fetched, 1);
    }

    #[test]
    fn partial_refresh_cannot_infer_deletion_or_import_another_calendar() {
        let old = event("one", "selected", "Existing");
        let start = at("2026-09-01T00:00:00Z");
        let (next, _) = reconcile(vec![old.clone()], &[], &["selected".into()], start, start);
        assert_eq!(next, vec![old]);
        assert!(
            validate_rows(&[event("two", "other", "Unselected")], &["selected".into()]).is_err()
        );
    }

    #[test]
    fn a_read_pages_past_the_reader_limit_and_proves_the_whole_window() {
        let source: Vec<Value> = (1..=5)
            .map(|hour| {
                event_at(
                    &format!("e{hour}"),
                    "work",
                    "Busy",
                    &format!("2026-09-12T0{hour}:00:00Z"),
                    &format!("2026-09-12T0{hour}:30:00Z"),
                )
            })
            .collect();
        let requests = RefCell::new(Vec::new());
        let (start, end) = (at("2026-09-12T00:00:00Z"), at("2026-09-13T00:00:00Z"));
        let scan =
            scan_window(&["work".into()], start, end, 2, reader(&source, &requests)).unwrap();

        assert_eq!(scan.covered_until, end);
        assert!(scan.pages > 1 && scan.pages <= MAX_SCAN_PAGES);
        // Each page resumes at the last start the previous page returned.
        assert_eq!(requests.borrow()[1].0, "2026-09-12T02:00:00Z");

        let stale = event_at(
            "gone",
            "work",
            "Deleted in Calendar",
            "2026-09-12T09:00:00Z",
            "2026-09-12T10:00:00Z",
        );
        let (next, fetched) = reconcile(
            vec![stale],
            &scan.rows,
            &["work".into()],
            start,
            scan.covered_until,
        );
        assert_eq!(fetched, 5, "boundary events that arrived twice count once");
        let mut ids: Vec<&str> = next
            .iter()
            .map(|row| row["external_id"].as_str().unwrap())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, ["e1", "e2", "e3", "e4", "e5"]);
    }

    #[test]
    fn a_read_that_cannot_advance_deletes_only_before_where_it_stopped() {
        // Three events start together but a page holds two: the read cannot
        // move past that instant, so nothing from it onward is proven absent.
        let crowd: Vec<Value> = ["a", "b", "c"]
            .iter()
            .map(|id| {
                event_at(
                    id,
                    "work",
                    "Crowd",
                    "2026-09-12T05:00:00Z",
                    "2026-09-12T06:00:00Z",
                )
            })
            .collect();
        let requests = RefCell::new(Vec::new());
        let (start, end) = (at("2026-09-12T00:00:00Z"), at("2026-09-13T00:00:00Z"));
        let scan = scan_window(&["work".into()], start, end, 2, reader(&crowd, &requests)).unwrap();
        assert_eq!(scan.covered_until, at("2026-09-12T05:00:00Z"));

        let deleted = event_at(
            "gone",
            "work",
            "Deleted",
            "2026-09-12T02:00:00Z",
            "2026-09-12T03:00:00Z",
        );
        let unseen = event_at(
            "later",
            "work",
            "Unseen",
            "2026-09-12T08:00:00Z",
            "2026-09-12T09:00:00Z",
        );
        let (next, _) = reconcile(
            vec![deleted, unseen.clone()],
            &scan.rows,
            &["work".into()],
            start,
            scan.covered_until,
        );
        assert!(next.contains(&unseen));
        assert!(!next.iter().any(|row| row["external_id"] == "gone"));
    }

    #[test]
    fn sync_reuses_a_recent_read_and_backs_off_a_failing_reader() {
        let now = at("2026-09-14T12:00:00Z");
        let two_minutes = Duration::seconds(120);
        let read = |seconds_ago: i64| SyncState {
            schema_version: 1,
            last_attempt_at: Some(now - Duration::seconds(seconds_ago)),
            last_read_at: Some(now - Duration::seconds(seconds_ago)),
            ..SyncState::default()
        };
        assert_eq!(
            sync_decision(&SyncState::default(), now, two_minutes, false),
            SyncDecision::Read
        );
        assert_eq!(
            sync_decision(&read(30), now, two_minutes, false),
            SyncDecision::Fresh
        );
        assert_eq!(
            sync_decision(&read(300), now, two_minutes, false),
            SyncDecision::Read
        );
        assert_eq!(
            sync_decision(&read(30), now, two_minutes, true),
            SyncDecision::Read
        );

        let failed = |seconds_ago: i64| SyncState {
            last_attempt_at: Some(now - Duration::seconds(seconds_ago)),
            error: Some("calendar_access_required".into()),
            ..read(600)
        };
        assert_eq!(
            sync_decision(&failed(10), now, two_minutes, false),
            SyncDecision::BackOff
        );
        assert_eq!(
            sync_decision(&failed(90), now, two_minutes, false),
            SyncDecision::Read
        );
        assert_eq!(
            sync_decision(&failed(10), now, two_minutes, true),
            SyncDecision::Read
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
