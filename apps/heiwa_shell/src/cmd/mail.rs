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
        Some("scan") => scan(&args[1..]),
        Some("triage") => triage(&args[1..]),
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

/// True only when Apple Mail actually has configured accounts — a bare
/// ~/Library/Mail directory does not make the metadata lane usable.
pub(crate) fn apple_mail_accounts_present() -> bool {
    !MailProbe::detect().accounts.is_empty()
}

// ---------------------------------------------------------------------------
// Snapshot scan (the headers.jsonl producer)
// ---------------------------------------------------------------------------

/// Snapshot stays bounded so the read model never pays for mailbox history.
const MAX_SNAPSHOT_LINES: usize = 1000;
/// Default messages pulled per source per scan.
const DEFAULT_SCAN_LIMIT: usize = 25;

/// JXA program: metadata-only pull from Mail.app inboxes. Emits one JSON row
/// per message on stdout. Bodies are never touched — policy is enforced by
/// only ever reading sender/subject/date/readStatus.
const APPLE_MAIL_JXA: &str = r#"
function run(argv) {
  const limit = parseInt(argv[0] || "25", 10);
  const Mail = Application("Mail");
  const rows = [];
  let accounts = [];
  try { accounts = Mail.accounts(); } catch (e) { return ""; }
  for (let a = 0; a < accounts.length && rows.length < limit; a++) {
    let accountName = "";
    let mailboxes = [];
    try { accountName = accounts[a].name(); } catch (e) { continue; }
    try { mailboxes = accounts[a].mailboxes(); } catch (e) { continue; }
    for (let m = 0; m < mailboxes.length && rows.length < limit; m++) {
      let boxName = "";
      try { boxName = mailboxes[m].name(); } catch (e) { continue; }
      if (!/^(INBOX|Inbox)$/.test(boxName)) continue;
      let count = 0;
      try { count = mailboxes[m].messages.length; } catch (e) { continue; }
      const take = Math.min(limit, count);
      for (let i = 0; i < take && rows.length < limit; i++) {
        try {
          const msg = mailboxes[m].messages[i];
          rows.push(JSON.stringify({
            account: accountName,
            mailbox: boxName,
            sender: String(msg.sender()),
            subject: String(msg.subject()),
            date: msg.dateReceived().toISOString(),
            unread: !msg.readStatus(),
          }));
        } catch (e) {}
      }
    }
  }
  return rows.join("\n");
}
"#;

fn scan(args: &[String]) -> Result<()> {
    let source = flag_value(args, "--source").unwrap_or_else(|| "all".to_string());
    let limit = flag_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_SCAN_LIMIT)
        .clamp(1, 200);
    let dry_run = has_flag(args, "--dry-run");
    let json_output = has_flag(args, "--json");

    let apple_ready = apple_mail_accounts_present();
    let gmail_token = crate::cmd::connectors::connector_token_path("gmail").exists();

    if dry_run {
        let payload = json!({
            "command": "mail scan",
            "dry_run": true,
            "policy": POLICY,
            "limit": limit,
            "snapshot": headers_snapshot_path().display().to_string(),
            "sources": {
                "apple": {
                    "selected": source == "all" || source == "apple",
                    "ready": apple_ready,
                    "blocker": if apple_ready { Value::Null } else {
                        Value::String("no Apple Mail accounts configured".into())
                    },
                },
                "gmail": {
                    "selected": source == "all" || source == "gmail",
                    "ready": gmail_token,
                    "blocker": if gmail_token { Value::Null } else {
                        Value::String("no token; run: heiwa connect gmail --client-secret <path> then --authorize".into())
                    },
                },
            },
        });
        println!("{payload}");
        return Ok(());
    }

    let mut fetched: Vec<Value> = Vec::new();
    let mut source_reports: Vec<Value> = Vec::new();

    if source == "all" || source == "apple" {
        if apple_ready {
            match scan_apple_mail(limit) {
                Ok(rows) => {
                    source_reports.push(json!({
                        "source": "apple", "status": "scanned", "fetched": rows.len(),
                    }));
                    fetched.extend(rows);
                }
                Err(error) => {
                    source_reports.push(json!({
                        "source": "apple", "status": "error", "error": error.to_string(),
                    }));
                }
            }
        } else {
            source_reports.push(json!({
                "source": "apple", "status": "skipped",
                "reason": "no Apple Mail accounts configured",
            }));
        }
    }

    if source == "all" || source == "gmail" {
        if gmail_token {
            match scan_gmail(limit) {
                Ok(rows) => {
                    source_reports.push(json!({
                        "source": "gmail", "status": "scanned", "fetched": rows.len(),
                    }));
                    fetched.extend(rows);
                }
                Err(error) => {
                    source_reports.push(json!({
                        "source": "gmail", "status": "error", "error": error.to_string(),
                    }));
                }
            }
        } else {
            source_reports.push(json!({
                "source": "gmail", "status": "needs_auth",
                "next": [
                    "heiwa connect gmail --client-secret <path>",
                    "heiwa connect gmail --authorize",
                ],
            }));
        }
    }

    let appended = append_snapshot_rows(&fetched)?;
    let receipt = write_scan_receipt(&source_reports, fetched.len(), appended)?;

    let payload = json!({
        "command": "mail scan",
        "policy": POLICY,
        "sources": source_reports,
        "fetched": fetched.len(),
        "appended": appended,
        "deduplicated": fetched.len().saturating_sub(appended),
        "snapshot": headers_snapshot_path().display().to_string(),
        "receipt": receipt,
    });
    if json_output {
        println!("{payload}");
    } else {
        println!("mail scan");
        println!("  fetched: {}", fetched.len());
        println!("  appended: {appended}");
        for report in payload["sources"].as_array().unwrap_or(&Vec::new()) {
            let source = report.get("source").and_then(Value::as_str).unwrap_or("?");
            let status = report.get("status").and_then(Value::as_str).unwrap_or("?");
            println!("  {source}: {status}");
        }
        println!("  snapshot: {}", headers_snapshot_path().display());
    }
    Ok(())
}

/// Pull inbox metadata from Mail.app via JXA (osascript -l JavaScript).
fn scan_apple_mail(limit: usize) -> Result<Vec<Value>> {
    use anyhow::Context;
    let output = std::process::Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(APPLE_MAIL_JXA)
        .arg(limit.to_string())
        .output()
        .context("failed to run osascript for Apple Mail scan")?;
    if !output.status.success() {
        return Err(anyhow!(
            "Apple Mail scan failed (Automation permission?): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .collect())
}

fn gmail_api_base() -> String {
    std::env::var("HEIWA_GMAIL_API_BASE")
        .unwrap_or_else(|_| "https://gmail.googleapis.com/gmail/v1".to_string())
}

/// Pull INBOX metadata via the Gmail API using the stored read-only token.
/// Refreshes the access token once on 401 and persists the rotation.
fn scan_gmail(limit: usize) -> Result<Vec<Value>> {
    let mut access_token = gmail_access_token()?;
    let base = gmail_api_base();

    let list_url = format!("{base}/users/me/messages?maxResults={limit}&labelIds=INBOX");
    let mut listing = gmail_get(&list_url, &access_token)?;
    if listing.get("error").is_some() {
        // One refresh attempt, then re-list.
        access_token = refresh_gmail_access_token()?;
        listing = gmail_get(&list_url, &access_token)?;
        if let Some(error) = listing.get("error") {
            return Err(anyhow!("gmail list failed after refresh: {error}"));
        }
    }

    let account = gmail_profile_address(&access_token).unwrap_or_else(|| "gmail".to_string());
    let ids: Vec<String> = listing
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut rows = Vec::new();
    for id in ids {
        let url = format!(
            "{base}/users/me/messages/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject"
        );
        let Ok(message) = gmail_get(&url, &access_token) else {
            continue;
        };
        if let Some(row) = gmail_message_to_row(&message, &account) {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Normalize one Gmail metadata response into the snapshot row shape.
fn gmail_message_to_row(message: &Value, account: &str) -> Option<Value> {
    let headers = message
        .get("payload")
        .and_then(|payload| payload.get("headers"))
        .and_then(Value::as_array)?;
    let header = |name: &str| -> Option<String> {
        headers.iter().find_map(|h| {
            (h.get("name").and_then(Value::as_str)? == name)
                .then(|| h.get("value").and_then(Value::as_str).map(str::to_string))?
        })
    };
    let label_ids = message
        .get("labelIds")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let date = message
        .get("internalDate")
        .and_then(Value::as_str)
        .and_then(|ms| ms.parse::<i64>().ok())
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    Some(json!({
        "account": account,
        "mailbox": "INBOX",
        "sender": header("From").unwrap_or_default(),
        "subject": header("Subject").unwrap_or_default(),
        "date": date,
        "unread": label_ids.iter().any(|label| label == "UNREAD"),
    }))
}

fn gmail_get(url: &str, access_token: &str) -> Result<Value> {
    use anyhow::Context;
    let auth_header = format!("{} {}", "Authorization: Bearer", access_token);
    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("-H")
        .arg(auth_header)
        .arg(url)
        .output()
        .context("failed to run curl for gmail request")?;
    if !output.status.success() {
        return Err(anyhow!(
            "gmail request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| anyhow!("gmail returned non-JSON: {e}"))
}

fn gmail_access_token() -> Result<String> {
    let path = crate::cmd::connectors::connector_token_path("gmail");
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow!("no gmail token; run: heiwa connect gmail --authorize"))?;
    let stored: Value = serde_json::from_str(&raw)?;
    stored
        .get("token")
        .and_then(|token| token.get("access_token"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("gmail token file missing access_token"))
}

fn refresh_gmail_access_token() -> Result<String> {
    let path = crate::cmd::connectors::connector_token_path("gmail");
    let raw = std::fs::read_to_string(&path)?;
    let mut stored: Value = serde_json::from_str(&raw)?;
    let refresh_token = stored
        .get("token")
        .and_then(|token| token.get("refresh_token"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!("gmail token has no refresh_token; re-run: heiwa connect gmail --authorize")
        })?;

    let refreshed_raw = crate::cmd::connectors::refresh_google_token(&refresh_token)?;
    let refreshed: Value = serde_json::from_str(&refreshed_raw)?;
    let access_token = refreshed
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("token refresh failed: {refreshed_raw}"))?;

    // Persist the rotated access token; refresh_token is retained.
    if let Some(token) = stored.get_mut("token") {
        if let Some(obj) = token.as_object_mut() {
            obj.insert("access_token".to_string(), json!(access_token));
            obj.insert(
                "rotated_at".to_string(),
                json!(chrono::Utc::now().to_rfc3339()),
            );
        }
    }
    crate::cmd::connectors::store_connector_token("gmail", &stored)?;
    Ok(access_token)
}

fn gmail_profile_address(access_token: &str) -> Option<String> {
    let url = format!("{}/users/me/profile", gmail_api_base());
    gmail_get(&url, access_token)
        .ok()?
        .get("emailAddress")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Stable identity for a snapshot row so repeated scans never duplicate.
fn snapshot_dedupe_key(row: &Value) -> String {
    let field = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
    format!(
        "{}|{}|{}|{}",
        field("account"),
        field("sender"),
        field("subject"),
        field("date"),
    )
}

/// Append unique rows to headers.jsonl, keeping the file bounded.
/// Returns how many rows were actually appended.
fn append_snapshot_rows(rows: &[Value]) -> Result<usize> {
    append_snapshot_rows_at(&headers_snapshot_path(), rows)
}

fn append_snapshot_rows_at(path: &std::path::Path, rows: &[Value]) -> Result<usize> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing_raw = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = existing_raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    let mut seen: std::collections::HashSet<String> = lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|row| snapshot_dedupe_key(&row))
        .collect();

    let scanned_at = chrono::Utc::now().to_rfc3339();
    let mut appended = 0;
    for row in rows {
        let key = snapshot_dedupe_key(row);
        if seen.contains(&key) {
            continue;
        }
        let mut stamped = row.clone();
        if let Some(obj) = stamped.as_object_mut() {
            obj.insert("scanned_at".to_string(), json!(scanned_at));
        }
        lines.push(stamped.to_string());
        seen.insert(key);
        appended += 1;
    }

    if lines.len() > MAX_SNAPSHOT_LINES {
        let drop = lines.len() - MAX_SNAPSHOT_LINES;
        lines.drain(..drop);
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(appended)
}

fn write_scan_receipt(sources: &[Value], fetched: usize, appended: usize) -> Result<String> {
    let receipts_dir = headers_snapshot_path()
        .parent()
        .map(|parent| parent.join("receipts"))
        .ok_or_else(|| anyhow!("no mail state parent dir"))?;
    std::fs::create_dir_all(&receipts_dir)?;
    let receipt_id = format!("scan-{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    let receipt = json!({
        "receipt_id": receipt_id,
        "kind": "mail_metadata_scan",
        "policy": POLICY,
        "sources": sources,
        "fetched": fetched,
        "appended": appended,
        "snapshot": headers_snapshot_path().display().to_string(),
        "scanned_at": chrono::Utc::now().to_rfc3339(),
    });
    let path = receipts_dir.join(format!("{receipt_id}.json"));
    std::fs::write(&path, receipt.to_string())?;
    Ok(receipt_id)
}

// ---------------------------------------------------------------------------
// Triage: summaries + suggested actions, reply drafts staged behind approvals
// ---------------------------------------------------------------------------

/// `heiwa mail triage` — turn the priority read model into suggestions the
/// user can act on. Draft-tier rows get a suggested reply (local Ollama,
/// deterministic template fallback) staged as a pending approval in the same
/// dispatch lane the schedule command uses. Delete/archive is suggested but
/// never staged: no write bridge exists (Gmail scope is read-only, Apple
/// bridge is metadata-only), and pretending otherwise would be theater.
fn triage(args: &[String]) -> Result<()> {
    let dry_run = has_flag(args, "--dry-run");
    let json_output = has_flag(args, "--json");
    let no_draft = has_flag(args, "--no-draft");
    let limit = flag_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(PRIORITY_LIMIT)
        .clamp(1, 50);

    let rows = priority_rows();
    if rows.is_empty() {
        println!("mail triage: snapshot is empty — run `heiwa mail scan` first");
        return Ok(());
    }

    let mut items: Vec<Value> = Vec::new();
    let mut staged = 0usize;
    let mut skipped_existing = 0usize;

    for row in rows.iter().take(limit) {
        let action = row.get("action").and_then(Value::as_str).unwrap_or("digest");
        let suggestion = suggested_action(row);
        let mut item = json!({
            "message": row,
            "summary": message_summary(row),
            "suggestion": suggestion,
        });

        if action == "draft" {
            let request_id = triage_request_id(row);
            let already = crate::cmd::approvals::requests_dir().join(format!("{request_id}.json")).exists()
                || crate::cmd::approvals::decisions_dir().join(format!("{request_id}.json")).exists();
            if already {
                skipped_existing += 1;
                item["staged"] = json!({"request_id": request_id, "status": "already_staged"});
            } else {
                let (draft, draft_source) = if no_draft {
                    (template_draft(row), "template".to_string())
                } else {
                    generate_draft(row)
                };
                let request = build_triage_request(&request_id, row, &draft, &draft_source);
                if dry_run {
                    item["staged"] = json!({
                        "request_id": request_id,
                        "status": "dry_run",
                        "request": request,
                    });
                } else {
                    let dir = crate::cmd::approvals::requests_dir();
                    std::fs::create_dir_all(&dir)?;
                    std::fs::write(
                        dir.join(format!("{request_id}.json")),
                        serde_json::to_string_pretty(&request)?,
                    )?;
                    staged += 1;
                    item["staged"] = json!({"request_id": request_id, "status": "pending"});
                }
            }
        }
        items.push(item);
    }

    let payload = json!({
        "command": "mail triage",
        "policy": POLICY,
        "dry_run": dry_run,
        "items": items,
        "counts": {
            "reviewed": items.len(),
            "staged": staged,
            "already_staged": skipped_existing,
        },
        "next": "heiwa approvals list — decide staged reply drafts",
    });
    if json_output {
        println!("{payload}");
        return Ok(());
    }
    println!("mail triage ({} messages)", items.len());
    for item in &items {
        let msg = &item["message"];
        let sender = msg.get("sender").and_then(Value::as_str).unwrap_or("?");
        let subject = msg.get("subject").and_then(Value::as_str).unwrap_or("?");
        let action = item["suggestion"]["action"].as_str().unwrap_or("?");
        println!("  [{action}] {sender} — {subject}");
        if let Some(reason) = item["suggestion"]["reason"].as_str() {
            println!("        {reason}");
        }
        if let Some(staged_info) = item.get("staged") {
            println!(
                "        approval: {} ({})",
                staged_info["request_id"].as_str().unwrap_or("?"),
                staged_info["status"].as_str().unwrap_or("?")
            );
        }
    }
    if staged > 0 {
        println!("  staged {staged} reply draft(s): heiwa approvals list");
    }
    Ok(())
}

/// One-line metadata-only summary. Honest about what we know: bodies are
/// never read, so the summary is sender + subject + age, not content.
fn message_summary(row: &Value) -> String {
    let sender = row.get("sender").and_then(Value::as_str).unwrap_or("?");
    let subject = row.get("subject").and_then(Value::as_str).unwrap_or("?");
    let date = row
        .get("date")
        .and_then(Value::as_str)
        .map(|d| &d[..d.len().min(10)])
        .unwrap_or("?");
    let unread = if row.get("unread").and_then(Value::as_bool).unwrap_or(false) {
        "unread"
    } else {
        "read"
    };
    format!("{sender}: \"{subject}\" ({date}, {unread})")
}

fn suggested_action(row: &Value) -> Value {
    let action = row.get("action").and_then(Value::as_str).unwrap_or("digest");
    let score = row.get("score").and_then(Value::as_i64).unwrap_or(0);
    match action {
        "draft" => json!({
            "action": "reply",
            "reason": "high priority — reply draft staged for approval",
        }),
        "report" => json!({
            "action": "review",
            "reason": "worth a look; no action staged",
        }),
        _ if score < 0 => json!({
            "action": "delete",
            "reason": "bulk sender — suggest delete/archive (not staged: no write bridge; Gmail scope is read-only, Apple bridge is metadata-only)",
        }),
        _ => json!({
            "action": "digest",
            "reason": "low priority — leave for the daily digest",
        }),
    }
}

/// Deterministic request id per message so repeated triage runs never
/// duplicate pending approvals.
pub(crate) fn triage_request_id(row: &Value) -> String {
    use sha1::{Digest, Sha1};
    let key = snapshot_dedupe_key(row);
    let digest = Sha1::digest(key.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("req_mail_{}", &hex[..12])
}

fn build_triage_request(request_id: &str, row: &Value, draft: &str, draft_source: &str) -> Value {
    json!({
        "schema_version": "operator_dispatch_request_v1",
        "request_id": request_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "action": "mail-reply-draft",
        "target_surface": "mail",
        "target_scope": snapshot_dedupe_key(row),
        "requested_mode": "draft",
        "intent": {
            "message": row,
            "draft": draft,
            "draft_source": draft_source,
            "policy": POLICY,
            "note": "draft-never-send: approval stages the draft to the local outbox; sending stays manual until a send scope exists",
        },
        "source": "heiwa mail triage",
    })
}

/// Suggested reply text. Tries local Ollama first (metadata only — the
/// prompt carries sender + subject, never a body, and never leaves the
/// machine); falls back to a deterministic template when Ollama is down.
fn generate_draft(row: &Value) -> (String, String) {
    let base = std::env::var("HEIWA_OLLAMA_BASE")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    let model =
        std::env::var("HEIWA_OLLAMA_MODEL").unwrap_or_else(|_| "gemma4:latest".to_string());
    let sender = row.get("sender").and_then(Value::as_str).unwrap_or("");
    let subject = row.get("subject").and_then(Value::as_str).unwrap_or("");
    let prompt = format!(
        "Draft a short email reply (2-3 sentences) on behalf of Devon. \
         You only know the metadata — sender: {sender}, subject: \"{subject}\" — \
         the body was not read for privacy reasons. Acknowledge the email and \
         promise a fuller response where appropriate. Plain text only, no \
         subject line, no placeholders. Sign as Devon."
    );
    let body = json!({"model": model, "prompt": prompt, "stream": false});
    let output = std::process::Command::new("curl")
        .args(["-s", "-m", "45", "-X", "POST"])
        .arg(format!("{base}/api/generate"))
        .arg("-d")
        .arg(body.to_string())
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(parsed) = serde_json::from_slice::<Value>(&output.stdout) {
                if let Some(text) = parsed.get("response").and_then(Value::as_str) {
                    let text = text.trim();
                    if !text.is_empty() {
                        return (text.to_string(), format!("ollama:{model}"));
                    }
                }
            }
        }
    }
    (template_draft(row), "template".to_string())
}

pub(crate) fn template_draft(row: &Value) -> String {
    let subject = row.get("subject").and_then(Value::as_str).unwrap_or("your email");
    format!(
        "Hi — thanks for your note about \"{subject}\". I've seen it and will \
         follow up with a proper reply shortly. — Devon"
    )
}

// ---------------------------------------------------------------------------
// Approval effects: outbox staging + dismissal receipts
// ---------------------------------------------------------------------------

fn outbox_dir() -> PathBuf {
    headers_snapshot_path()
        .parent()
        .map(|p| p.join("outbox"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn mail_receipts_dir() -> PathBuf {
    headers_snapshot_path()
        .parent()
        .map(|p| p.join("receipts"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Approve effect for a mail-reply-draft request: materialize the draft in
/// the local outbox (manual send) and record a receipt. Never sends.
pub(crate) fn stage_outbox_draft(request_id: &str, intent: &Value) -> Result<Value> {
    let message = intent.get("message").cloned().unwrap_or_else(|| json!({}));
    let draft = intent
        .get("draft")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("request {request_id} has no draft text"))?;
    let subject = message.get("subject").and_then(Value::as_str).unwrap_or("");
    let entry = json!({
        "id": request_id,
        "to": message.get("sender"),
        "subject": format!("Re: {subject}"),
        "body": draft,
        "draft_source": intent.get("draft_source"),
        "status": "ready_for_manual_send",
        "staged_at": chrono::Utc::now().to_rfc3339(),
        "approved_by_decision": request_id,
        "external_writes": [],
    });
    let dir = outbox_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{request_id}.json")),
        serde_json::to_string_pretty(&entry)?,
    )?;

    let receipt = json!({
        "receipt_id": format!("rcpt-{request_id}-outbox"),
        "kind": "mail_reply_draft_staged",
        "request_id": request_id,
        "to": message.get("sender"),
        "subject": entry["subject"],
        "created_at": chrono::Utc::now().to_rfc3339(),
        "external_writes": [],
    });
    let receipts = mail_receipts_dir();
    std::fs::create_dir_all(&receipts)?;
    std::fs::write(
        receipts.join(format!("rcpt-{request_id}-outbox.json")),
        receipt.to_string(),
    )?;
    Ok(entry)
}

/// Deny effect: record that the suggestion was dismissed so triage never
/// re-stages it (the decision file already blocks re-staging; the receipt
/// makes the dismissal visible in the evidence lane).
pub(crate) fn dismiss_suggestion(request_id: &str, target_scope: &str) -> Result<()> {
    let receipt = json!({
        "receipt_id": format!("rcpt-{request_id}-dismissed"),
        "kind": "mail_suggestion_dismissed",
        "request_id": request_id,
        "message_key": target_scope,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "external_writes": [],
    });
    let receipts = mail_receipts_dir();
    std::fs::create_dir_all(&receipts)?;
    std::fs::write(
        receipts.join(format!("rcpt-{request_id}-dismissed.json")),
        receipt.to_string(),
    )?;
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let idx = args.iter().position(|arg| arg == flag)?;
    args.get(idx + 1).cloned()
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
            "status": if probe.accounts.is_empty() { "planned" } else { "metadata" },
            "read": "Metadata snapshot via `heiwa mail scan --source apple` (account, mailbox, sender, subject, date, unread).",
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
    println!("  heiwa mail scan [--source apple|gmail|all] [--limit N] [--dry-run] [--json]");
    println!("  heiwa mail triage [--limit N] [--no-draft] [--dry-run] [--json]");
    println!();
    println!("Policy: {POLICY}.");
    println!("Scan writes metadata rows to ~/.heiwa/state/mail/headers.jsonl with a receipt;");
    println!("message bodies are never read. Triage stages reply drafts (local Ollama or");
    println!("template) as pending approvals; approve moves the draft to the local outbox —");
    println!("nothing is ever sent automatically.");
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

    #[test]
    fn snapshot_append_dedupes_and_stamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let row = json!({
            "account": "a", "mailbox": "INBOX", "sender": "s",
            "subject": "x", "date": "2026-06-12", "unread": true,
        });
        let appended = append_snapshot_rows_at(&path, &[row.clone(), row.clone()]).unwrap();
        assert_eq!(appended, 1, "identical rows in one batch dedupe");
        let appended_again = append_snapshot_rows_at(&path, &[row]).unwrap();
        assert_eq!(appended_again, 0, "re-scan of same row appends nothing");
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 1);
        assert!(raw.contains("\"scanned_at\""));
    }

    #[test]
    fn snapshot_file_stays_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let rows: Vec<Value> = (0..MAX_SNAPSHOT_LINES + 50)
            .map(|i| {
                json!({
                    "account": "a", "mailbox": "INBOX", "sender": format!("s{i}"),
                    "subject": format!("subject {i}"), "date": "2026-06-12", "unread": false,
                })
            })
            .collect();
        append_snapshot_rows_at(&path, &rows).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), MAX_SNAPSHOT_LINES);
        assert!(
            raw.contains(&format!("subject {}", MAX_SNAPSHOT_LINES + 49)),
            "newest rows are kept"
        );
        assert!(!raw.contains("\"subject 0\""), "oldest rows are dropped");
    }

    #[test]
    fn triage_request_id_is_deterministic_per_message() {
        let row = json!({
            "account": "a", "sender": "s@example.com", "subject": "Invoice", "date": "2026-06-12",
        });
        let id1 = triage_request_id(&row);
        let id2 = triage_request_id(&row);
        assert_eq!(id1, id2);
        assert!(id1.starts_with("req_mail_"));
        let other = json!({
            "account": "a", "sender": "s@example.com", "subject": "Other", "date": "2026-06-12",
        });
        assert_ne!(id1, triage_request_id(&other));
    }

    #[test]
    fn template_draft_mentions_subject() {
        let row = json!({"subject": "Invoice follow-up"});
        let draft = template_draft(&row);
        assert!(draft.contains("Invoice follow-up"));
        assert!(draft.contains("Devon"));
    }

    #[test]
    fn triage_request_shape_matches_dispatch_v1() {
        let row = json!({
            "account": "a", "mailbox": "INBOX", "sender": "vendor@example.com",
            "subject": "Invoice overdue", "date": "2026-06-11", "unread": true,
            "score": 5, "action": "draft",
        });
        let req = build_triage_request("req_mail_abc", &row, "draft text", "template");
        assert_eq!(req["schema_version"], "operator_dispatch_request_v1");
        assert_eq!(req["action"], "mail-reply-draft");
        assert_eq!(req["target_surface"], "mail");
        assert_eq!(req["requested_mode"], "draft");
        assert_eq!(req["intent"]["draft"], "draft text");
        let summary = crate::cmd::approvals::approval_request_summary(&req);
        assert_eq!(summary["action"], "mail-reply-draft");
        assert_eq!(summary["risk"], "draft");
    }

    #[test]
    fn bulk_sender_suggestion_is_delete_and_honest_about_no_bridge() {
        let row = json!({"sender": "noreply@x.com", "subject": "w", "score": -2, "action": "digest"});
        let suggestion = suggested_action(&row);
        assert_eq!(suggestion["action"], "delete");
        assert!(suggestion["reason"].as_str().unwrap().contains("not staged"));
    }

    #[test]
    fn gmail_metadata_normalizes_to_snapshot_row() {
        let message = json!({
            "internalDate": "1749710000000",
            "labelIds": ["INBOX", "UNREAD"],
            "payload": {"headers": [
                {"name": "From", "value": "Vendor <vendor@example.com>"},
                {"name": "Subject", "value": "Invoice follow-up"},
            ]},
        });
        let row = gmail_message_to_row(&message, "devon@gmail.com").unwrap();
        assert_eq!(row["account"], "devon@gmail.com");
        assert_eq!(row["sender"], "Vendor <vendor@example.com>");
        assert_eq!(row["subject"], "Invoice follow-up");
        assert_eq!(row["unread"], true);
        assert!(row["date"].as_str().unwrap().starts_with("2025-06-12"));
    }
}
