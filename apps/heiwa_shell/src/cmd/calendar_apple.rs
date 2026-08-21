//! Device-local Apple Calendar connector bridge.
//!
//! Calendar.app owns authorization through macOS Automation. This module owns
//! exact resource discovery and idempotent event creation; approval policy and
//! durable Heiwa evidence stay in the calendar/approval services.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

const APPLE_CALENDAR_JXA: &str = r#"
function calendarRow(calendar) {
  let name = "";
  let writable = false;
  try { name = String(calendar.name()); } catch (e) {}
  try { writable = Boolean(calendar.writable()); } catch (e) {}
  return { name, writable };
}

function eventRow(event, calendarName, marker, created) {
  return {
    calendar: calendarName,
    external_id: String(event.uid()),
    marker,
    title: String(event.summary()),
    start: event.startDate().toISOString(),
    end: event.endDate().toISOString(),
    created,
  };
}

function run(argv) {
  const operation = String(argv[0] || "");
  const Calendar = Application("Calendar");

  if (operation === "list") {
    return JSON.stringify(Calendar.calendars().map(calendarRow));
  }

  if (operation !== "create") {
    throw new Error(`unsupported Apple Calendar bridge operation: ${operation}`);
  }

  const request = JSON.parse(String(argv[1] || "{}"));
  const calendars = Calendar.calendars.whose({ name: String(request.calendar) })();
  if (calendars.length !== 1) {
    throw new Error(`expected exactly one Apple calendar named ${request.calendar}; found ${calendars.length}`);
  }
  const calendar = calendars[0];
  if (!Boolean(calendar.writable())) {
    throw new Error(`Apple calendar ${request.calendar} is read-only`);
  }

  const start = new Date(String(request.start));
  const end = new Date(String(request.end));
  if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end <= start) {
    throw new Error("Apple Calendar event requires a valid start and later end");
  }

  const marker = String(request.marker);
  const existing = calendar.events.whose({ url: marker })();
  if (existing.length > 1) {
    throw new Error(`Apple Calendar marker collision for ${marker}`);
  }
  if (existing.length === 1) {
    const event = existing[0];
    if (
      String(event.summary()) !== String(request.title) ||
      event.startDate().getTime() !== start.getTime() ||
      event.endDate().getTime() !== end.getTime()
    ) {
      throw new Error(`Apple Calendar marker ${marker} exists with different event content`);
    }
    return JSON.stringify(eventRow(event, String(request.calendar), marker, false));
  }

  const event = Calendar.Event({
    summary: String(request.title),
    startDate: start,
    endDate: end,
    description: String(request.note || ""),
    url: marker,
  });
  calendar.events.push(event);
  return JSON.stringify(eventRow(event, String(request.calendar), marker, true));
}
"#;

pub(crate) fn list_calendars() -> Result<Vec<Value>> {
    let value = run_bridge("list", None)?;
    let calendars = value
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("Apple Calendar bridge returned a non-array resource list"))?;
    Ok(calendars)
}

pub(crate) fn ensure_writable_calendar(name: &str) -> Result<()> {
    let matches: Vec<Value> = list_calendars()?
        .into_iter()
        .filter(|calendar| calendar.get("name").and_then(Value::as_str) == Some(name))
        .collect();
    if matches.len() != 1 {
        return Err(anyhow!(
            "expected exactly one Apple calendar named {name:?}; found {}",
            matches.len()
        ));
    }
    if matches[0].get("writable").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow!("Apple calendar {name:?} is read-only"));
    }
    Ok(())
}

pub(crate) fn create_event(request: &Value) -> Result<Value> {
    let result = run_bridge("create", Some(request))?;
    let external_id = result
        .get("external_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Apple Calendar bridge returned no external event id"))?
        .to_string();
    let expected_calendar = request
        .get("calendar")
        .and_then(Value::as_str)
        .unwrap_or("");
    if result.get("calendar").and_then(Value::as_str) != Some(expected_calendar) {
        return Err(anyhow!(
            "Apple Calendar bridge returned an event from the wrong calendar"
        ));
    }
    let mut normalized = result;
    normalized["external_id"] = Value::String(external_id);
    normalized["marker"] = request.get("marker").cloned().unwrap_or(Value::Null);
    Ok(normalized)
}

pub(crate) fn bridge_available() -> bool {
    std::env::var_os("HEIWA_APPLE_CALENDAR_OSASCRIPT")
        .filter(|value| !value.is_empty())
        .is_some()
        || (cfg!(target_os = "macos")
            && (PathBuf::from("/System/Applications/Calendar.app").exists()
                || PathBuf::from("/Applications/Calendar.app").exists()))
}

fn run_bridge(operation: &str, payload: Option<&Value>) -> Result<Value> {
    let executable = bridge_executable()?;
    let mut command = Command::new(&executable);
    command
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(APPLE_CALENDAR_JXA)
        .arg(operation);
    if let Some(payload) = payload {
        command.arg(payload.to_string());
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to run Apple Calendar bridge {}",
            executable.display()
        )
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow!(
            "Apple Calendar {operation} failed (Automation permission or calendar target): {detail}"
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow!("Apple Calendar bridge returned invalid JSON: {error}"))
}

fn bridge_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("HEIWA_APPLE_CALENDAR_OSASCRIPT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }
    if cfg!(target_os = "macos") {
        return Ok(PathBuf::from("/usr/bin/osascript"));
    }
    Err(anyhow!(
        "Apple Calendar connector is available only on macOS"
    ))
}
