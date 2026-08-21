//! Heiwa Calendar read model plus governed connector actions.
//!
//! Holds remain local truth. A named Apple Calendar promotion is an explicit
//! T2 effect and can execute only from the approval service.

use anyhow::{anyhow, Result};
use chrono::{Duration, Local, NaiveDateTime, TimeZone, Utc};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const HOLD_KINDS: [&str; 3] = ["focus", "travel", "soft"];
const MAX_EVENT_LINES: usize = 1000;
const DEFAULT_SYNC_LIMIT: usize = 50;
const DEFAULT_SYNC_DAYS_FORWARD: i64 = 45;
const GOOGLE_CALENDAR_API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const SYNC_POLICY: &str = "read-model-before-external-writes";
const GOOGLE_SYNC_SEMANTICS: &str =
    "full sync stores nextSyncToken; incremental sync handles 410 Gone with scoped wipe + full resync";

fn calendar_state_dir() -> PathBuf {
    crate::home::heiwa_state_dir().join("calendar")
}

fn holds_dir() -> PathBuf {
    calendar_state_dir().join("holds")
}

fn receipts_dir() -> PathBuf {
    calendar_state_dir().join("receipts")
}

fn events_snapshot_path() -> PathBuf {
    calendar_state_dir().join("events.jsonl")
}

fn google_sync_tokens_path() -> PathBuf {
    calendar_state_dir().join("google_sync_tokens.json")
}

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => {
            let payload = summary_payload();
            if has_flag(args, "--json") {
                println!("{payload}");
            } else {
                print_status(&payload);
            }
            Ok(())
        }
        Some("calendars") => calendars(&args[1..]),
        Some("sync") => sync(&args[1..]).await,
        Some("hold") => hold(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown calendar command: {other}")),
    }
}

fn calendars(args: &[String]) -> Result<()> {
    let source = flag_value(args, "--source")
        .unwrap_or_else(|| "apple".to_string())
        .replace('-', "_")
        .to_ascii_lowercase();
    if !["apple", "apple_calendar"].contains(&source.as_str()) {
        return Err(anyhow!(
            "unknown calendar resource source: {source} (expected apple)"
        ));
    }
    let payload = apple_calendar_resources_payload()?;
    if has_flag(args, "--json") {
        println!("{payload}");
    } else {
        println!("Apple calendars");
        for calendar in payload["calendars"].as_array().into_iter().flatten() {
            let name = calendar.get("name").and_then(Value::as_str).unwrap_or("?");
            let writable = calendar
                .get("writable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            println!("  {name}{}", if writable { "" } else { " (read-only)" });
        }
        println!("  revoke: System Settings > Privacy & Security > Automation > heiwa > Calendar");
    }
    Ok(())
}

pub(crate) fn apple_calendar_resources_payload() -> Result<Value> {
    let calendars = crate::cmd::calendar_apple::list_calendars()?;
    Ok(json!({
        "source": "apple_calendar",
        "status": "ready",
        "auth": {
            "mode": "macos_automation",
            "owner": "macOS",
            "secrets": "none",
        },
        "calendars": calendars,
        "revoke": {
            "owner": "macOS",
            "path": "System Settings > Privacy & Security > Automation > heiwa > Calendar",
        },
    }))
}

pub(crate) fn apple_calendar_resources_payload_or_error() -> Value {
    apple_calendar_resources_payload().unwrap_or_else(|error| {
        json!({
            "source": "apple_calendar",
            "status": "error",
            "calendars": [],
            "error": error.to_string(),
            "revoke": {
                "owner": "macOS",
                "path": "System Settings > Privacy & Security > Automation > heiwa > Calendar",
            },
        })
    })
}

fn print_status(payload: &Value) {
    println!("calendar");
    if let Some(lanes) = payload.get("lanes").and_then(Value::as_array) {
        println!("  lanes:");
        for lane in lanes {
            let id = lane.get("id").and_then(Value::as_str).unwrap_or("?");
            let status = lane.get("status").and_then(Value::as_str).unwrap_or("?");
            println!("    {id:<16} {status}");
        }
    }
    if let Some(holds) = payload.get("holds").and_then(Value::as_array) {
        println!("  holds: {}", holds.len());
        for hold in holds.iter().take(5) {
            let date = hold.get("date").and_then(Value::as_str).unwrap_or("?");
            let start = hold.get("start").and_then(Value::as_str).unwrap_or("--:--");
            let title = hold.get("title").and_then(Value::as_str).unwrap_or("?");
            println!("    {date} {start} {title}");
        }
    }
    println!("  next: heiwa calendar hold add \"Focus block\" --date YYYY-MM-DD --start HH:MM --end HH:MM");
}

fn hold(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let title = args
                .get(1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("usage: heiwa calendar hold add <title> [--date --start --end --kind --note]"))?;
            let date = flag_value(args, "--date")
                .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
            let request = json!({
                "title": title,
                "date": date,
                "start": flag_value(args, "--start"),
                "end": flag_value(args, "--end"),
                "kind": flag_value(args, "--kind").unwrap_or_else(|| "focus".to_string()),
                "note": flag_value(args, "--note"),
                "source": "cli",
            });
            let created = create_hold(&request)?;
            println!("{created}");
            Ok(())
        }
        Some("list") | None => {
            let holds = load_holds();
            if has_flag(args, "--json") {
                println!("{}", json!({ "holds": holds }));
                return Ok(());
            }
            for hold in holds {
                let date = hold.get("date").and_then(Value::as_str).unwrap_or("?");
                let start = hold.get("start").and_then(Value::as_str).unwrap_or("--:--");
                let title = hold.get("title").and_then(Value::as_str).unwrap_or("?");
                println!("{date} {start} {title}");
            }
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown hold command: {other}")),
    }
}

async fn sync(args: &[String]) -> Result<()> {
    let source = flag_value(args, "--source")
        .unwrap_or_else(|| "all".to_string())
        .replace('-', "_")
        .to_ascii_lowercase();
    if ![
        "all",
        "apple",
        "apple_calendar",
        "google",
        "google_calendar",
    ]
    .contains(&source.as_str())
    {
        return Err(anyhow!(
            "unknown calendar sync source: {source} (expected all, apple, or google)"
        ));
    }
    let limit = flag_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_SYNC_LIMIT)
        .clamp(1, 500);
    let days = flag_value(args, "--days")
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(DEFAULT_SYNC_DAYS_FORWARD)
        .clamp(1, 366);
    let dry_run = has_flag(args, "--dry-run");
    let json_output = has_flag(args, "--json");

    if dry_run {
        println!("{}", calendar_sync_dry_run_payload(&source, limit, days));
        return Ok(());
    }

    let mut fetched: Vec<Value> = Vec::new();
    let mut source_reports: Vec<Value> = Vec::new();

    if source == "all" || source == "apple" || source == "apple_calendar" {
        if calendar_app_present() {
            match scan_apple_calendar(limit, days) {
                Ok(rows) => {
                    source_reports.push(json!({
                        "source": "apple", "status": "scanned", "fetched": rows.len(),
                        "bridge": "Calendar.app JXA metadata bridge",
                    }));
                    fetched.extend(rows);
                }
                Err(error) => source_reports.push(json!({
                    "source": "apple", "status": "error", "error": error.to_string(),
                    "bridge": "Calendar.app JXA metadata bridge",
                })),
            }
        } else {
            source_reports.push(json!({
                "source": "apple", "status": "skipped", "reason": "Calendar.app not present",
            }));
        }
    }

    if source == "all" || source == "google" || source == "google_calendar" {
        if crate::cmd::connectors::google_lane_status("google_calendar") == "connected" {
            match sync_google_calendar(limit.saturating_sub(fetched.len()).max(1), days).await {
                Ok((rows, reports)) => {
                    fetched.extend(rows);
                    source_reports.extend(reports);
                }
                Err(error) => source_reports.push(json!({
                    "source": "google", "status": "error", "error": error.to_string(),
                    "sync_semantics": GOOGLE_SYNC_SEMANTICS,
                })),
            }
        } else {
            source_reports.push(json!({
                "source": "google", "status": "needs_auth",
                "sync_semantics": GOOGLE_SYNC_SEMANTICS,
                "next": [
                    "heiwa connect google-calendar --client-secret <path>",
                    "heiwa connect google-calendar --authorize",
                ],
            }));
        }
    }

    let appended = append_event_rows(&fetched)?;
    let receipt = write_sync_receipt(&source_reports, fetched.len(), appended)?;
    let payload = json!({
        "command": "calendar sync",
        "policy": SYNC_POLICY,
        "sources": source_reports,
        "fetched": fetched.len(),
        "appended": appended,
        "deduplicated": fetched.len().saturating_sub(appended),
        "snapshot": events_snapshot_path().display().to_string(),
        "receipt": receipt,
    });
    if json_output {
        println!("{payload}");
    } else {
        println!("calendar sync");
        println!("  fetched: {}", fetched.len());
        println!("  appended: {appended}");
        for report in payload["sources"].as_array().unwrap_or(&Vec::new()) {
            let source = report.get("source").and_then(Value::as_str).unwrap_or("?");
            let status = report.get("status").and_then(Value::as_str).unwrap_or("?");
            println!("  {source}: {status}");
        }
        println!("  snapshot: {}", events_snapshot_path().display());
    }
    Ok(())
}

fn calendar_sync_dry_run_payload(source: &str, limit: usize, days: i64) -> Value {
    let apple_selected = source == "all" || source == "apple" || source == "apple_calendar";
    let google_selected = source == "all" || source == "google" || source == "google_calendar";
    let google_token = crate::cmd::connectors::google_lane_status("google_calendar") == "connected";
    json!({
        "command": "calendar sync",
        "dry_run": true,
        "policy": SYNC_POLICY,
        "limit": limit,
        "days_forward": days,
        "snapshot": events_snapshot_path().display().to_string(),
        "sources": {
            "apple": {
                "selected": apple_selected,
                "ready": calendar_app_present(),
                "bridge": "Calendar.app JXA metadata bridge",
                "fields": ["calendar", "title", "start", "end", "date", "status"],
                "external_writes": "approval_required",
            },
            "google": {
                "selected": google_selected,
                "ready": google_token,
                "blocker": if google_token { Value::Null } else {
                    Value::String("no token; run: heiwa connect google-calendar --client-secret <path> then --authorize".into())
                },
                "sync_semantics": GOOGLE_SYNC_SEMANTICS,
                "external_writes": "approval_required_not_implemented_here",
            },
        },
    })
}

fn calendar_app_present() -> bool {
    crate::cmd::calendar_apple::bridge_available()
}

fn local_timezone_name() -> String {
    fs::read_link("/etc/localtime")
        .ok()
        .and_then(|path| {
            let path = path.to_string_lossy();
            path.split("zoneinfo/").nth(1).map(str::to_string)
        })
        .unwrap_or_else(|| "local".to_string())
}

const APPLE_CALENDAR_JXA: &str = r#"
function run(argv) {
  const limit = parseInt(argv[0] || "50", 10);
  const startWindow = new Date(argv[1]);
  const endWindow = new Date(argv[2]);
  const Calendar = Application("Calendar");
  const rows = [];
  let calendars = [];
  try { calendars = Calendar.calendars(); } catch (e) { return ""; }
  for (let c = 0; c < calendars.length && rows.length < limit; c++) {
    let calendarName = "Calendar";
    let events = [];
    try { calendarName = String(calendars[c].name()); } catch (e) {}
    try { events = calendars[c].events(); } catch (e) { continue; }
    for (let i = 0; i < events.length && rows.length < limit; i++) {
      try {
        const event = events[i];
        const start = event.startDate();
        const end = event.endDate();
        if (!start || start < startWindow || start > endWindow) continue;
        let externalId = "";
        try { externalId = String(event.uid()); } catch (e) {}
        rows.push(JSON.stringify({
          source: "apple_calendar",
          calendar: calendarName,
          title: String(event.summary()),
          start: start.toISOString(),
          end: end ? end.toISOString() : null,
          date: start.toISOString().slice(0, 10),
          status: "confirmed",
          external_id: externalId,
        }));
      } catch (e) {}
    }
  }
  return rows.join("\n");
}
"#;

fn scan_apple_calendar(limit: usize, days: i64) -> Result<Vec<Value>> {
    use anyhow::Context;
    let start = Utc::now();
    let end = start + Duration::days(days);
    let output = std::process::Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(APPLE_CALENDAR_JXA)
        .arg(limit.to_string())
        .arg(start.to_rfc3339())
        .arg(end.to_rfc3339())
        .output()
        .context("failed to run osascript for Apple Calendar sync")?;
    if !output.status.success() {
        return Err(anyhow!(
            "Apple Calendar sync failed (Automation permission?): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .map(ensure_event_identity)
        .collect())
}

async fn sync_google_calendar(limit: usize, days: i64) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut access_token =
        crate::cmd::connectors::connector_access_token("google_calendar").await?;
    let base = google_calendar_api_base();
    let list_url = format!("{base}/users/me/calendarList?maxResults=50");
    let listing = google_calendar_get_retry(&list_url, &mut access_token).await?;
    if let Some(error) = listing.get("error") {
        return Err(anyhow!("google calendar list failed: {error}"));
    }

    let calendars = listing
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    let mut reports = Vec::new();
    for calendar in calendars {
        if rows.len() >= limit {
            break;
        }
        let calendar_id = calendar
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("primary");
        let calendar_name = calendar
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or(calendar_id);
        match google_events_for_calendar(
            &base,
            &mut access_token,
            calendar_id,
            calendar_name,
            limit - rows.len(),
            days,
        )
        .await
        {
            Ok((mut events, report)) => {
                rows.append(&mut events);
                reports.push(report);
            }
            Err(error) => reports.push(json!({
                "source": "google", "calendar_id": calendar_id, "calendar": calendar_name,
                "status": "error", "error": error.to_string(), "sync_semantics": GOOGLE_SYNC_SEMANTICS,
            })),
        }
    }
    Ok((rows, reports))
}

async fn google_events_for_calendar(
    base: &str,
    access_token: &mut String,
    calendar_id: &str,
    calendar_name: &str,
    limit: usize,
    days: i64,
) -> Result<(Vec<Value>, Value)> {
    let encoded_id = url_encode(calendar_id);
    let mut mode = "full";
    let mut used_410_recovery = false;
    let sync_token = google_sync_token_for(calendar_id);
    let mut url = if let Some(token) = sync_token.as_deref() {
        mode = "incremental";
        format!(
            "{base}/calendars/{encoded_id}/events?maxResults={limit}&showDeleted=true&syncToken={}",
            url_encode(token)
        )
    } else {
        google_full_events_url(base, &encoded_id, limit, days)
    };

    let mut payload = google_calendar_get_retry(&url, access_token).await?;
    if google_error_code(&payload) == Some(410) && sync_token.is_some() {
        clear_google_sync_token(calendar_id)?;
        remove_google_calendar_events(calendar_id)?;
        used_410_recovery = true;
        mode = "full_after_410";
        url = google_full_events_url(base, &encoded_id, limit, days);
        payload = google_calendar_get_retry(&url, access_token).await?;
    }
    if let Some(error) = payload.get("error") {
        return Err(anyhow!("google calendar events failed: {error}"));
    }

    let rows = payload
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|event| normalize_google_event(event, calendar_id, calendar_name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let wrote_token = if let Some(token) = payload.get("nextSyncToken").and_then(Value::as_str) {
        write_google_sync_token(calendar_id, token)?;
        true
    } else {
        false
    };

    Ok((
        rows.clone(),
        json!({
            "source": "google",
            "calendar_id": calendar_id,
            "calendar": calendar_name,
            "status": "scanned",
            "mode": mode,
            "fetched": rows.len(),
            "next_sync_token_stored": wrote_token,
            "used_410_recovery": used_410_recovery,
            "sync_semantics": GOOGLE_SYNC_SEMANTICS,
        }),
    ))
}

fn google_full_events_url(
    base: &str,
    encoded_calendar_id: &str,
    limit: usize,
    days: i64,
) -> String {
    let start = Utc::now();
    let end = start + Duration::days(days);
    format!(
        "{base}/calendars/{encoded_calendar_id}/events?maxResults={limit}&singleEvents=true&orderBy=startTime&timeMin={}&timeMax={}",
        url_encode(&start.to_rfc3339()),
        url_encode(&end.to_rfc3339())
    )
}

fn normalize_google_event(event: &Value, calendar_id: &str, calendar_name: &str) -> Option<Value> {
    let external_id = event.get("id").and_then(Value::as_str).unwrap_or("");
    let title = event
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("(busy)");
    let start = google_event_time(event.get("start")?)?;
    let end = event.get("end").and_then(google_event_time);
    let date = event_date(&start);
    Some(ensure_event_identity(json!({
        "source": "google_calendar",
        "calendar_id": calendar_id,
        "calendar": calendar_name,
        "title": title,
        "start": start,
        "end": end,
        "date": date,
        "status": event.get("status").and_then(Value::as_str).unwrap_or("confirmed"),
        "external_id": external_id,
        "etag": event.get("etag").and_then(Value::as_str),
        "updated": event.get("updated").and_then(Value::as_str),
    })))
}

fn google_event_time(value: &Value) -> Option<String> {
    value
        .get("dateTime")
        .and_then(Value::as_str)
        .or_else(|| value.get("date").and_then(Value::as_str))
        .map(str::to_string)
}

fn google_calendar_api_base() -> String {
    std::env::var("HEIWA_GOOGLE_CALENDAR_API_BASE")
        .unwrap_or_else(|_| GOOGLE_CALENDAR_API_BASE.to_string())
}

async fn google_calendar_get_retry(url: &str, access_token: &mut String) -> Result<Value> {
    let first = google_calendar_get(url, access_token)?;
    if google_error_code(&first) == Some(401) {
        *access_token =
            crate::cmd::connectors::force_refresh_connector_access_token("google_calendar").await?;
        return google_calendar_get(url, access_token);
    }
    Ok(first)
}

fn google_calendar_get(url: &str, access_token: &str) -> Result<Value> {
    use anyhow::Context;
    let auth_header = ["Authorization: Bearer ", access_token].concat();
    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("-H")
        .arg(auth_header)
        .arg(url)
        .output()
        .context("failed to run curl for Google Calendar request")?;
    if !output.status.success() {
        return Err(anyhow!(
            "Google Calendar request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Google Calendar returned non-JSON: {e}"))
}

fn google_error_code(payload: &Value) -> Option<i64> {
    payload
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64)
}

fn google_sync_token_for(calendar_id: &str) -> Option<String> {
    let raw = fs::read_to_string(google_sync_tokens_path()).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get(calendar_id)
        .and_then(|entry| entry.get("sync_token"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn write_google_sync_token(calendar_id: &str, token: &str) -> Result<()> {
    let path = google_sync_tokens_path();
    let mut parsed: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(obj) = parsed.as_object_mut() {
        obj.insert(
            calendar_id.to_string(),
            json!({"sync_token": token, "updated_at": Utc::now().to_rfc3339()}),
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&parsed)?)?;
    Ok(())
}

fn clear_google_sync_token(calendar_id: &str) -> Result<()> {
    let path = google_sync_tokens_path();
    let mut parsed: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(obj) = parsed.as_object_mut() {
        obj.remove(calendar_id);
    }
    if path.exists() {
        fs::write(path, serde_json::to_string_pretty(&parsed)?)?;
    }
    Ok(())
}

fn load_events() -> Vec<Value> {
    let raw = fs::read_to_string(events_snapshot_path()).unwrap_or_default();
    let mut events: Vec<Value> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .collect();
    events.sort_by_key(event_sort_key);
    events
}

fn append_event_rows(rows: &[Value]) -> Result<usize> {
    append_event_rows_at(&events_snapshot_path(), rows)
}

fn append_event_rows_at(path: &Path, rows: &[Value]) -> Result<usize> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing_raw = fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = existing_raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    let mut seen: HashSet<String> = lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|row| event_dedupe_key(&row))
        .collect();

    let synced_at = Utc::now().to_rfc3339();
    let mut appended = 0;
    for row in rows {
        let row = ensure_event_identity(row.clone());
        let key = event_dedupe_key(&row);
        if seen.contains(&key) {
            continue;
        }
        let mut stamped = row;
        if let Some(obj) = stamped.as_object_mut() {
            obj.insert("synced_at".to_string(), json!(synced_at));
        }
        lines.push(stamped.to_string());
        seen.insert(key);
        appended += 1;
    }

    if lines.len() > MAX_EVENT_LINES {
        let drop = lines.len() - MAX_EVENT_LINES;
        lines.drain(..drop);
    }
    fs::write(path, lines.join("\n") + "\n")?;
    Ok(appended)
}

fn remove_google_calendar_events(calendar_id: &str) -> Result<()> {
    let path = events_snapshot_path();
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&path)?;
    let kept: Vec<String> = raw
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .map(|row| {
                    row.get("source").and_then(Value::as_str) != Some("google_calendar")
                        || row.get("calendar_id").and_then(Value::as_str) != Some(calendar_id)
                })
                .unwrap_or(true)
        })
        .map(str::to_string)
        .collect();
    fs::write(
        &path,
        kept.join("\n") + if kept.is_empty() { "" } else { "\n" },
    )?;
    Ok(())
}

fn write_sync_receipt(sources: &[Value], fetched: usize, appended: usize) -> Result<String> {
    let receipts = receipts_dir();
    fs::create_dir_all(&receipts)?;
    let receipt_id = format!("calendar-sync-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));
    let receipt = json!({
        "receipt_id": receipt_id,
        "kind": "calendar_sync",
        "policy": SYNC_POLICY,
        "sources": sources,
        "fetched": fetched,
        "appended": appended,
        "snapshot": events_snapshot_path().display().to_string(),
        "synced_at": Utc::now().to_rfc3339(),
        "external_writes": [],
    });
    fs::write(
        receipts.join(format!("{receipt_id}.json")),
        receipt.to_string(),
    )?;
    Ok(receipt_id)
}

fn ensure_event_identity(mut row: Value) -> Value {
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| stable_event_id(&event_dedupe_key(&row)));
    let source = row
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("calendar")
        .to_string();
    let deadline = row.get("start").cloned().unwrap_or(Value::Null);
    if let Some(obj) = row.as_object_mut() {
        obj.insert("id".to_string(), json!(id));
        obj.entry("signal".to_string()).or_insert_with(|| {
            json!({
                "source": source,
                "deadline": deadline,
                "actionable": false,
                "privacy": "personal",
            })
        });
    }
    row
}

fn stable_event_id(key: &str) -> String {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(key.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("evt_{}", &hex[..16])
}

fn event_dedupe_key(row: &Value) -> String {
    let field = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        field("source"),
        field("calendar_id"),
        field("calendar"),
        field("external_id"),
        field("title"),
        field("start"),
        field("end"),
    )
}

fn event_sort_key(row: &Value) -> String {
    format!(
        "{} {} {}",
        row.get("date")
            .and_then(Value::as_str)
            .unwrap_or("9999-99-99"),
        row.get("start").and_then(Value::as_str).unwrap_or("99:99"),
        row.get("title").and_then(Value::as_str).unwrap_or("")
    )
}

fn event_date(start: &str) -> String {
    start.chars().take(10).collect()
}

fn event_occurs_on(event: &Value, date: &str) -> bool {
    event.get("date").and_then(Value::as_str) == Some(date)
        || event
            .get("start")
            .and_then(Value::as_str)
            .is_some_and(|start| start.starts_with(date))
}

fn event_start_time(event: &Value) -> String {
    let Some(start) = event.get("start").and_then(Value::as_str) else {
        return "--:--".to_string();
    };
    if start.len() >= 16 && start.as_bytes().get(10) == Some(&b'T') {
        start[11..16].to_string()
    } else {
        "all-day".to_string()
    }
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

/// Validate and persist a hold plus its local receipt. Shared by CLI and the
/// app POST endpoint so both lanes produce identical evidence.
pub(crate) fn create_hold(request: &Value) -> Result<Value> {
    let title = request
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| anyhow!("hold title is required"))?;
    if title.len() > 200 {
        return Err(anyhow!("hold title too long (max 200 chars)"));
    }

    let date = request
        .get("date")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| anyhow!("hold date must be YYYY-MM-DD"))?;

    let start = optional_time(request, "start")?;
    let end = optional_time(request, "end")?;
    if let (Some(start), Some(end)) = (&start, &end) {
        if end <= start {
            return Err(anyhow!("hold end must be after start"));
        }
    }

    let kind = request
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("focus")
        .to_ascii_lowercase();
    if !HOLD_KINDS.contains(&kind.as_str()) {
        return Err(anyhow!("hold kind must be one of: focus, travel, soft"));
    }

    let note = request
        .get("note")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|note| !note.is_empty());
    let source = request
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("cockpit");
    let work_id = request
        .get("work_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("work_{}", uuid::Uuid::new_v4().simple()));
    if work_id.len() > 200 {
        return Err(anyhow!("hold work_id too long (max 200 chars)"));
    }
    let promotion = normalized_promotion(request.get("promotion"))?;

    let id = format!(
        "hold-{}-{}",
        date.replace('-', ""),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let created_at = chrono::Utc::now().to_rfc3339();
    let hold = json!({
        "id": id,
        "title": title,
        "date": date,
        "start": start,
        "end": end,
        "kind": kind,
        "status": "draft",
        "note": note,
        "source": source,
        "work_id": work_id,
        "created_at": created_at,
        "external_promotion": if promotion.is_some() { "approval_required" } else { "local_only" },
        "promotion": promotion,
    });

    let dir = holds_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{id}.json")), hold.to_string())?;

    let receipt = json!({
        "receipt_id": format!("rcpt-{id}"),
        "kind": "calendar_hold_created",
        "hold_id": id,
        "work_id": work_id,
        "title": title,
        "date": date,
        "source": source,
        "promotion": promotion,
        "created_at": created_at,
        "external_writes": [],
    });
    let receipts = receipts_dir();
    fs::create_dir_all(&receipts)?;
    fs::write(
        receipts.join(format!("rcpt-{id}.json")),
        receipt.to_string(),
    )?;

    Ok(hold)
}

pub(crate) fn create_hold_from_app(request: &Value) -> Result<Value> {
    let mut request = request.clone();
    request["source"] = json!("heiwa.app");
    let promotion = normalized_promotion(request.get("promotion"))?;
    if let Some(promotion) = promotion.as_ref() {
        validate_promotion_target(promotion)?;
    }
    let hold = create_hold(&request)?;
    let Some(promotion) = promotion else {
        return Ok(json!({ "hold": hold }));
    };

    let hold_id = hold
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("staged hold has no id"))?;
    if let Err(error) = hold_interval(&hold) {
        let _ = fs::remove_file(holds_dir().join(format!("{hold_id}.json")));
        let _ = fs::remove_file(receipts_dir().join(format!("rcpt-{hold_id}.json")));
        return Err(anyhow!(
            "Apple Calendar staging requires a concrete local interval: {error}"
        ));
    }
    let work_id = hold
        .get("work_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("staged hold has no work_id"))?;
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let approval = json!({
        "schema_version": "operator_dispatch_request_v1",
        "request_id": request_id,
        "work_id": work_id,
        "created_at": Utc::now().to_rfc3339(),
        "action": "calendar-event-create",
        "target_surface": "calendar",
        "target_scope": hold_id,
        "requested_mode": "stage",
        "risk_tier": "T2",
        "intent": {
            "hold_id": hold_id,
            "title": hold.get("title"),
            "date": hold.get("date"),
            "start": hold.get("start"),
            "end": hold.get("end"),
            "note": hold.get("note"),
            "promotion": promotion,
        },
        "source": "Heiwa.app Calendar",
    });
    let approval_path = crate::cmd::approvals::requests_dir().join(format!("{request_id}.json"));
    let persisted = (|| -> Result<()> {
        if let Some(parent) = approval_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&approval_path, serde_json::to_string_pretty(&approval)?)?;
        Ok(())
    })();
    if let Err(error) = persisted {
        let _ = fs::remove_file(holds_dir().join(format!("{hold_id}.json")));
        let _ = fs::remove_file(receipts_dir().join(format!("rcpt-{hold_id}.json")));
        return Err(anyhow!(
            "failed to stage approval; newly created hold was rolled back: {error}"
        ));
    }

    Ok(json!({
        "hold": hold,
        "approval_request": approval,
        "approval_path": approval_path.display().to_string(),
    }))
}

pub(crate) fn normalized_promotion(raw: Option<&Value>) -> Result<Option<Value>> {
    let Some(raw) = raw.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let connector = raw
        .get("connector")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('-', "_")
        .to_ascii_lowercase();
    if !["apple", "apple_calendar"].contains(&connector.as_str()) {
        return Err(anyhow!(
            "calendar promotion connector must be apple_calendar"
        ));
    }
    let calendar = raw
        .get("calendar")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Apple Calendar promotion requires an exact calendar name"))?;
    if calendar.len() > 200 {
        return Err(anyhow!("Apple calendar name too long (max 200 chars)"));
    }
    Ok(Some(json!({
        "connector": "apple_calendar",
        "calendar": calendar,
    })))
}

pub(crate) fn validate_promotion_target(promotion: &Value) -> Result<()> {
    let normalized = normalized_promotion(Some(promotion))?
        .ok_or_else(|| anyhow!("calendar promotion target is missing"))?;
    let calendar = normalized["calendar"]
        .as_str()
        .ok_or_else(|| anyhow!("Apple calendar target is missing"))?;
    crate::cmd::calendar_apple::ensure_writable_calendar(calendar)
}

pub(crate) fn promote_hold_to_apple(
    hold_id: &str,
    calendar: &str,
    approval_id: &str,
    work_id: &str,
) -> Result<Value> {
    crate::cmd::calendar_apple::ensure_writable_calendar(calendar)?;
    let path = holds_dir().join(format!("{hold_id}.json"));
    let raw = fs::read_to_string(&path)
        .map_err(|error| anyhow!("hold {hold_id} could not be read: {error}"))?;
    let mut hold: Value = serde_json::from_str(&raw)
        .map_err(|error| anyhow!("hold {hold_id} is malformed: {error}"))?;
    if hold.get("work_id").and_then(Value::as_str) != Some(work_id) {
        return Err(anyhow!(
            "approval work_id does not match hold {hold_id}; refusing external write"
        ));
    }
    let promotion = normalized_promotion(hold.get("promotion"))?
        .ok_or_else(|| anyhow!("hold {hold_id} has no external promotion target"))?;
    if promotion.get("calendar").and_then(Value::as_str) != Some(calendar) {
        return Err(anyhow!(
            "approval calendar does not match hold {hold_id}; refusing external write"
        ));
    }

    let (start, end) = hold_interval(&hold)?;
    let title = hold
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("hold {hold_id} has no title"))?;
    let marker = format!("heiwa://calendar/holds/{hold_id}");
    let origin_device_id = match heiwa_install::load_machine_manifest() {
        Ok(Some(manifest)) => Some(manifest.device_id),
        Ok(None) => None,
        Err(error) => {
            return Err(anyhow!(
                "cannot bind connector receipt to this machine: {error}"
            ))
        }
    };
    let request = json!({
        "calendar": calendar,
        "title": title,
        "start": start,
        "end": end,
        "note": hold.get("note").and_then(Value::as_str).unwrap_or(""),
        "marker": marker,
    });
    let bridge_result = crate::cmd::calendar_apple::create_event(&request)?;
    let external_id = bridge_result
        .get("external_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Apple Calendar returned no external event id"))?;
    let observed_external_event = json!({
        "connector": "apple_calendar",
        "calendar": calendar,
        "external_id": external_id,
        "marker": marker,
        "title": title,
        "start": start,
        "end": end,
        "created": bridge_result.get("created").and_then(Value::as_bool).unwrap_or(false),
    });

    let created_at = Utc::now().to_rfc3339();
    let receipt_id = format!("rcpt-{hold_id}-apple-create");
    let proposed_receipt = json!({
        "schema_version": "heiwa_connector_receipt_v1",
        "receipt_id": receipt_id,
        "kind": "calendar_event_created",
        "work_id": work_id,
        "approval_id": approval_id,
        "connector": "apple_calendar",
        "action": "event.create",
        "origin_device_id": origin_device_id,
        "target": {
            "calendar": calendar,
            "marker": marker,
        },
        "external_id": external_id,
        "after": observed_external_event,
        "created_at": created_at,
        "undo": {
            "supported": true,
            "posture": "delete the exact external_id only through a new approval",
        },
    });

    fs::create_dir_all(receipts_dir())?;
    let receipt_path = receipts_dir().join(format!("{receipt_id}.json"));
    let journal = heiwa_evidence::JsonlTransport::default_local()?;
    let replay = heiwa_evidence::read_stream(journal.dir(), "connector_receipts")?;
    let journal_receipt = replay
        .events
        .into_iter()
        .find(|event| {
            event.record.get("receipt_id").and_then(Value::as_str) == Some(receipt_id.as_str())
        })
        .map(|event| event.record);
    let file_receipt = if receipt_path.exists() {
        let raw = fs::read_to_string(&receipt_path)?;
        Some(serde_json::from_str::<Value>(&raw).map_err(|error| {
            anyhow!(
                "existing connector receipt {} is malformed: {error}",
                receipt_path.display()
            )
        })?)
    } else {
        None
    };
    if let (Some(journal_receipt), Some(file_receipt)) = (&journal_receipt, &file_receipt) {
        if journal_receipt != file_receipt {
            return Err(anyhow!(
                "connector receipt file and journal disagree for {receipt_id}"
            ));
        }
    }
    let receipt = journal_receipt
        .clone()
        .or(file_receipt)
        .unwrap_or(proposed_receipt);
    if receipt.get("work_id").and_then(Value::as_str) != Some(work_id)
        || receipt.get("approval_id").and_then(Value::as_str) != Some(approval_id)
        || receipt.get("external_id").and_then(Value::as_str) != Some(external_id)
    {
        return Err(anyhow!(
            "existing connector receipt does not match this approval or external event"
        ));
    }
    let external_event = receipt
        .get("after")
        .cloned()
        .ok_or_else(|| anyhow!("connector receipt has no after state"))?;
    let promoted_at = receipt
        .get("created_at")
        .cloned()
        .unwrap_or_else(|| json!(created_at));
    fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)?;
    use heiwa_evidence::EvidenceTransport;
    if journal_receipt.is_none() {
        journal.journal("connector_receipts", receipt.clone())?;
    }

    hold["external_promotion"] = json!("promoted");
    hold["external_event"] = external_event.clone();
    hold["promoted_at"] = promoted_at;
    hold["promoted_by_decision"] = json!(approval_id);
    fs::write(&path, serde_json::to_string_pretty(&hold)?)?;

    Ok(external_event)
}

fn hold_interval(hold: &Value) -> Result<(String, String)> {
    let date = hold
        .get("date")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("external calendar promotion requires a date"))?;
    let start = hold
        .get("start")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("external calendar promotion requires a start time"))?;
    let end = hold
        .get("end")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("external calendar promotion requires an end time"))?;
    let parse_local = |time: &str| -> Result<String> {
        let naive = NaiveDateTime::parse_from_str(&format!("{date}T{time}"), "%Y-%m-%dT%H:%M")
            .map_err(|_| anyhow!("external calendar promotion has an invalid date/time"))?;
        Local
            .from_local_datetime(&naive)
            .single()
            .map(|value| value.to_rfc3339())
            .ok_or_else(|| {
                anyhow!("external calendar promotion time is ambiguous or skipped locally")
            })
    };
    Ok((parse_local(start)?, parse_local(end)?))
}

/// Flip a hold's status and write a status-change receipt. Idempotent on
/// the same target status: a re-approval of an already-confirmed hold
/// overwrites the same fields and re-emits the receipt (so the audit trail
/// records each decision, not just the latest one).
///
/// This is the on-approval wire for the schedule→approvals→calendar loop.
/// Both the cockpit POST lane and `heiwa schedule` produce holds with
/// `status: "draft"`; `heiwa approvals decide --approve` is the only caller
/// that flips it to `confirmed`. `--deny` is the only caller that removes
/// the draft.
pub(crate) fn update_hold_status(
    hold_id: &str,
    new_status: &str,
    by_decision: &str,
) -> Result<Value> {
    if !["draft", "confirmed", "cancelled"].contains(&new_status) {
        return Err(anyhow!(
            "hold status must be one of: draft, confirmed, cancelled"
        ));
    }

    let path = holds_dir().join(format!("{hold_id}.json"));
    if !path.exists() {
        return Err(anyhow!(
            "hold {hold_id} not found in {}/",
            holds_dir().display()
        ));
    }
    let raw = fs::read_to_string(&path)?;
    let mut hold: Value =
        serde_json::from_str(&raw).map_err(|e| anyhow!("hold {hold_id} is malformed: {e}"))?;

    let now = chrono::Utc::now().to_rfc3339();
    let work_id = hold.get("work_id").cloned().unwrap_or(Value::Null);
    let external_writes = hold
        .get("external_event")
        .cloned()
        .map(|event| json!([event]))
        .unwrap_or_else(|| json!([]));
    hold["status"] = json!(new_status);
    if new_status == "confirmed" {
        hold["confirmed_at"] = json!(now);
        hold["confirmed_by_decision"] = json!(by_decision);
    } else if new_status == "cancelled" {
        hold["cancelled_at"] = json!(now);
        hold["cancelled_by_decision"] = json!(by_decision);
    } else {
        hold["reverted_at"] = json!(now);
        hold["reverted_by_decision"] = json!(by_decision);
    }

    fs::write(&path, serde_json::to_string_pretty(&hold)?)?;

    let receipt = json!({
        "receipt_id": format!("rcpt-{hold_id}-status-{new_status}"),
        "kind": "calendar_hold_status_changed",
        "hold_id": hold_id,
        "work_id": work_id,
        "new_status": new_status,
        "by_decision": by_decision,
        "created_at": now,
        "external_writes": external_writes,
    });
    let receipts = receipts_dir();
    fs::create_dir_all(&receipts)?;
    fs::write(
        receipts.join(format!("rcpt-{hold_id}-status-{new_status}.json")),
        receipt.to_string(),
    )?;

    Ok(hold)
}

/// Drop a draft hold and emit a removal receipt. Used on `--deny` so the
/// spine doesn't accumulate orphaned drafts. Confirmed holds are not
/// removable this way — cancellation goes through `update_hold_status`
/// with `cancelled` so the audit trail stays intact.
pub(crate) fn drop_draft_hold(hold_id: &str, by_decision: &str) -> Result<()> {
    let path = holds_dir().join(format!("{hold_id}.json"));
    if !path.exists() {
        return Err(anyhow!("hold {hold_id} not found"));
    }
    let raw = fs::read_to_string(&path)?;
    let hold: Value =
        serde_json::from_str(&raw).map_err(|e| anyhow!("hold {hold_id} is malformed: {e}"))?;
    if hold.get("status").and_then(Value::as_str) != Some("draft") {
        return Err(anyhow!(
            "hold {hold_id} is not a draft (status={}); use --status cancelled instead",
            hold.get("status").and_then(Value::as_str).unwrap_or("?")
        ));
    }
    fs::remove_file(&path)?;

    let now = chrono::Utc::now().to_rfc3339();
    let work_id = hold.get("work_id").cloned().unwrap_or(Value::Null);
    let receipt = json!({
        "receipt_id": format!("rcpt-{hold_id}-dropped"),
        "kind": "calendar_hold_dropped",
        "hold_id": hold_id,
        "work_id": work_id,
        "by_decision": by_decision,
        "created_at": now,
        "external_writes": [],
    });
    let receipts = receipts_dir();
    fs::create_dir_all(&receipts)?;
    fs::write(
        receipts.join(format!("rcpt-{hold_id}-dropped.json")),
        receipt.to_string(),
    )?;
    Ok(())
}

fn optional_time(request: &Value, key: &str) -> Result<Option<String>> {
    let Some(raw) = request.get(key).and_then(Value::as_str).map(str::trim) else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    chrono::NaiveTime::parse_from_str(raw, "%H:%M")
        .map_err(|_| anyhow!("hold {key} must be HH:MM"))?;
    Ok(Some(raw.to_string()))
}

pub(crate) fn load_holds() -> Vec<Value> {
    let mut holds = Vec::new();
    let Ok(entries) = fs::read_dir(holds_dir()) else {
        return holds;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                holds.push(value);
            }
        }
    }
    holds.sort_by(|a, b| {
        let key = |hold: &Value| {
            format!(
                "{} {}",
                hold.get("date").and_then(Value::as_str).unwrap_or(""),
                hold.get("start").and_then(Value::as_str).unwrap_or("99:99"),
            )
        };
        key(a).cmp(&key(b))
    });
    holds
}

pub(crate) fn holds_for_date(date: &str) -> Vec<Value> {
    load_holds()
        .into_iter()
        .filter(|hold| hold.get("date").and_then(Value::as_str) == Some(date))
        .collect()
}

/// The calendar summary read model served at /api/v1/calendar/summary.
pub(crate) fn summary_payload() -> Value {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let holds = load_holds();
    let events = load_events();
    let holds_today: Vec<Value> = holds
        .iter()
        .filter(|hold| hold.get("date").and_then(Value::as_str) == Some(today.as_str()))
        .cloned()
        .collect();
    let events_today: Vec<Value> = events
        .iter()
        .filter(|event| event_occurs_on(event, &today))
        .cloned()
        .collect();

    // Today moments: holds, synced calendar events, plus dated appointments from the life read model.
    let mut moments: Vec<Value> = holds_today
        .iter()
        .map(|hold| {
            json!({
                "time": hold.get("start").and_then(Value::as_str).unwrap_or("--:--"),
                "title": hold.get("title"),
                "source": "Heiwa Hold",
                "pressure": if hold.get("status").and_then(Value::as_str) == Some("draft") { "draft" } else { "soft" },
                "detail": hold.get("note").and_then(Value::as_str).unwrap_or("Local hold; promote externally only after approval."),
            })
        })
        .collect();
    for event in &events_today {
        moments.push(json!({
            "time": event_start_time(event),
            "title": event.get("title").and_then(Value::as_str).unwrap_or("Calendar event"),
            "source": event.get("source").and_then(Value::as_str).unwrap_or("calendar"),
            "pressure": "fixed",
            "detail": event.get("calendar").and_then(Value::as_str).unwrap_or("synced calendar read model"),
        }));
    }
    for appointment in crate::cmd::life::appointments_for_today() {
        moments.push(json!({
            "time": "--:--",
            "title": format!(
                "{} appointment",
                appointment.get("kind").and_then(Value::as_str).unwrap_or("external")
            ),
            "source": "life register",
            "pressure": "fixed",
            "detail": appointment.get("note").and_then(Value::as_str).unwrap_or(""),
        }));
    }
    moments.sort_by(|a, b| {
        let key = |m: &Value| {
            m.get("time")
                .and_then(Value::as_str)
                .unwrap_or("99:99")
                .to_string()
        };
        key(a).cmp(&key(b))
    });

    let google_status = crate::cmd::connectors::google_lane_status("google_calendar");
    let apple_status = if crate::cmd::calendar_apple::bridge_available() {
        "available"
    } else {
        "unavailable"
    };
    let lanes = json!([
        {
            "id": "heiwa_holds",
            "name": "Heiwa Holds",
            "status": "ready",
            "sync": "Local runtime state for focus blocks, travel buffers, and tentative commitments.",
            "write": "Promote holds to Apple or Google only after user approval.",
            "evidence": "Local hold receipt under ~/.heiwa/state/calendar/receipts.",
        },
        {
            "id": "google_calendar",
            "name": "Google Calendar",
            "status": google_status,
            "sync": "OAuth read-only full sync first; per-calendar incremental sync tokens after.",
            "write": "Draft focus holds and event edits before external writes.",
            "evidence": "Sync token, external event id, etag, approval id, undo posture.",
        },
        {
            "id": "apple_calendar",
            "name": "Apple Calendar",
            "status": apple_status,
            "sync": "Device-local Calendar.app read model through macOS Automation permission.",
            "write": "Exact-calendar event creation after a T2 approval; stable hold markers make retries idempotent.",
            "evidence": "External event id, work id, approval id, device handle, undo posture, and replayable connector receipt.",
        },
    ]);

    json!({
        "command": "calendar summary",
        "date": today,
        "timezone": local_timezone_name(),
        "lanes": lanes,
        "holds": holds,
        "events": events,
        "today": moments,
        "counts": {
            "holds_total": holds.len(),
            "holds_today": holds_today.len(),
            "events_total": events.len(),
            "events_today": events_today.len(),
            "moments_today": moments.len(),
        },
    })
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let idx = args.iter().position(|arg| arg == flag)?;
    args.get(idx + 1).cloned()
}

fn print_help() {
    println!("heiwa calendar");
    println!();
    println!("Usage:");
    println!("  heiwa calendar status [--json]");
    println!("  heiwa calendar sync [--source all|apple|google] [--dry-run] [--json]");
    println!("  heiwa calendar hold add <title> [--date YYYY-MM-DD] [--start HH:MM] [--end HH:MM] [--kind focus|travel|soft] [--note <text>]");
    println!("  heiwa calendar hold list [--json]");
    println!();
    println!("Holds are local-first; external promotion stays approval-gated.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_hold_rejects_bad_date() {
        let request = json!({"title": "x", "date": "june 12"});
        assert!(create_hold(&request).is_err());
    }

    #[test]
    fn create_hold_rejects_inverted_times() {
        let request = json!({"title": "x", "date": "2026-06-12", "start": "15:00", "end": "14:00"});
        assert!(create_hold(&request).is_err());
    }

    #[test]
    fn create_hold_rejects_unknown_kind() {
        let request = json!({"title": "x", "kind": "party"});
        assert!(create_hold(&request).is_err());
    }
}
