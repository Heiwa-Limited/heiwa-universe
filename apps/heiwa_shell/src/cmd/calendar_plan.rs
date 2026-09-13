//! Calendar plans: a desired-state batch of Apple Calendar events.
//!
//! A plan owns exactly the events it wrote. Each carries the URL marker
//! `heiwa://calendar/plan/<plan_id>/<key>` (AD-29's hold marker, applied to a
//! whole plan), so a change is data rather than a re-import:
//!
//!   heiwa calendar plan diff  <plan.json>   what would change, nothing written
//!   heiwa calendar plan stage <plan.json>   one T2 approval listing every change
//!   heiwa approvals decide <id> --approve   one EventKit commit, one receipt
//!
//! Only the delta is written. Unmarked events are never touched unless `--adopt`
//! matches them exactly (migrating an earlier import). Approval re-scans first
//! and refuses if the calendar drifted since staging, so the reviewed change set
//! is the applied change set.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub(crate) const MARKER_ROOT: &str = "heiwa://calendar/plan/";
const MAX_EVENTS: usize = 2000;
const MAX_WINDOW_DAYS: i64 = 400;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub schema_version: u32,
    pub plan_id: String,
    pub window: Window,
    pub calendars: Vec<String>,
    pub events: Vec<PlanEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Window {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanEvent {
    pub key: String,
    pub calendar: String,
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub tentative: bool,
}

/// An event the helper reported: marked (owned by this plan) or unmarked.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct Existing {
    #[serde(default)]
    pub marker: String,
    pub external_id: String,
    pub calendar: String,
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub tentative: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct Change {
    pub op: String,
    pub marker: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<PlanEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Existing>,
}

pub(crate) fn marker_prefix(plan_id: &str) -> String {
    format!("{MARKER_ROOT}{plan_id}/")
}

pub(crate) fn marker(plan_id: &str, key: &str) -> String {
    format!("{MARKER_ROOT}{plan_id}/{key}")
}

fn instant(value: &str, field: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| anyhow!("{field} must be an RFC 3339 timestamp with an offset: {value:?}"))
}

fn valid_token(value: &str, max: usize, extra: &[char]) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

pub(crate) fn validate(plan: &Plan) -> Result<()> {
    if plan.schema_version != 1 {
        bail!(
            "unsupported calendar plan schema_version {}",
            plan.schema_version
        );
    }
    if !valid_token(&plan.plan_id, 64, &['-']) || plan.plan_id.starts_with('-') {
        bail!("plan_id must be 1-64 characters of a-z, 0-9 and '-'");
    }
    if plan.plan_id.chars().any(|c| c.is_ascii_uppercase()) {
        bail!("plan_id must be lowercase");
    }
    let start = instant(&plan.window.start, "window.start")?;
    let end = instant(&plan.window.end, "window.end")?;
    if end <= start || (end - start).num_days() > MAX_WINDOW_DAYS {
        bail!("window must end after it starts and span at most {MAX_WINDOW_DAYS} days");
    }
    if plan.calendars.is_empty() || plan.calendars.len() > 50 {
        bail!("a plan names between 1 and 50 calendars");
    }
    let calendars: HashSet<&str> = plan.calendars.iter().map(String::as_str).collect();
    if calendars.len() != plan.calendars.len()
        || plan
            .calendars
            .iter()
            .any(|name| name.trim().is_empty() || name.len() > 256)
    {
        bail!("plan calendars must be distinct, non-empty names");
    }
    if plan.events.len() > MAX_EVENTS {
        bail!("a plan holds at most {MAX_EVENTS} events");
    }
    let mut keys = HashSet::new();
    for event in &plan.events {
        if !valid_token(&event.key, 128, &['-', '_', '.', ':']) {
            bail!(
                "event key {:?} must be 1-128 of A-Z a-z 0-9 - _ . :",
                event.key
            );
        }
        if !keys.insert(event.key.as_str()) {
            bail!("duplicate event key {:?}", event.key);
        }
        if !calendars.contains(event.calendar.as_str()) {
            bail!(
                "event {:?} targets calendar {:?}, which the plan does not name",
                event.key,
                event.calendar
            );
        }
        if event.title.trim().is_empty() || event.title.len() > 1024 {
            bail!("event {:?} needs a title of at most 1024 bytes", event.key);
        }
        if event.notes.len() > 8000 || event.location.len() > 512 {
            bail!("event {:?} notes or location is too long", event.key);
        }
        let event_start = instant(&event.start, "event start")?;
        let event_end = instant(&event.end, "event end")?;
        if event_end <= event_start {
            bail!("event {:?} must end after it starts", event.key);
        }
        if event_start < start || event_start >= end {
            bail!("event {:?} starts outside the plan window", event.key);
        }
    }
    Ok(())
}

fn same_content(want: &PlanEvent, have: &Existing) -> Result<bool> {
    Ok(want.calendar == have.calendar
        && want.title == have.title
        && instant(&want.start, "event start")? == instant(&have.start, "existing start")?
        && instant(&want.end, "event end")? == instant(&have.end, "existing end")?
        && want.notes.trim() == have.notes.trim()
        && want.location.trim() == have.location.trim()
        && want.tentative == have.tentative)
}

/// The change set that turns `marked` (+ adoptable `unmarked`) into the plan.
/// Deletes come first, then updates and adoptions, then creates; each group is
/// ordered by marker so the same inputs always produce the same approval.
pub(crate) fn diff(
    plan: &Plan,
    marked: &[Existing],
    unmarked: &[Existing],
    adopt: bool,
) -> Result<Vec<Change>> {
    validate(plan)?;
    let prefix = marker_prefix(&plan.plan_id);
    let window_start = instant(&plan.window.start, "window.start")?;
    let window_end = instant(&plan.window.end, "window.end")?;
    let desired: BTreeMap<String, &PlanEvent> = plan
        .events
        .iter()
        .map(|event| (marker(&plan.plan_id, &event.key), event))
        .collect();

    let (mut deletes, mut updates, mut creates) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: HashSet<String> = HashSet::new();
    let mut owned: Vec<&Existing> = marked
        .iter()
        .filter(|existing| existing.marker.starts_with(&prefix))
        .collect();
    owned.sort_by(|a, b| (&a.marker, &a.external_id).cmp(&(&b.marker, &b.external_id)));
    for existing in owned {
        let starts = instant(&existing.start, "existing start")?;
        if starts < window_start || starts >= window_end {
            continue; // outside this revision's window: leave it alone
        }
        match desired.get(&existing.marker) {
            Some(want) if seen.insert(existing.marker.clone()) => {
                if !same_content(want, existing)? {
                    updates.push(Change {
                        op: "update".into(),
                        marker: existing.marker.clone(),
                        external_id: Some(existing.external_id.clone()),
                        event: Some((*want).clone()),
                        before: Some(existing.clone()),
                    });
                }
            }
            // Not in the plan any more, or a duplicate of a marker already kept.
            _ => deletes.push(Change {
                op: "delete".into(),
                marker: existing.marker.clone(),
                external_id: Some(existing.external_id.clone()),
                event: None,
                before: Some(existing.clone()),
            }),
        }
    }

    let mut claimed: HashSet<String> = HashSet::new();
    for (marker, want) in &desired {
        if seen.contains(marker) {
            continue;
        }
        let adoptable = if adopt {
            unmarked.iter().find(|candidate| {
                candidate.marker.is_empty()
                    && !claimed.contains(&candidate.external_id)
                    && candidate.calendar == want.calendar
                    && candidate.title == want.title
                    && instant(&candidate.start, "").ok() == instant(&want.start, "").ok()
                    && instant(&candidate.end, "").ok() == instant(&want.end, "").ok()
            })
        } else {
            None
        };
        match adoptable {
            Some(candidate) => {
                claimed.insert(candidate.external_id.clone());
                updates.push(Change {
                    op: "adopt".into(),
                    marker: marker.clone(),
                    external_id: Some(candidate.external_id.clone()),
                    event: Some((*want).clone()),
                    before: Some(candidate.clone()),
                });
            }
            None => creates.push(Change {
                op: "create".into(),
                marker: marker.clone(),
                external_id: None,
                event: Some((*want).clone()),
                before: None,
            }),
        }
    }
    deletes.extend(updates);
    deletes.extend(creates);
    Ok(deletes)
}

pub(crate) fn counts(changes: &[Change], plan: &Plan) -> Value {
    let count = |op: &str| changes.iter().filter(|change| change.op == op).count();
    let touched = count("update") + count("adopt") + count("create");
    json!({
        "create": count("create"),
        "update": count("update"),
        "adopt": count("adopt"),
        "delete": count("delete"),
        "unchanged": plan.events.len().saturating_sub(touched),
    })
}

fn summary_line(counts: &Value) -> String {
    format!(
        "create {} · update {} · delete {} · adopt {} · unchanged {}",
        counts["create"], counts["update"], counts["delete"], counts["adopt"], counts["unchanged"]
    )
}

/// Readable review rows for the approval: an approval nobody can read is not
/// an approval (L3 spec).
fn review_rows(changes: &[Change]) -> Vec<Value> {
    changes
        .iter()
        .map(|change| {
            let shown = change
                .event
                .as_ref()
                .map(|event| json!({"calendar": event.calendar, "title": event.title, "start": event.start, "end": event.end, "tentative": event.tentative}))
                .or_else(|| change.before.as_ref().map(|before| json!({"calendar": before.calendar, "title": before.title, "start": before.start, "end": before.end})))
                .unwrap_or(Value::Null);
            json!({"op": change.op, "marker": change.marker, "event": shown})
        })
        .collect()
}

fn plan_sha256(plan: &Plan) -> Result<String> {
    let bytes = serde_json::to_vec(plan)?;
    let digest = Sha256::digest(&bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn plans_dir(plan_id: &str) -> PathBuf {
    super::calendar::calendar_state_dir()
        .join("plans")
        .join(plan_id)
}

fn load_plan(path: &str) -> Result<Plan> {
    let raw = fs::read(path).with_context(|| format!("read calendar plan {path}"))?;
    let plan: Plan =
        serde_json::from_slice(&raw).with_context(|| format!("parse calendar plan {path}"))?;
    validate(&plan)?;
    Ok(plan)
}

// ------------------------------------------------------------------ helper IO

fn call_helper(request: &Value, timeout_secs: u64) -> Result<Value> {
    let helper = super::calendar_read::helper_path().ok_or_else(|| {
        anyhow!("The Apple resource helper is missing. Install the complete Heiwa app.")
    })?;
    let mut command = std::process::Command::new(helper);
    command.env_remove("XPC_SERVICE_NAME");
    command.arg(request.to_string());
    let bytes = heiwa_core::subprocess::bounded_output(
        &mut command,
        std::time::Duration::from_secs(timeout_secs),
        16 * 1024 * 1024,
    )
    .map_err(|error| anyhow!("Apple Calendar plan helper failed: {error}"))?;
    let value: Value =
        serde_json::from_slice(&bytes).context("read Apple Calendar plan response")?;
    if value["schema_version"] != 1 {
        bail!("Apple Calendar plan helper returned an unsupported response");
    }
    if let Some(code) = value.get("error").and_then(Value::as_str) {
        let hint = match code {
            "calendar_access_required" => {
                " Run `heiwa connect apple-calendar --authorize` and allow full Calendar access."
            }
            "selected_calendar_unavailable" => {
                " Each plan calendar must exist exactly once by name."
            }
            "selected_calendar_read_only" => " A plan calendar is read-only.",
            "plan_event_changed" => " The calendar changed since staging; stage the plan again.",
            _ => "",
        };
        bail!("Apple Calendar plan helper refused: {code}.{hint}");
    }
    Ok(value)
}

fn scan(plan: &Plan, adopt: bool) -> Result<(Vec<Existing>, Vec<Existing>)> {
    let response = call_helper(
        &json!({
            "operation": "plan_scan",
            "calendars": plan.calendars,
            "marker_prefix": marker_prefix(&plan.plan_id),
            "start": plan.window.start,
            "end": plan.window.end,
            "adopt": adopt,
        }),
        60,
    )?;
    let parse = |field: &str| -> Result<Vec<Existing>> {
        serde_json::from_value(response.get(field).cloned().unwrap_or_else(|| json!([])))
            .with_context(|| format!("invalid {field} events from the plan helper"))
    };
    Ok((parse("marked")?, parse("unmarked")?))
}

fn helper_changes(changes: &[Change]) -> Vec<Value> {
    changes
        .iter()
        .map(|change| {
            let mut row = json!({"op": change.op, "marker": change.marker});
            if let Some(id) = &change.external_id {
                row["external_id"] = json!(id);
            }
            if let Some(event) = &change.event {
                row["calendar"] = json!(event.calendar);
                row["title"] = json!(event.title);
                row["start"] = json!(event.start);
                row["end"] = json!(event.end);
                row["notes"] = json!(event.notes);
                row["location"] = json!(event.location);
                row["tentative"] = json!(event.tentative);
            }
            row
        })
        .collect()
}

// ------------------------------------------------------------------ CLI

pub(crate) fn run(args: &[String]) -> Result<()> {
    let usage = "usage: heiwa calendar plan diff|stage <plan.json> [--adopt] [--json]";
    let (Some(sub), Some(path)) = (args.first().map(String::as_str), args.get(1)) else {
        bail!("{usage}");
    };
    let adopt = args.iter().any(|arg| arg == "--adopt");
    let as_json = args.iter().any(|arg| arg == "--json");
    match sub {
        "diff" => {
            super::connectors::require_apple_calendar_connection()?;
            let plan = load_plan(path)?;
            let (marked, unmarked) = scan(&plan, adopt)?;
            let changes = diff(&plan, &marked, &unmarked, adopt)?;
            let counts = counts(&changes, &plan);
            if as_json {
                println!(
                    "{}",
                    json!({"command": "calendar plan diff", "plan_id": plan.plan_id, "counts": counts, "changes": review_rows(&changes)})
                );
            } else {
                println!("calendar plan diff: {}", plan.plan_id);
                println!("  {}", summary_line(&counts));
                print_rows(&changes);
            }
            Ok(())
        }
        "stage" => {
            let staged = stage(path, adopt)?;
            if as_json {
                println!("{staged}");
            } else if staged["in_sync"] == true {
                println!(
                    "calendar plan {}: already in sync, nothing to approve",
                    staged["plan_id"].as_str().unwrap_or("?")
                );
            } else {
                let id = staged["approval_request"]["request_id"]
                    .as_str()
                    .unwrap_or("?");
                println!("calendar plan staged: {id} (T2)");
                println!(
                    "  {}",
                    summary_line(&staged["approval_request"]["intent"]["counts"])
                );
                println!("  review: heiwa approvals show {id}");
                println!("  apply:  heiwa approvals decide {id} --approve");
            }
            Ok(())
        }
        _ => bail!("{usage}"),
    }
}

fn print_rows(changes: &[Change]) {
    for change in changes.iter().take(40) {
        let event = change.event.as_ref();
        let before = change.before.as_ref();
        let calendar = event
            .map(|e| e.calendar.as_str())
            .or(before.map(|b| b.calendar.as_str()))
            .unwrap_or("?");
        let title = event
            .map(|e| e.title.as_str())
            .or(before.map(|b| b.title.as_str()))
            .unwrap_or("?");
        let start = event
            .map(|e| e.start.as_str())
            .or(before.map(|b| b.start.as_str()))
            .unwrap_or("?");
        println!("  - {:<6} {calendar} · {start} · {title}", change.op);
    }
    if changes.len() > 40 {
        println!("  … {} more (use --json)", changes.len() - 40);
    }
}

pub(crate) fn stage(path: &str, adopt: bool) -> Result<Value> {
    super::connectors::require_apple_calendar_connection()?;
    let plan = load_plan(path)?;
    let (marked, unmarked) = scan(&plan, adopt)?;
    let changes = diff(&plan, &marked, &unmarked, adopt)?;
    let counts = counts(&changes, &plan);
    if changes.is_empty() {
        return Ok(json!({"plan_id": plan.plan_id, "in_sync": true, "counts": counts}));
    }
    let sha = plan_sha256(&plan)?;
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let dir = plans_dir(&plan.plan_id);
    fs::create_dir_all(&dir)?;
    let plan_path = dir.join(format!("{sha}.plan.json"));
    let changes_path = dir.join(format!("{request_id}.changes.json"));
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan)?)?;
    fs::write(
        &changes_path,
        serde_json::to_vec_pretty(&json!({"adopt": adopt, "changes": changes}))?,
    )?;
    let approval = json!({
        "schema_version": "operator_dispatch_request_v1",
        "request_id": request_id,
        "work_id": format!("work_calendar_plan_{}", plan.plan_id.replace('-', "_")),
        "created_at": Utc::now().to_rfc3339(),
        "action": "calendar-plan-apply",
        "target_surface": "calendar_plan",
        "target_scope": plan.plan_id,
        "requested_mode": "stage",
        "risk_tier": "T2",
        "intent": {
            "plan_id": plan.plan_id,
            "plan_sha256": sha,
            "plan_path": plan_path.display().to_string(),
            "changes_path": changes_path.display().to_string(),
            "adopt": adopt,
            "calendars": plan.calendars,
            "window": plan.window,
            "counts": counts,
            "changes": review_rows(&changes),
        },
        "source": "heiwa calendar plan stage",
    });
    let approval_path = super::approvals::requests_dir().join(format!("{request_id}.json"));
    if let Some(parent) = approval_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Err(error) = fs::write(&approval_path, serde_json::to_string_pretty(&approval)?) {
        let _ = fs::remove_file(&changes_path);
        bail!("failed to stage calendar plan approval: {error}");
    }
    Ok(
        json!({"plan_id": plan.plan_id, "in_sync": false, "approval_request": approval, "approval_path": approval_path.display().to_string()}),
    )
}

/// One-line change description for `approvals decide` output.
pub(crate) fn effect_summary(intent: &Value) -> String {
    format!(
        "apply calendar plan {}: {}",
        intent.get("plan_id").and_then(Value::as_str).unwrap_or("?"),
        summary_line(&intent["counts"])
    )
}

/// Execute an approved plan. Replaying the same approval returns its receipt
/// without writing again; a calendar that drifted since staging is refused.
pub(crate) fn apply_approved(approval_id: &str, work_id: &str, intent: &Value) -> Result<Value> {
    super::connectors::require_apple_calendar_connection()?;
    let plan_id = intent
        .get("plan_id")
        .and_then(Value::as_str)
        .context("approved plan has no plan_id")?;
    let sha = intent
        .get("plan_sha256")
        .and_then(Value::as_str)
        .context("approved plan has no plan_sha256")?;
    let receipt_id = format!("rcpt-calendar-plan-{approval_id}");
    let receipts = super::calendar::calendar_state_dir().join("receipts");
    let receipt_path = receipts.join(format!("{receipt_id}.json"));
    if receipt_path.exists() {
        return serde_json::from_slice(&fs::read(&receipt_path)?)
            .context("read existing plan receipt");
    }

    let plan_path = plans_dir(plan_id).join(format!("{sha}.plan.json"));
    let plan: Plan = serde_json::from_slice(
        &fs::read(&plan_path)
            .with_context(|| format!("read staged plan {}", plan_path.display()))?,
    )?;
    if plan_sha256(&plan)? != sha || plan.plan_id != plan_id {
        bail!("staged calendar plan does not match its approval; refusing to write");
    }
    let changes_path = plans_dir(plan_id).join(format!("{approval_id}.changes.json"));
    let staged: Value = serde_json::from_slice(
        &fs::read(&changes_path)
            .with_context(|| format!("read staged changes {}", changes_path.display()))?,
    )?;
    let adopt = staged["adopt"].as_bool().unwrap_or(false);
    let staged_changes: Vec<Change> = serde_json::from_value(staged["changes"].clone())?;

    let (marked, unmarked) = scan(&plan, adopt)?;
    let current = diff(&plan, &marked, &unmarked, adopt)?;
    if current != staged_changes {
        bail!(
            "Apple Calendar changed since this plan was staged ({} staged, {} now); stage it again",
            staged_changes.len(),
            current.len()
        );
    }

    let response = call_helper(
        &json!({"operation": "plan_apply", "changes": helper_changes(&staged_changes)}),
        180,
    )?;
    let applied = response["applied"]
        .as_array()
        .cloned()
        .context("plan helper returned no applied list")?;
    if applied.len() != staged_changes.len() {
        bail!(
            "plan helper applied {} of {} changes",
            applied.len(),
            staged_changes.len()
        );
    }
    let mut results = Vec::new();
    for change in &staged_changes {
        let row = applied
            .iter()
            .find(|row| row["marker"] == change.marker.as_str() && row["op"] == change.op.as_str())
            .with_context(|| {
                format!("plan helper did not report {} {}", change.op, change.marker)
            })?;
        let external_id = row["external_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context("plan helper returned an empty event id")?;
        let shown = change.event.as_ref().map(
            |e| json!({"calendar": e.calendar, "title": e.title, "start": e.start, "end": e.end}),
        );
        results.push(json!({"op": change.op, "marker": change.marker, "external_id": external_id, "event": shown}));
    }

    let origin_device_id = match heiwa_install::load_machine_manifest() {
        Ok(Some(manifest)) => Some(manifest.device_id),
        Ok(None) => None,
        Err(error) => bail!("cannot bind connector receipt to this machine: {error}"),
    };
    let receipt = json!({
        "schema_version": "heiwa_connector_receipt_v1",
        "receipt_id": receipt_id,
        "kind": "calendar_plan_applied",
        "work_id": work_id,
        "approval_id": approval_id,
        "connector": "apple_calendar",
        "action": "plan.apply",
        "origin_device_id": origin_device_id,
        "target": {"plan_id": plan_id, "plan_sha256": sha, "calendars": plan.calendars, "window": plan.window},
        "counts": counts(&staged_changes, &plan),
        "changes": results,
        "created_at": Utc::now().to_rfc3339(),
        "undo": {"supported": true, "posture": "stage the previous plan revision; its diff reverses these changes"},
    });
    fs::create_dir_all(&receipts)?;
    fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)?;
    use heiwa_evidence::EvidenceTransport;
    heiwa_evidence::JsonlTransport::default_local()?
        .journal("connector_receipts", receipt.clone())?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: &str, title: &str, start: &str, end: &str) -> PlanEvent {
        PlanEvent {
            key: key.into(),
            calendar: "Life".into(),
            title: title.into(),
            start: start.into(),
            end: end.into(),
            notes: String::new(),
            location: String::new(),
            tentative: false,
        }
    }

    fn plan(events: Vec<PlanEvent>) -> Plan {
        Plan {
            schema_version: 1,
            plan_id: "devon-2026-09".into(),
            window: Window {
                start: "2026-09-01T00:00:00-07:00".into(),
                end: "2026-10-01T00:00:00-07:00".into(),
            },
            calendars: vec!["Life".into()],
            events,
        }
    }

    fn existing(key: &str, title: &str, start: &str, end: &str, id: &str) -> Existing {
        Existing {
            marker: marker("devon-2026-09", key),
            external_id: id.into(),
            calendar: "Life".into(),
            title: title.into(),
            start: start.into(),
            end: end.into(),
            notes: String::new(),
            location: String::new(),
            tentative: false,
        }
    }

    #[test]
    fn diff_writes_only_the_delta_and_compares_instants_not_strings() {
        let p = plan(vec![
            event(
                "a",
                "Lunch",
                "2026-09-14T11:15:00-07:00",
                "2026-09-14T12:00:00-07:00",
            ),
            event(
                "b",
                "Dinner",
                "2026-09-14T18:00:00-07:00",
                "2026-09-14T19:00:00-07:00",
            ),
            event(
                "c",
                "Open",
                "2026-09-15T13:00:00-07:00",
                "2026-09-15T15:00:00-07:00",
            ),
        ]);
        let marked = vec![
            // same instant, UTC spelling: unchanged
            existing(
                "a",
                "Lunch",
                "2026-09-14T18:15:00Z",
                "2026-09-14T19:00:00Z",
                "ek-a",
            ),
            existing(
                "b",
                "Supper",
                "2026-09-15T01:00:00Z",
                "2026-09-15T02:00:00Z",
                "ek-b",
            ),
            existing(
                "gone",
                "Old block",
                "2026-09-16T17:00:00Z",
                "2026-09-16T18:00:00Z",
                "ek-z",
            ),
        ];
        let changes = diff(&p, &marked, &[], false).unwrap();
        let ops: Vec<(&str, &str)> = changes
            .iter()
            .map(|c| (c.op.as_str(), c.marker.rsplit('/').next().unwrap()))
            .collect();
        assert_eq!(
            ops,
            vec![("delete", "gone"), ("update", "b"), ("create", "c")]
        );
        assert_eq!(counts(&changes, &p)["unchanged"], 1);
        assert_eq!(
            diff(&p, &marked, &[], false).unwrap(),
            changes,
            "same inputs, same approval"
        );
    }

    #[test]
    fn adopt_claims_an_exact_unmarked_match_once_and_never_touches_others() {
        let p = plan(vec![
            event(
                "a",
                "Lunch",
                "2026-09-14T11:15:00-07:00",
                "2026-09-14T12:00:00-07:00",
            ),
            event(
                "b",
                "Lunch",
                "2026-09-14T11:15:00-07:00",
                "2026-09-14T12:00:00-07:00",
            ),
        ]);
        let mut loose = existing(
            "a",
            "Lunch",
            "2026-09-14T18:15:00Z",
            "2026-09-14T19:00:00Z",
            "ek-loose",
        );
        loose.marker = String::new();
        let mut personal = existing(
            "x",
            "Dentist",
            "2026-09-14T17:00:00Z",
            "2026-09-14T18:00:00Z",
            "ek-mine",
        );
        personal.marker = String::new();
        let changes = diff(&p, &[], &[loose, personal], true).unwrap();
        let ops: Vec<&str> = changes.iter().map(|c| c.op.as_str()).collect();
        assert_eq!(
            ops,
            vec!["adopt", "create"],
            "one exact match adopted, the twin created"
        );
        assert!(changes
            .iter()
            .all(|c| c.external_id.as_deref() != Some("ek-mine")));
        let plain = diff(&p, &[], &[], false).unwrap();
        assert!(
            plain.iter().all(|c| c.op == "create"),
            "without --adopt nothing unmarked is claimed"
        );
    }

    #[test]
    fn duplicate_markers_keep_one_and_events_outside_the_window_are_left_alone() {
        let p = plan(vec![event(
            "a",
            "Lunch",
            "2026-09-14T11:15:00-07:00",
            "2026-09-14T12:00:00-07:00",
        )]);
        let marked = vec![
            existing(
                "a",
                "Lunch",
                "2026-09-14T18:15:00Z",
                "2026-09-14T19:00:00Z",
                "ek-1",
            ),
            existing(
                "a",
                "Lunch",
                "2026-09-14T18:15:00Z",
                "2026-09-14T19:00:00Z",
                "ek-2",
            ),
            existing(
                "old",
                "August block",
                "2026-08-31T18:00:00Z",
                "2026-08-31T19:00:00Z",
                "ek-aug",
            ),
        ];
        let changes = diff(&p, &marked, &[], false).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            (changes[0].op.as_str(), changes[0].external_id.as_deref()),
            ("delete", Some("ek-2"))
        );
    }

    #[test]
    fn validation_rejects_ambiguous_or_out_of_scope_plans() {
        let ok = event(
            "a",
            "Lunch",
            "2026-09-14T11:15:00-07:00",
            "2026-09-14T12:00:00-07:00",
        );
        assert!(validate(&plan(vec![ok.clone()])).is_ok());
        assert!(
            validate(&plan(vec![ok.clone(), ok.clone()])).is_err(),
            "duplicate key"
        );
        let mut other_calendar = ok.clone();
        other_calendar.calendar = "Work".into();
        assert!(
            validate(&plan(vec![other_calendar])).is_err(),
            "calendar not named by the plan"
        );
        let outside = event(
            "b",
            "Lunch",
            "2026-10-02T11:15:00-07:00",
            "2026-10-02T12:00:00-07:00",
        );
        assert!(
            validate(&plan(vec![outside])).is_err(),
            "starts outside window"
        );
        let floating = event("c", "Lunch", "2026-09-14T11:15:00", "2026-09-14T12:00:00");
        assert!(validate(&plan(vec![floating])).is_err(), "offset required");
        let mut upper = plan(vec![]);
        upper.plan_id = "Devon".into();
        assert!(validate(&upper).is_err());
    }
}
