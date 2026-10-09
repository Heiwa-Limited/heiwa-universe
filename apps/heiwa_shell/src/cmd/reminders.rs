//! Explicitly selected Reminders reads and local Calendar-to-reminder proposals.
//! No reminder effect, approval, or external write is admitted by this module.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use super::apple_resources::{self, AppleResource};
use crate::output::{self, CliError};

const RESOURCE: AppleResource = AppleResource::Reminders;
const MAX_LISTS: usize = 100;
const MAX_ROWS: usize = 500;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MARKER_ROOT: &str = "heiwa://effect/rem-evt-";

#[derive(Debug, Deserialize, Serialize)]
struct List {
    id: String,
    name: String,
    source: String,
    writable: bool,
}

#[derive(Deserialize)]
struct Inventory {
    schema_version: u32,
    lists: Vec<List>,
    truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Reminder {
    id: String,
    list_id: String,
    title: String,
    due: Value,
    completed: bool,
    #[serde(default)]
    marker: Option<String>,
}

#[derive(Deserialize)]
struct Scan {
    schema_version: u32,
    list_ids: Vec<String>,
    reminders: Vec<Reminder>,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SourceEvent {
    calendar_id: String,
    external_id: String,
    occurrence: String,
}

#[derive(Serialize)]
struct Proposed {
    effect_id: String,
    marker: String,
    title: String,
    due: Value,
    source_event: SourceEvent,
}

fn state_dir() -> PathBuf {
    crate::home::heiwa_state_dir().join("reminders")
}

fn selection_path() -> PathBuf {
    state_dir().join("apple_selection.json")
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
}

fn validate_ids(ids: &[String]) -> Result<()> {
    if ids.is_empty()
        || ids.len() > MAX_LISTS
        || ids.iter().any(|id| !valid_id(id))
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        bail!(CliError::usage(
            "choose 1–100 distinct, nonempty Reminders list IDs"
        ));
    }
    Ok(())
}

fn selected_ids() -> Result<Vec<String>> {
    let path = selection_path();
    if path.exists() && fs::metadata(&path)?.len() > 256 * 1024 {
        bail!("Reminders selection exceeds the read bound");
    }
    let ids = apple_resources::load_selection(&path, "list_ids", "Reminders")?;
    validate_ids(&ids)?;
    Ok(ids)
}

fn selection_lock() -> Result<fs::File> {
    let state = state_dir();
    fs::create_dir_all(&state)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
    }
    super::calendar_read::snapshot_lock(&selection_path())
}

fn request(request: Value) -> Result<Value> {
    let result = apple_resources::helper_request(
        RESOURCE,
        &request,
        Duration::from_secs(45),
        MAX_RESPONSE_BYTES,
        "Apple Reminders helper is unavailable; build the matching Heiwa bundle",
        "invalid Apple Reminders helper response",
    )
    .map_err(|_| {
        CliError::failure("Apple Reminders could not be read")
            .with_hint("run `heiwa connect apple-reminders --authorize`; check System Settings > Privacy & Security > Reminders")
    })?;
    if result["schema_version"] != 1 {
        bail!("Unsupported Apple Reminders helper schema");
    }
    if result.get("error").is_some() {
        // Project a fixed remedy, never helper exceptions or personal content.
        bail!(CliError::failure("Apple Reminders read is unavailable")
            .with_hint("run `heiwa connect apple-reminders --authorize`; check System Settings > Privacy & Security > Reminders"));
    }
    Ok(result)
}

fn inventory(request_access: bool) -> Result<Inventory> {
    let response = request(json!({"operation":"reminders_list", "request_access":request_access}))?;
    let inventory: Inventory = serde_json::from_value(response)
        .map_err(|_| CliError::failure("Invalid Apple Reminders list response"))?;
    let mut seen = HashSet::new();
    if inventory.schema_version != 1
        || inventory.lists.len() > MAX_LISTS
        || inventory.lists.iter().any(|list| {
            !valid_id(&list.id)
                || !seen.insert(&list.id)
                || list.name.len() > 16 * 1024
                || list.source.len() > 16 * 1024
        })
    {
        bail!("Invalid or oversized Apple Reminders list response");
    }
    Ok(inventory)
}

fn require_lists(inventory: &Inventory, ids: &[String]) -> Result<()> {
    validate_ids(ids)?;
    if ids
        .iter()
        .any(|id| !inventory.lists.iter().any(|list| &list.id == id))
    {
        bail!("A selected Reminders list is unavailable; choose available lists again");
    }
    Ok(())
}

fn normalized_due(due: &Value) -> Result<Value> {
    if due.is_null() {
        return Ok(Value::Null);
    }
    let object = due.as_object().context("invalid reminder due date")?;
    if object.len() == 1 {
        if let Some(date) = object.get("date").and_then(Value::as_str) {
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
            if parsed.format("%Y-%m-%d").to_string() == date {
                return Ok(json!({"date":date}));
            }
        }
        if let Some(instant) = object.get("instant").and_then(Value::as_str) {
            return Ok(json!({"instant":DateTime::parse_from_rfc3339(instant)?
                .with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::AutoSi, true)}));
        }
    }
    bail!("invalid reminder due date");
}

fn canonical_marker(marker: &str) -> bool {
    marker.strip_prefix(MARKER_ROOT).is_some_and(|suffix| {
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn scan(ids: &[String]) -> Result<Scan> {
    validate_ids(ids)?;
    let response =
        request(json!({"operation":"reminders_scan", "list_ids":ids, "limit":MAX_ROWS}))?;
    let mut scan: Scan = serde_json::from_value(response)
        .map_err(|_| CliError::failure("Invalid Apple Reminders scan response"))?;
    if scan.schema_version != 1 || scan.list_ids != ids || scan.reminders.len() > MAX_ROWS {
        bail!("Apple Reminders scan did not cover the requested lists within its bound");
    }
    let mut seen = HashSet::new();
    for reminder in &mut scan.reminders {
        if !valid_id(&reminder.id)
            || !ids.contains(&reminder.list_id)
            || !seen.insert(reminder.id.clone())
            || reminder.title.len() > 16 * 1024
        {
            bail!("Apple Reminders scan contained an invalid or unselected reminder");
        }
        reminder.due = normalized_due(&reminder.due)
            .map_err(|_| CliError::failure("Apple Reminders scan contained an invalid due date"))?;
        reminder.marker = reminder
            .marker
            .take()
            .filter(|marker| canonical_marker(marker));
    }
    Ok(scan)
}

pub(crate) fn connect(args: &[String]) -> Result<()> {
    let json_output = output::wants_json(args);
    let flags: Vec<&str> = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .map(String::as_str)
        .collect();
    let data = match flags.as_slice() {
        [] => apple_resources::connection_payload(RESOURCE),
        ["--authorize"] => authorize()?,
        ["--disconnect"] => disconnect()?,
        _ => bail!(CliError::usage(
            "heiwa connect apple-reminders [--authorize|--disconnect] [--json]"
        )),
    };
    output::emit(json_output, data, &[], |data| {
        println!(
            "Apple Reminders: {}",
            data["status"].as_str().unwrap_or("unavailable")
        );
        if let Some(next) = data["next_action"].as_str() {
            println!("  next: {next}");
        }
        if let Some(count) = data["resource_count"].as_u64() {
            println!("  visible lists: {count}; choose lists with `heiwa reminders select`");
        }
    })
}

/// Enroll Reminders for this installation and device, asking macOS for
/// Reminders access through the helper. Shared by the CLI and the app API;
/// from the app, macOS attributes the request to Heiwa itself.
pub(crate) fn authorize() -> Result<Value> {
    let _lock = selection_lock()?;
    let count =
        apple_resources::connect(RESOURCE, &["reminders.read", "reminders.propose"], || {
            inventory(true).map(|inventory| inventory.lists.len())
        })?;
    Ok(
        json!({"connector":"apple_reminders","status":"connected","resource_count":count,
        "auth":{"mode":"eventkit","owner":"macOS","secrets":"none"}}),
    )
}

pub(crate) fn disconnect() -> Result<Value> {
    let _lock = selection_lock()?;
    apple_resources::remove_enrollment(RESOURCE)?;
    Ok(
        json!({"connector":"apple_reminders","status":"disconnected","local_selection_preserved":true,
        "revoke":"System Settings > Privacy & Security > Reminders"}),
    )
}

/// Enrollment and saved selection only; never calls the helper.
pub(crate) fn status() -> Value {
    let mut payload = apple_resources::connection_payload(RESOURCE);
    payload["selected_list_ids"] = json!(selected_ids().unwrap_or_default());
    payload
}

pub(crate) fn lists() -> Result<Value> {
    apple_resources::require_connection(RESOURCE)?;
    let inventory = inventory(false)?;
    apple_resources::require_connection(RESOURCE)?;
    Ok(
        json!({"lists":inventory.lists,"truncated":inventory.truncated,"complete":!inventory.truncated}),
    )
}

pub fn run(args: &[String]) -> Result<()> {
    let json_output = output::wants_json(args);
    let args: Vec<String> = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect();
    let data = match args.first().map(String::as_str) {
        Some("lists") if args.len() == 1 => lists()?,
        Some("select") => select(&args[1..])?,
        Some("read") if args.len() == 1 => read()?,
        Some("propose") => propose(&args[1..])?,
        None | Some("--help") | Some("-h") if args.len() <= 1 => json!({
            "usage":"heiwa reminders lists|select <list_id>...|read|propose --list <id> --event <calendar_id> <external_id> [--occurrence <iso>] [--json]",
            "effects":"read and proposal only; no reminder writes"}),
        _ => bail!(CliError::usage(
            "unknown or malformed reminders command; run `heiwa reminders --help`"
        )),
    };
    output::emit(json_output, data, &[], print_result)
}

fn print_result(data: &Value) {
    if let Some(usage) = data["usage"].as_str() {
        println!("{usage}\nRead and proposal only; no reminder writes.");
    } else if let Some(lists) = data["lists"].as_array() {
        for list in lists {
            println!(
                "{}  {}{}",
                list["id"].as_str().unwrap_or("?"),
                list["name"].as_str().unwrap_or("?"),
                if list["writable"] == true {
                    ""
                } else {
                    " (read-only)"
                }
            );
        }
    } else if let Some(outcome) = data["outcome"].as_str() {
        println!(
            "{outcome}: {}",
            data["proposed"]["title"].as_str().unwrap_or("?")
        );
        println!("  due: {}", data["proposed"]["due"]);
        println!("  proposal only; no reminder written");
    } else if let Some(rows) = data["reminders"].as_array() {
        for row in rows {
            println!(
                "{} {}  due: {}",
                if row["completed"] == true {
                    "[done]"
                } else {
                    "[ ]"
                },
                row["title"].as_str().unwrap_or("?"),
                row["due"]
            );
        }
    } else if let Some(ids) = data["selected_list_ids"].as_array() {
        println!(
            "Selected {} Reminders lists. Next: heiwa reminders read",
            ids.len()
        );
    }
    if data["complete"] == false || data["scan_complete"] == false {
        println!("  evidence incomplete; absence or synchronization is not established");
    }
}

pub(crate) fn select(ids: &[String]) -> Result<Value> {
    apple_resources::require_connection(RESOURCE)?;
    validate_ids(ids)?;
    let _lock = selection_lock()?;
    let inventory = inventory(false)?;
    require_lists(&inventory, ids)?;
    apple_resources::require_connection(RESOURCE)?;
    apple_resources::store_selection(&selection_path(), "list_ids", ids)?;
    Ok(json!({"selected_list_ids":ids,"inventory_complete":!inventory.truncated}))
}

pub(crate) fn read() -> Result<Value> {
    apple_resources::require_connection(RESOURCE)?;
    let _lock = selection_lock()?;
    let ids = selected_ids()?;
    let inventory = inventory(false)?;
    require_lists(&inventory, &ids)?;
    let scan = scan(&ids)?;
    apple_resources::require_connection(RESOURCE)?;
    Ok(json!({"reminders":scan.reminders,"selected_list_ids":ids,
        "truncated":scan.truncated,"complete":!scan.truncated && !inventory.truncated}))
}

fn proposal_args(args: &[String]) -> Result<(String, SourceEvent)> {
    let mut list = None;
    let mut event = None;
    let mut occurrence = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--list" if list.is_none() && index + 1 < args.len() => {
                list = Some(args[index + 1].clone());
                index += 2;
            }
            "--event" if event.is_none() && index + 2 < args.len() => {
                event = Some((args[index + 1].clone(), args[index + 2].clone()));
                index += 3;
            }
            "--occurrence" if occurrence.is_none() && index + 1 < args.len() => {
                occurrence = Some(args[index + 1].clone());
                index += 2;
            }
            _ => bail!(CliError::usage("propose requires --list <id> --event <calendar_id> <external_id> [--occurrence <iso>]")),
        }
    }
    let list = list
        .filter(|id| valid_id(id) && !id.starts_with("--"))
        .context(CliError::usage("--list <selected_id> is required"))?;
    let (calendar_id, external_id) = event
        .filter(|(calendar, event)| {
            valid_id(calendar)
                && valid_id(event)
                && !calendar.starts_with("--")
                && !event.starts_with("--")
        })
        .context(CliError::usage(
            "--event <calendar_id> <external_id> is required",
        ))?;
    let occurrence = occurrence.unwrap_or_default();
    if !occurrence.is_empty()
        && (occurrence.len() > 128 || DateTime::parse_from_rfc3339(&occurrence).is_err())
    {
        bail!(CliError::usage("--occurrence must be an RFC3339 instant"));
    }
    Ok((
        list,
        SourceEvent {
            calendar_id,
            external_id,
            occurrence,
        },
    ))
}

fn imported_proposal(source: SourceEvent) -> Result<Proposed> {
    apple_resources::require_connection(AppleResource::Calendar)?;
    let state = super::calendar::calendar_state_dir();
    let _lock = super::calendar_read::snapshot_lock(&state.join("events.jsonl"))?;
    if !super::calendar_read::selected_ids()?.contains(&source.calendar_id) {
        bail!("The source event must belong to a currently selected Calendar");
    }
    let rows = super::calendar_read::read_snapshot(&state.join("events.jsonl"))?;
    let matches: Vec<&Value> = rows
        .iter()
        .filter(|row| {
            row["source"] == "apple_calendar"
                && row["calendar_id"] == source.calendar_id
                && row["external_id"] == source.external_id
                && row["occurrence"] == source.occurrence
        })
        .collect();
    if matches.len() != 1 {
        bail!(CliError::not_found(
            "The selected imported event is missing or ambiguous; import Calendar events again"
        ));
    }
    let event = matches[0];
    if event["status"] != "confirmed" {
        bail!("A cancelled or unconfirmed event cannot propose a reminder");
    }
    let title = event["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty() && title.len() <= 16 * 1024)
        .context("The imported event title is invalid")?
        .to_string();
    let due = match event["all_day"].as_bool() {
        Some(true) => normalized_due(&json!({"date":event["date"]}))?,
        Some(false) => normalized_due(&json!({"instant":event["start"]}))?,
        None => bail!("The imported event has no all-day classification"),
    };
    // An unambiguous JSON tuple. Identity is device/store-local;
    // occurrence is the imported EventKit identity, not a cross-device ID.
    let identity = serde_json::to_vec(&(
        source.calendar_id.as_str(),
        source.external_id.as_str(),
        source.occurrence.as_str(),
    ))?;
    let effect_id = format!("rem-evt-{:x}", Sha256::digest(identity));
    apple_resources::require_connection(AppleResource::Calendar)?;
    Ok(Proposed {
        marker: format!("heiwa://effect/{effect_id}"),
        effect_id,
        title,
        due,
        source_event: source,
    })
}

fn propose(args: &[String]) -> Result<Value> {
    let (target_id, source) = proposal_args(args)?;
    apple_resources::require_connection(RESOURCE)?;
    let proposed = imported_proposal(source)?;
    let _lock = selection_lock()?;
    let ids = selected_ids()?;
    if !ids.contains(&target_id) {
        bail!("The target Reminders list must be explicitly selected");
    }
    let inventory = inventory(false)?;
    require_lists(&inventory, &ids)?;
    let target = inventory
        .lists
        .iter()
        .find(|list| list.id == target_id)
        .context("target list unavailable")?;
    let scan_result = scan(&ids);
    apple_resources::require_connection(RESOURCE)?;
    apple_resources::require_connection(AppleResource::Calendar)?;
    if !super::calendar_read::selected_ids()?.contains(&proposed.source_event.calendar_id) {
        bail!("The source Calendar selection changed while preparing the proposal; retry");
    }
    let (outcome, complete, matches, reason) = match scan_result {
        Err(_) => ("undetermined", false, vec![], Some("scan_unavailable")),
        Ok(scan) => {
            let complete = !scan.truncated && !inventory.truncated;
            let matches: Vec<Reminder> = scan
                .reminders
                .into_iter()
                .filter(|reminder| reminder.marker.as_deref() == Some(&proposed.marker))
                .collect();
            let outcome = if matches.len() > 1 {
                "ambiguous"
            } else if !complete {
                "undetermined"
            } else if !target.writable {
                "list_read_only"
            } else if let Some(existing) = matches.first() {
                if existing.list_id != target_id {
                    "exists_in_other_list"
                } else if existing.title == proposed.title
                    && existing.due == proposed.due
                    && !existing.completed
                {
                    "in_sync"
                } else {
                    "conflict"
                }
            } else {
                "create"
            };
            (
                outcome,
                complete,
                matches,
                (!complete).then_some("incomplete_scan"),
            )
        }
    };
    Ok(
        json!({"outcome":outcome,"proposed":proposed,"matches":matches,"scan_complete":complete,"reason":reason,
        "scope":{"selected_list_ids":ids,"target_list_id":target_id,"identity":"device/store-local imported event and occurrence"},
        "effects":"proposal only; no reminder written"}),
    )
}
