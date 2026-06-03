use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("list") | Some("status") | None => list(args),
        Some("show") => show(&args[1..]),
        Some("decide") => decide(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown approvals command: {other}")),
    }
}

fn list(args: &[String]) -> Result<()> {
    let pending = scan_pending_requests();
    let decisions = scan_decisions();
    let pending_summary: Vec<Value> = pending.iter().map(approval_request_summary).collect();
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "approvals list",
                "requests_dir": requests_dir().display().to_string(),
                "decisions_dir": decisions_dir().display().to_string(),
                "pending": pending,
                "pending_summary": pending_summary,
                "decided": decisions,
            })
        );
        return Ok(());
    }
    println!("approvals");
    println!("  requests: {} pending", pending.len());
    println!("  decisions: {} on record", decisions.len());
    println!("  requests dir: {}", requests_dir().display());
    println!("  decisions dir: {}", decisions_dir().display());
    for req in pending.iter().take(10) {
        let summary = approval_request_summary(req);
        let id = summary.get("id").and_then(Value::as_str).unwrap_or("?");
        let action = summary.get("action").and_then(Value::as_str).unwrap_or("?");
        let target = summary.get("target").and_then(Value::as_str).unwrap_or("?");
        let risk = summary.get("risk").and_then(Value::as_str).unwrap_or("?");
        println!("    {id}  {action} -> {target}  risk={risk}");
    }
    if pending.len() > 10 {
        println!("    ... {} more", pending.len() - 10);
    }
    Ok(())
}

fn show(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .ok_or_else(|| anyhow!("usage: heiwa approvals show <id>"))?;
    let path = requests_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(anyhow!("approval not found: {}", path.display()));
    }
    let raw = fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("approval {id}");
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn decide(args: &[String]) -> Result<()> {
    let id = args.first().ok_or_else(|| {
        anyhow!("usage: heiwa approvals decide <id> --approve|--deny [--note ...]")
    })?;
    let approve = has_flag(args, "--approve");
    let deny = has_flag(args, "--deny");
    if approve == deny {
        return Err(anyhow!("must pass exactly one of --approve or --deny"));
    }
    let dry_run = has_flag(args, "--dry-run");
    let decision = json!({
        "id": id,
        "outcome": if approve { "approved" } else { "denied" },
        "decided_at_utc": Utc::now().to_rfc3339(),
        "operator": "local-cli",
        "note": flag_value(args, "--note"),
    });
    let path = decisions_dir().join(format!("{id}.json"));
    if dry_run {
        if has_flag(args, "--json") {
            println!(
                "{}",
                json!({
                    "command": "approvals decide",
                    "dry_run": true,
                    "path": path.display().to_string(),
                    "decision": decision,
                })
            );
        } else {
            println!("approvals decide (dry-run)");
            println!("  id: {id}");
            println!("  outcome: {}", decision["outcome"]);
            println!("  would write: {}", path.display());
        }
        return Ok(());
    }
    fs::create_dir_all(decisions_dir())?;
    fs::write(&path, serde_json::to_string_pretty(&decision)?)?;
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "approvals decide",
                "dry_run": false,
                "path": path.display().to_string(),
                "decision": decision,
            })
        );
    } else {
        println!("approvals decide");
        println!("  id: {id}");
        println!("  outcome: {}", decision["outcome"]);
        println!("  wrote: {}", path.display());
    }
    Ok(())
}

pub(crate) fn scan_pending_requests() -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(requests_dir()) else {
        return out;
    };
    let decided: std::collections::HashSet<String> = scan_decisions()
        .into_iter()
        .filter_map(|d| d.get("id").and_then(Value::as_str).map(|s| s.to_string()))
        .collect();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if decided.contains(&stem) {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let mut value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({"raw": raw}));
        if value.get("id").is_none() {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("id".to_string(), Value::String(stem));
            }
        }
        out.push(value);
    }
    out
}

pub(crate) fn approval_request_summary(req: &Value) -> Value {
    let id = string_field(req, &["id", "request_id"]).unwrap_or_else(|| "?".to_string());
    let action = string_field(req, &["action"]).unwrap_or_else(|| "?".to_string());
    let target = string_field(req, &["target"]).unwrap_or_else(|| {
        match (
            string_field(req, &["target_surface"]),
            string_field(req, &["target_scope"]),
        ) {
            (Some(surface), Some(scope)) => format!("{surface}:{scope}"),
            (Some(surface), None) => surface,
            (None, Some(scope)) => scope,
            (None, None) => "?".to_string(),
        }
    });
    let risk = string_field(req, &["risk", "requested_mode"]).unwrap_or_else(|| "?".to_string());
    let requested_at = string_field(req, &["requested_at", "requested_at_utc", "created_at"]);

    json!({
        "id": id,
        "action": action,
        "target": target,
        "risk": risk,
        "requested_at": requested_at,
    })
}

pub(crate) fn pending_approvals_summary_payload() -> Value {
    let pending = scan_pending_requests();
    let pending_summary: Vec<Value> = pending.iter().map(approval_request_summary).collect();
    json!({
        "pending_count": pending_summary.len(),
        "pending": pending_summary,
        "requests_dir": requests_dir().display().to_string(),
        "decisions_dir": decisions_dir().display().to_string(),
    })
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        value
            .get(*field)
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn scan_decisions() -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(decisions_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            out.push(value);
        }
    }
    out
}

fn dispatch_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".heiwa").join("state").join("dispatch")
}

fn requests_dir() -> PathBuf {
    dispatch_dir().join("requests")
}

fn decisions_dir() -> PathBuf {
    dispatch_dir().join("approvals").join("decisions")
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
    }
    None
}

fn print_help() {
    println!("heiwa approvals");
    println!();
    println!("Usage:");
    println!("  heiwa approvals list [--json]");
    println!("  heiwa approvals show <id> [--json]");
    println!("  heiwa approvals decide <id> --approve|--deny [--note ...] [--dry-run] [--json]");
    println!();
    println!("Reads from ~/.heiwa/state/dispatch/requests/ and writes decisions to");
    println!("~/.heiwa/state/dispatch/approvals/decisions/.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_summary_maps_dispatch_v1_schema() {
        let request = json!({
            "schema_version": "operator_dispatch_request_v1",
            "request_id": "req_123",
            "created_at": "2026-03-30T18:20:22.112520Z",
            "action": "write-file",
            "target_surface": "filesystem",
            "target_scope": "/Users/dmcgregsauce/.gemini/settings.json",
            "requested_mode": "write"
        });

        let summary = approval_request_summary(&request);

        assert_eq!(summary["id"], "req_123");
        assert_eq!(summary["action"], "write-file");
        assert_eq!(
            summary["target"],
            "filesystem:/Users/dmcgregsauce/.gemini/settings.json"
        );
        assert_eq!(summary["risk"], "write");
        assert_eq!(summary["requested_at"], "2026-03-30T18:20:22.112520Z");
    }
}
