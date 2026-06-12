//! Heiwa Calendar read model: local-first holds plus connector lane status.
//!
//! Holds are the only writable lane today and they are local-only; promoting
//! a hold to Apple/Google stays approval-gated and is not implemented here.

use anyhow::{anyhow, Result};
use chrono::Local;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const HOLD_KINDS: [&str; 3] = ["focus", "travel", "soft"];

fn calendar_state_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".heiwa")
        .join("state")
        .join("calendar")
}

fn holds_dir() -> PathBuf {
    calendar_state_dir().join("holds")
}

fn receipts_dir() -> PathBuf {
    calendar_state_dir().join("receipts")
}

pub fn run(args: &[String]) -> Result<()> {
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
        Some("hold") => hold(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown calendar command: {other}")),
    }
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
        "created_at": created_at,
        "external_promotion": "approval_required",
    });

    let dir = holds_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{id}.json")), hold.to_string())?;

    let receipt = json!({
        "receipt_id": format!("rcpt-{id}"),
        "kind": "calendar_hold_created",
        "hold_id": id,
        "title": title,
        "date": date,
        "source": source,
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
    let holds_today: Vec<Value> = holds
        .iter()
        .filter(|hold| hold.get("date").and_then(Value::as_str) == Some(today.as_str()))
        .cloned()
        .collect();

    // Today moments: holds plus dated appointments from the life read model.
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
            "status": "planned",
            "sync": "Device-local EventKit bridge with Calendar permission and change notifications.",
            "write": "Create/update events only after an approval lease is granted.",
            "evidence": "Local receipt for permission, sync, and before/after writes.",
        },
    ]);

    json!({
        "command": "calendar summary",
        "date": today,
        "timezone": "America/Vancouver",
        "lanes": lanes,
        "holds": holds,
        "today": moments,
        "counts": {
            "holds_total": holds.len(),
            "holds_today": holds_today.len(),
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
