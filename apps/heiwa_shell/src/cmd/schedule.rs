//! `heiwa schedule <text>` — free-text intent -> parsed time -> staged
//! approval + local calendar hold. The first user-intent wire into the
//! approvals/calendar spine.
//!
//! Deterministic by design: NL parsing happens in
//! `heiwa_sdk/intent/parse_time.py` (dateparser + recurrent, no LLM). When
//! the parser is unavailable or low-confidence, `--at` is the explicit
//! escape hatch; DREX escalation is a later commit. Nothing here calls a
//! provider — the only writes are the staging primitives the spine already
//! gates: a pending dispatch request and a draft local hold.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDateTime, TimeZone};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const MIN_CONFIDENCE: f64 = 0.5;
const DEFAULT_HOLD_MINUTES: i64 = 30;

pub fn run(args: &[String]) -> Result<()> {
    if args.is_empty()
        || matches!(
            args.first().map(String::as_str),
            Some("--help") | Some("-h")
        )
    {
        print_help();
        return Ok(());
    }

    let text = free_text(args);
    if text.is_empty() {
        return Err(anyhow!("usage: heiwa schedule \"<free text with a time>\" [--at YYYY-MM-DDTHH:MM] [--kind soft|focus|travel] [--dry-run] [--json]"));
    }

    let parsed = match flag_value(args, "--at") {
        Some(at) => parsed_from_at(&at)?,
        None => parse_via_sdk(&text)?,
    };

    let confidence = parsed
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if confidence < MIN_CONFIDENCE {
        return Err(anyhow!(
            "could not parse a time from {text:?} (confidence {confidence:.2}); \
             pass an explicit --at YYYY-MM-DDTHH:MM"
        ));
    }

    let kind = flag_value(args, "--kind").unwrap_or_else(|| "soft".to_string());
    let hold_request = build_hold_request(&text, &parsed, &kind)?;
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);

    if has_flag(args, "--dry-run") {
        let approval = build_approval_request(&request_id, &text, &parsed, "(dry-run)");
        let payload = json!({
            "command": "schedule",
            "dry_run": true,
            "parsed": parsed,
            "hold_request": hold_request,
            "approval_request": approval,
        });
        if has_flag(args, "--json") {
            println!("{payload}");
        } else {
            println!("schedule (dry-run)");
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        return Ok(());
    }

    let hold = crate::cmd::calendar::create_hold(&hold_request)?;
    let hold_id = hold
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string();

    let approval = build_approval_request(&request_id, &text, &parsed, &hold_id);
    let dir = requests_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{request_id}.json"));
    fs::write(&path, serde_json::to_string_pretty(&approval)?)?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "schedule",
                "dry_run": false,
                "parsed": parsed,
                "hold": hold,
                "approval_request": approval,
                "approval_path": path.display().to_string(),
            })
        );
    } else {
        let dt = parsed.get("dt").and_then(Value::as_str).unwrap_or("?");
        println!("schedule");
        println!("  text: {text}");
        println!("  parsed: {dt} (confidence {confidence:.2})");
        if let Some(rrule) = parsed.get("rrule").and_then(Value::as_str) {
            println!("  recurrence: {rrule}");
        }
        println!("  hold: {hold_id} (draft, local-only)");
        println!("  approval: {request_id} pending");
        println!("  next: heiwa approvals decide {request_id} --approve");
    }
    Ok(())
}

/// Everything that isn't a flag or a flag's value is the intent text.
pub fn free_text(args: &[String]) -> String {
    let value_flags = ["--at", "--kind"];
    let mut parts: Vec<&str> = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            skip = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        parts.push(arg);
    }
    parts.join(" ").trim().to_string()
}

/// `--at` bypasses NL parsing entirely: explicit time, confidence 1.0.
pub fn parsed_from_at(at: &str) -> Result<Value> {
    let naive = NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M"))
        .map_err(|_| anyhow!("--at must be YYYY-MM-DDTHH:MM"))?;
    let local = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("--at maps to an ambiguous or skipped local time"))?;
    Ok(json!({
        "dt": local.to_rfc3339(),
        "tz": iana_local_tz(),
        "rrule": null,
        "confidence": 1.0,
        "matched_text": at,
    }))
}

fn iana_local_tz() -> String {
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| {
            let s = p.to_string_lossy().to_string();
            s.split("zoneinfo/").nth(1).map(str::to_string)
        })
        .unwrap_or_else(|| "local".to_string())
}

/// Subprocess into the SDK's deterministic parser. Resolution order keeps
/// the installed binary working without the repo: HEIWA_PARSE_TIME /
/// HEIWA_PYTHON envs first, then the dev checkout, then PATH python3.
fn parse_via_sdk(text: &str) -> Result<Value> {
    let script = std::env::var("HEIWA_PARSE_TIME")
        .map(PathBuf::from)
        .ok()
        .filter(|p| p.exists())
        .or_else(|| {
            let dev = crate::home::heiwa_home()?
                .join("heiwa-universe/packages/heiwa_sdk/heiwa_sdk/intent/parse_time.py");
            dev.exists().then_some(dev)
        })
        .ok_or_else(|| {
            anyhow!("parse_time.py not found; set HEIWA_PARSE_TIME or pass --at YYYY-MM-DDTHH:MM")
        })?;
    let python = std::env::var("HEIWA_PYTHON")
        .map(PathBuf::from)
        .ok()
        .filter(|p| p.exists())
        .or_else(|| {
            let venv = crate::home::heiwa_home()?.join("heiwa-universe/.venv/bin/python");
            venv.exists().then_some(venv)
        })
        .unwrap_or_else(|| PathBuf::from("python3"));

    let output = Command::new(&python)
        .arg(&script)
        .arg("--text")
        .arg(text)
        .output()
        .map_err(|e| anyhow!("failed to run {}: {e}", python.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "parse_time failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("parse_time emitted invalid JSON: {e}"))?;
    Ok(parsed)
}

/// Shape consumed by `calendar::create_hold` — same lane the cockpit POST
/// uses, so schedule-created holds carry identical evidence.
pub fn build_hold_request(text: &str, parsed: &Value, kind: &str) -> Result<Value> {
    let dt_raw = parsed
        .get("dt")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("parsed time has no concrete occurrence"))?;
    let dt: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(dt_raw)
        .map_err(|e| anyhow!("bad parsed datetime {dt_raw:?}: {e}"))?;
    let end = dt + Duration::minutes(DEFAULT_HOLD_MINUTES);
    let note = parsed
        .get("rrule")
        .and_then(Value::as_str)
        .map(|r| format!("recurring: {r}"));
    Ok(json!({
        "title": text,
        "date": dt.format("%Y-%m-%d").to_string(),
        "start": dt.format("%H:%M").to_string(),
        "end": end.format("%H:%M").to_string(),
        "kind": kind,
        "note": note,
        "source": "schedule",
    }))
}

/// Shape consumed by `approvals list/show/decide` (operator_dispatch_request_v1).
pub fn build_approval_request(
    request_id: &str,
    text: &str,
    parsed: &Value,
    hold_id: &str,
) -> Value {
    json!({
        "schema_version": "operator_dispatch_request_v1",
        "request_id": request_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "action": "schedule-intent",
        "target_surface": "calendar",
        "target_scope": hold_id,
        "requested_mode": "stage",
        "intent": {
            "text": text,
            "parsed": parsed,
            "hold_id": hold_id,
        },
        "source": "heiwa schedule",
    })
}

fn requests_dir() -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("dispatch")
        .join("requests")
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let idx = args.iter().position(|arg| arg == flag)?;
    args.get(idx + 1).cloned()
}

fn print_help() {
    println!("heiwa schedule");
    println!();
    println!("Usage:");
    println!("  heiwa schedule \"remind me Friday 3pm to call mom\" [--json] [--dry-run]");
    println!("  heiwa schedule \"call mom\" --at 2026-06-19T15:00 [--kind soft|focus|travel]");
    println!();
    println!("Parses the time deterministically (no LLM), writes a draft local");
    println!("calendar hold and a pending approval. Nothing leaves the machine;");
    println!("external promotion stays behind `heiwa approvals decide`.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn free_text_strips_flags_and_their_values() {
        let argv = args(&[
            "call",
            "mom",
            "--at",
            "2026-06-19T15:00",
            "--json",
            "--kind",
            "soft",
        ]);
        assert_eq!(free_text(&argv), "call mom");
    }

    #[test]
    fn parsed_from_at_is_full_confidence_and_tz_aware() {
        let parsed = parsed_from_at("2026-06-19T15:00").unwrap();
        assert_eq!(parsed["confidence"], 1.0);
        let dt = parsed["dt"].as_str().unwrap();
        assert!(dt.starts_with("2026-06-19T15:00:00"));
        assert!(dt.contains('+') || dt.contains('-'));
    }

    #[test]
    fn parsed_from_at_rejects_bad_format() {
        assert!(parsed_from_at("friday 3pm").is_err());
        assert!(parsed_from_at("2026-06-19").is_err());
    }

    #[test]
    fn hold_request_shape_matches_calendar_lane() {
        let parsed = json!({"dt": "2026-06-19T15:00:00-07:00", "tz": "America/Vancouver", "rrule": null, "confidence": 0.9});
        let hold = build_hold_request("call mom", &parsed, "soft").unwrap();
        assert_eq!(hold["title"], "call mom");
        assert_eq!(hold["date"], "2026-06-19");
        assert_eq!(hold["start"], "15:00");
        assert_eq!(hold["end"], "15:30");
        assert_eq!(hold["kind"], "soft");
        assert_eq!(hold["source"], "schedule");
    }

    #[test]
    fn hold_request_carries_rrule_in_note() {
        let parsed = json!({"dt": "2026-06-15T09:00:00-07:00", "rrule": "RRULE:FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=0", "confidence": 0.85});
        let hold = build_hold_request("check pipeline", &parsed, "soft").unwrap();
        assert!(hold["note"].as_str().unwrap().contains("FREQ=WEEKLY"));
    }

    #[test]
    fn approval_request_matches_dispatch_v1() {
        let parsed = json!({"dt": "2026-06-19T15:00:00-07:00", "confidence": 0.9});
        let req = build_approval_request("req_abc123", "call mom", &parsed, "hold-20260619-aaaa");
        assert_eq!(req["schema_version"], "operator_dispatch_request_v1");
        assert_eq!(req["request_id"], "req_abc123");
        assert_eq!(req["action"], "schedule-intent");
        assert_eq!(req["target_surface"], "calendar");
        assert_eq!(req["target_scope"], "hold-20260619-aaaa");
        assert_eq!(req["requested_mode"], "stage");
        assert_eq!(req["intent"]["hold_id"], "hold-20260619-aaaa");
        // The summary the approvals list view renders must resolve cleanly.
        let summary = crate::cmd::approvals::approval_request_summary(&req);
        assert_eq!(summary["id"], "req_abc123");
        assert_eq!(summary["action"], "schedule-intent");
        assert_eq!(summary["risk"], "stage");
    }

    #[test]
    fn build_hold_request_requires_concrete_dt() {
        let parsed = json!({"dt": null, "rrule": null, "confidence": 0.0});
        assert!(build_hold_request("x", &parsed, "soft").is_err());
    }
}
