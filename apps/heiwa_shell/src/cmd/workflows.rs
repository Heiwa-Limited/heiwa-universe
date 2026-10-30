//! Versioned, deterministic workflows over existing runtime services.
//! No model calls, shell evaluation, permission prompts, or external writes.

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::output::{self, CliError};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

fn catalog() -> Value {
    json!({
        "workflow_schema": "heiwa.workflows/v1",
        "workflows": [
            {"id":"daily-brief", "description":"Today's synced schedule, priority Mail headers, approvals, and Reminders connection status",
             "sources":["calendar/events", "mail/summary", "approvals/summary", "reminders/status"],
             "freshness":"local snapshots; no automatic native refresh"},
            {"id":"native-audit", "description":"Apple connector enrollment and implemented integration boundaries",
             "sources":["connectors/apple-calendar", "reminders/status"],
             "freshness":"enrollment only; macOS permission and native reads are not probed"},
            {"id":"runtime-check", "description":"Local app listener, installed CLI path, and pending approval count",
             "sources":["app/probe", "approvals/summary"],
             "freshness":"live TCP probe; a reachable port does not prove runtime identity or API health"}
        ],
        "policy":"read-only; no model calls, permission requests, script execution, or external writes"
    })
}

/// Keep repeated agent reads bounded, even when source strings are very long.
fn rows(value: &Value, keys: &[&str], limit: usize) -> Value {
    let source = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let items: Vec<Value> = source
        .iter()
        .take(limit)
        .map(|row| {
            let mut out = serde_json::Map::new();
            for key in keys {
                if let Some(value) = row.get(*key) {
                    let value = match value {
                        Value::String(text) => Value::String(text.chars().take(240).collect()),
                        Value::Bool(_) | Value::Number(_) | Value::Null => value.clone(),
                        _ => continue,
                    };
                    out.insert((*key).into(), value);
                }
            }
            Value::Object(out)
        })
        .collect();
    json!({"items":items, "available":source.len(), "truncated":source.len() > limit,
        "string_limit_chars":240})
}

pub(crate) fn daily_brief(limit: usize) -> Result<Value> {
    let today = chrono::Local::now().date_naive().to_string();
    let calendar = super::calendar::events_range_payload(Some(&today), Some(&today))?;
    let mail = super::mail::summary_payload();
    let approvals = super::approvals::pending_approvals_summary_payload();
    Ok(json!({
        "date":today, "timezone":calendar["timezone"],
        "calendar": {
            "events":rows(&calendar["events"], &["id","title","source","start","end","all_day"], limit),
            "sync":calendar["sync"], "source_truncated":calendar["truncated"]
        },
        "mail": {
            "snapshot":mail["snapshot"], "counts":mail["counts"],
            "priority":rows(&mail["priority"], &["id","subject","sender","date","unread","action"], limit)
        },
        "approvals": {
            "pending_count":approvals["pending_count"],
            "pending":rows(&approvals["pending"], &["id","action","target","risk"], limit)
        },
        "reminders": {
            "status":super::reminders::status()["status"],
            "content":"not read by this snapshot workflow; use heiwa reminders read --json for selected lists"
        }
    }))
}

pub(crate) fn native_audit() -> Value {
    let connection = |status: Value| {
        rows(
            &json!([status]),
            &["status", "connected_at", "next_action"],
            1,
        )["items"][0]
            .clone()
    };
    json!({"integrations":[
        {"app":"Calendar", "connection":connection(super::connectors::apple_calendar_connection_payload()),
         "implemented":"selected calendar reads and approval-staged event creation",
         "verify":"heiwa calendar sync --source apple --json"},
        {"app":"Reminders", "connection":connection(super::reminders::status()),
         "implemented":"selected-list reads, local event-to-reminder proposals, and owner-approved creation/completion through heiwa tools",
         "verify":"heiwa reminders read --json"},
        {"app":"Mail", "implemented":"inbox metadata snapshot; no body reads or sending",
         "permission":"not probed", "verify":"Read Apple Mail in Heiwa.app"},
        {"app":"Notes", "implemented":"title search, exact-ID reads, folder inventory, and approved new notes through heiwa tools", "gap":"existing-note edits, attachments, and live change notifications"},
        {"app":"Finder", "implemented":"native workspace folder selection, scoped file reads, new artifacts, and approved Finder reveal", "gap":"file moves, revisions, and sandboxed-app bookmark lifecycle"},
        {"app":"Terminal", "implemented":"CLI, approved workspace process execution in the macOS sandbox, and Terminal.app directory handoff",
         "gap":"interactive Terminal session observation and job reattachment"},
        {"app":"Shortcuts", "implemented":false, "gap":"typed inputs, declared effects, approvals, and run receipts"}
    ], "permission_policy":"enrollment does not prove current TCC permission; this workflow never requests access"})
}

fn runtime_check() -> Value {
    json!({
        "cli_path":std::env::current_exe().ok(),
        "version":env!("CARGO_PKG_VERSION"),
        "local_app":super::app::probe_local_app(super::app::DEFAULT_PORT),
        "probe_scope":"TCP reachability only; verify runtime identity and API health separately",
        "pending_approvals":super::approvals::pending_approvals_summary_payload()["pending_count"]
    })
}

pub fn run(args: &[String]) -> Result<()> {
    let json_output = output::wants_json(args);
    let mut positional = Vec::new();
    let mut limit = DEFAULT_LIMIT;
    let mut explicit_limit = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {}
            "--limit" if !explicit_limit => {
                index += 1;
                limit = args
                    .get(index)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| (1..=MAX_LIMIT).contains(v))
                    .ok_or_else(|| CliError::usage("--limit must be an integer from 1 to 50"))?;
                explicit_limit = true;
            }
            flag if flag.starts_with('-') && !["--help", "-h"].contains(&flag) => {
                bail!(CliError::usage("unknown workflows flag")
                    .with_hint("run `heiwa workflows --help`"))
            }
            value => positional.push(value),
        }
        index += 1;
    }
    let data = match positional.as_slice() {
        [] | ["list"] | ["--help"] | ["-h"] if !explicit_limit => catalog(),
        ["show", id] if !explicit_limit => catalog()["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == *id)
            .cloned()
            .ok_or_else(|| {
                CliError::not_found("unknown workflow").with_hint("run `heiwa workflows list`")
            })?,
        ["run", id] => {
            let result =
                match *id {
                    "daily-brief" => daily_brief(limit)?,
                    "native-audit" if !explicit_limit => native_audit(),
                    "runtime-check" if !explicit_limit => runtime_check(),
                    "native-audit" | "runtime-check" => {
                        bail!(CliError::usage("--limit applies only to daily-brief"))
                    }
                    _ => bail!(CliError::not_found("unknown workflow")
                        .with_hint("run `heiwa workflows list`")),
                };
            json!({"workflow_schema":"heiwa.workflows/v1", "id":id,
                "generated_at":chrono::Utc::now().to_rfc3339(), "result":result,
                "policy":"read-only; no model calls, permission requests, script execution, or external writes"})
        }
        _ => bail!(CliError::usage(
            "usage: heiwa workflows list|show <id>|run <id> [--limit 1..50] [--json]"
        )),
    };
    output::emit(json_output, data, &[], |data| {
        // The same bounded fields serve humans and agents; no second data path.
        println!(
            "{}",
            serde_json::to_string_pretty(data).expect("JSON value")
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_rows_exclude_unrequested_content_and_report_truncation() {
        let source = json!([
            {"id":"one", "subject":"é".repeat(1000), "body":"private body", "nested":{"secret":true}},
            {"id":"two", "subject":"second"}
        ]);
        let result = rows(&source, &["id", "subject", "nested"], 1);
        assert_eq!(result["available"], 2);
        assert_eq!(result["truncated"], true);
        assert_eq!(
            result["items"][0]["subject"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            240
        );
        assert!(result["items"][0].get("body").is_none());
        assert!(result["items"][0].get("nested").is_none());
    }
}
