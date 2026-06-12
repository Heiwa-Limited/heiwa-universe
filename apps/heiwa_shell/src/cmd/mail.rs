use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

const POLICY: &str = "metadata-only-no-body";

/// Max header lines scanned from the metadata snapshot per summary build.
const HEADER_SCAN_LIMIT: usize = 200;
/// Max priority rows surfaced to read models.
const PRIORITY_LIMIT: usize = 10;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => status(args),
        Some("accounts") => accounts(args),
        Some("summary") => {
            println!("{}", summary_payload());
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown mail command: {other}")),
    }
}

fn status(args: &[String]) -> Result<()> {
    let probe = MailProbe::detect();
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "mail status",
                "policy": POLICY,
                "mail_app_path": probe.app_path.display().to_string(),
                "mail_app_present": probe.app_present,
                "mail_data_dir": probe.data_dir.display().to_string(),
                "mail_data_present": probe.data_present,
                "fields": probe.allowed_fields,
                "bridge_state": probe.bridge_state,
                "next": probe.next,
            })
        );
        return Ok(());
    }
    println!("mail status");
    println!("  policy: {POLICY}");
    println!("  app: {}", probe.app_path.display());
    println!("  app_present: {}", probe.app_present);
    println!("  data_dir: {}", probe.data_dir.display());
    println!("  data_present: {}", probe.data_present);
    println!("  bridge: {}", probe.bridge_state);
    println!("  fields: {}", probe.allowed_fields.join(", "));
    println!("  next: {}", probe.next);
    Ok(())
}

fn accounts(args: &[String]) -> Result<()> {
    let probe = MailProbe::detect();
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "mail accounts",
                "policy": POLICY,
                "mail_data_dir": probe.data_dir.display().to_string(),
                "accounts": probe.accounts,
                "note": "enumeration is metadata-only; no body or recipient bodies are read",
            })
        );
        return Ok(());
    }
    println!("mail accounts");
    println!("  policy: {POLICY}");
    if probe.accounts.is_empty() {
        println!("  (no accounts detected — Mail.app not configured or no Mail data dir)");
        println!("  data dir: {}", probe.data_dir.display());
    } else {
        for account in &probe.accounts {
            println!("  - {account}");
        }
    }
    Ok(())
}

struct MailProbe {
    app_path: PathBuf,
    app_present: bool,
    data_dir: PathBuf,
    data_present: bool,
    allowed_fields: Vec<&'static str>,
    bridge_state: &'static str,
    next: &'static str,
    accounts: Vec<String>,
}

impl MailProbe {
    fn detect() -> Self {
        let app_path = PathBuf::from("/System/Applications/Mail.app");
        let app_present = app_path.exists() || PathBuf::from("/Applications/Mail.app").exists();
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Mail");
        let data_present = data_dir.exists();
        let accounts = if data_present {
            enumerate_account_labels(&data_dir)
        } else {
            Vec::new()
        };
        Self {
            app_path,
            app_present,
            data_dir,
            data_present,
            allowed_fields: vec!["account", "mailbox", "sender", "subject", "date", "unread"],
            bridge_state: "metadata-only-probe",
            next:
                "wire AppleScript-driven metadata snapshot into ~/.heiwa/state/mail/headers.jsonl",
            accounts,
        }
    }
}

fn enumerate_account_labels(data_dir: &PathBuf) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        // Mail.app account directories typically look like Vx -> Account.mbox or IMAP-...
        if name.starts_with('V') && name.len() <= 4 {
            if let Ok(inner) = std::fs::read_dir(&path) {
                for inner_entry in inner.flatten() {
                    let inner_name = inner_entry.file_name().to_string_lossy().to_string();
                    if !inner_name.is_empty() {
                        out.push(format!("{name}/{inner_name}"));
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Read model
// ---------------------------------------------------------------------------

pub(crate) fn mail_data_present() -> bool {
    MailProbe::detect().data_present
}

fn headers_snapshot_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".heiwa")
        .join("state")
        .join("mail")
        .join("headers.jsonl")
}

/// Load the most recent metadata rows from the local snapshot, newest last.
fn load_header_rows() -> Vec<Value> {
    let Ok(raw) = std::fs::read_to_string(headers_snapshot_path()) else {
        return Vec::new();
    };
    let lines: Vec<&str> = raw.lines().filter(|line| !line.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(HEADER_SCAN_LIMIT);
    lines[start..]
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

/// Deterministic priority scoring over metadata only. No model call, no body
/// read — this is the cheap always-on lane that runs before any LLM triage.
fn priority_score(row: &Value, today: chrono::NaiveDate) -> i64 {
    let mut score = 0;
    if row.get("unread").and_then(Value::as_bool).unwrap_or(false) {
        score += 2;
    }
    if let Some(date) = row.get("date").and_then(Value::as_str).and_then(|raw| {
        chrono::NaiveDate::parse_from_str(&raw[..raw.len().min(10)], "%Y-%m-%d").ok()
    }) {
        let age = (today - date).num_days();
        if age <= 2 {
            score += 2;
        } else if age <= 7 {
            score += 1;
        }
    }
    let subject = row
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    const URGENT_HINTS: [&str; 7] = [
        "urgent",
        "action required",
        "invoice",
        "reschedule",
        "deadline",
        "overdue",
        "confirm",
    ];
    if URGENT_HINTS.iter().any(|hint| subject.contains(hint)) {
        score += 1;
    }
    let sender = row
        .get("sender")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    const BULK_HINTS: [&str; 4] = ["noreply", "no-reply", "newsletter", "notifications@"];
    if BULK_HINTS.iter().any(|hint| sender.contains(hint)) {
        score -= 2;
    }
    score
}

fn action_for_score(score: i64) -> &'static str {
    if score >= 3 {
        "draft"
    } else if score >= 1 {
        "report"
    } else {
        "digest"
    }
}

pub(crate) fn priority_rows() -> Vec<Value> {
    let today = chrono::Local::now().date_naive();
    let mut scored: Vec<(i64, Value)> = load_header_rows()
        .into_iter()
        .map(|row| (priority_score(&row, today), row))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(PRIORITY_LIMIT)
        .map(|(score, row)| {
            json!({
                "account": row.get("account"),
                "mailbox": row.get("mailbox"),
                "sender": row.get("sender"),
                "subject": row.get("subject"),
                "date": row.get("date"),
                "unread": row.get("unread"),
                "score": score,
                "action": action_for_score(score),
            })
        })
        .collect()
}

/// The mail summary read model served at /api/v1/mail/summary.
pub(crate) fn summary_payload() -> Value {
    let probe = MailProbe::detect();
    let snapshot_path = headers_snapshot_path();
    let snapshot_present = snapshot_path.exists();
    let rows = priority_rows();
    let unread = rows
        .iter()
        .filter(|row| row.get("unread").and_then(Value::as_bool).unwrap_or(false))
        .count();

    let gmail_status = crate::cmd::connectors::google_lane_status("gmail");
    let lanes = json!([
        {
            "id": "gmail",
            "name": "Gmail",
            "status": gmail_status,
            "read": "OAuth read-only scope; local scheduled scan (no hosted webhook runtime).",
            "reply": "Draft replies in-app; send through Gmail API only after approval.",
            "guardrail": "Send scope stays separate from read scope; every outbound send records a receipt.",
        },
        {
            "id": "apple_mail",
            "name": "Apple Mail",
            "status": if probe.data_present { "metadata" } else { "planned" },
            "read": "Metadata snapshot only (account, mailbox, sender, subject, date, unread).",
            "reply": "Stage local drafts and hand off to Mail.app until a MailKit bridge exists.",
            "guardrail": "Body reads require explicit connector permission; none granted today.",
        },
        {
            "id": "imap",
            "name": "IMAP / Himalaya",
            "status": if crate::cmd::connectors::imap_configured() { "staged" } else { "planned" },
            "read": "Portable fallback for user-owned IMAP accounts.",
            "reply": "Template replies through SMTP after approval.",
            "guardrail": "Account-scoped leases; never blind-retry sends after partial failures.",
        },
    ]);

    json!({
        "command": "mail summary",
        "policy": POLICY,
        "lanes": lanes,
        "accounts": probe.accounts,
        "snapshot": {
            "path": snapshot_path.display().to_string(),
            "present": snapshot_present,
            "scanned": rows.len(),
        },
        "priority": rows,
        "counts": {
            "priority": rows.len(),
            "unread_in_priority": unread,
            "accounts": probe.accounts.len(),
        },
    })
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn print_help() {
    println!("heiwa mail");
    println!();
    println!("Usage:");
    println!("  heiwa mail status [--json]");
    println!("  heiwa mail accounts [--json]");
    println!("  heiwa mail summary");
    println!();
    println!("Policy: {POLICY}.");
    println!("Reads Mail.app data dir layout only; does not parse message bodies.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn unread_recent_urgent_scores_high() {
        let row = json!({
            "sender": "vendor@example.com",
            "subject": "Invoice overdue — action required",
            "date": "2026-06-11",
            "unread": true,
        });
        let score = priority_score(&row, day("2026-06-12"));
        assert!(score >= 3, "expected draft-tier score, got {score}");
        assert_eq!(action_for_score(score), "draft");
    }

    #[test]
    fn bulk_sender_lands_in_digest() {
        let row = json!({
            "sender": "newsletter@example.com",
            "subject": "Weekly update",
            "date": "2026-06-11",
            "unread": false,
        });
        let score = priority_score(&row, day("2026-06-12"));
        assert_eq!(action_for_score(score), "digest");
    }

    #[test]
    fn stale_read_mail_is_digest() {
        let row = json!({
            "sender": "person@example.com",
            "subject": "hello",
            "date": "2026-05-01",
            "unread": false,
        });
        let score = priority_score(&row, day("2026-06-12"));
        assert_eq!(action_for_score(score), "digest");
    }
}
