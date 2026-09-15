use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

const POLICY: &str = "metadata-only-no-body";

/// Max header lines scanned from the metadata snapshot per summary build.
const HEADER_SCAN_LIMIT: usize = 200;
/// Max priority rows surfaced to read models.
const PRIORITY_LIMIT: usize = 10;

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => status(args),
        Some("accounts") => accounts(args),
        Some("summary") => {
            println!("{}", summary_payload());
            Ok(())
        }
        Some("scan") => scan(&args[1..]),
        Some("triage") => triage(&args[1..]).await,
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
        let data_dir = heiwa_config::HeiwaPaths::resolve()
            .home_dir
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
const RETENTION_DAYS: i64 = 60;
const DEFAULT_SCAN_DAYS: u64 = 14;
const DEFAULT_PER_ACCOUNT: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct MailSyncState {
    schema_version: u32,
    consented_at: Option<chrono::DateTime<chrono::Utc>>,
    last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    last_scan_at: Option<chrono::DateTime<chrono::Utc>>,
    complete: Option<bool>,
    fetched: Option<usize>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MailSyncDecision {
    NoConsent,
    Fresh,
    Backoff,
    Scan,
}

fn mail_sync_state_path() -> PathBuf {
    crate::home::heiwa_state_dir()
        .join("mail")
        .join("apple_sync.json")
}

fn load_mail_sync_state() -> MailSyncState {
    std::fs::read(mail_sync_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .filter(|state: &MailSyncState| state.schema_version == 1)
        .unwrap_or_default()
}

fn save_mail_sync_state(state: &MailSyncState) -> Result<()> {
    let path = mail_sync_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_private_write(&path, serde_json::to_string_pretty(state)?.as_bytes())
}

fn mail_sync_decision(
    saved: &MailSyncState,
    now: chrono::DateTime<chrono::Utc>,
    stale_seconds: Option<u64>,
    background: bool,
) -> MailSyncDecision {
    if background && saved.consented_at.is_none() {
        return MailSyncDecision::NoConsent;
    }
    let Some(max_age) =
        stale_seconds.map(|seconds| chrono::Duration::seconds(seconds.min(86_400) as i64))
    else {
        return MailSyncDecision::Scan;
    };
    if saved.error.is_some()
        && saved.last_attempt_at.is_some()
        && saved.last_attempt_at >= saved.last_scan_at
    {
        if let Some(attempt) = saved.last_attempt_at {
            if now - attempt < max_age.min(chrono::Duration::seconds(60)) {
                return MailSyncDecision::Backoff;
            }
        }
    }
    if saved.last_scan_at.is_some_and(|scan| now - scan < max_age) {
        MailSyncDecision::Fresh
    } else {
        MailSyncDecision::Scan
    }
}

#[derive(Debug, Clone)]
struct AccountScan {
    account: String,
    window_start: String,
    window_end: String,
    matched: usize,
    rows: Vec<Value>,
}

#[derive(Debug, Default)]
struct MailScanOutput {
    status: String,
    accounts: Vec<AccountScan>,
}

#[derive(Debug, Default)]
struct ReconcileCounts {
    appended: usize,
    updated: usize,
    removed: usize,
    total: usize,
}

/// JXA program: query each account's INBOX independently, newest first. The
/// only properties read are metadata fields; no body or recipient content is
/// ever touched.
const APPLE_MAIL_JXA: &str = r#"
function run(argv) {
  const days = Math.max(1, parseInt(argv[0] || "14", 10));
  const perAccount = Math.max(1, parseInt(argv[1] || "200", 10));
  const ifRunning = argv[2] === "1";
  if (ifRunning) {
    const probe = Application("Mail");
    if (!probe.running()) return JSON.stringify({kind: "status", status: "mail_not_running"});
  }
  const Mail = Application("Mail");
  const cutoff = new Date(Date.now() - days * 86400000);
  const now = new Date();
  const output = [];
  let accounts = [];
  try { accounts = Mail.accounts(); } catch (e) {
    throw new Error("Apple Mail account query failed: " + e);
  }
  for (let a = 0; a < accounts.length; a++) {
    let accountName = "";
    let inbox;
    try {
      accountName = String(accounts[a].name());
      const mailboxes = accounts[a].mailboxes();
      inbox = mailboxes.find((box) => /^(INBOX|Inbox)$/.test(String(box.name())));
    } catch (e) { continue; }
    if (!inbox) continue;
    try {
      const spec = inbox.messages.whose({dateReceived: {_greaterThan: cutoff}});
      const dates = spec.dateReceived();
      const senders = spec.sender();
      const subjects = spec.subject();
      const readStates = spec.readStatus();
      const messageIds = spec.messageId();
      const rows = [];
      for (let i = 0; i < dates.length; i++) {
        const date = new Date(dates[i]);
        if (isNaN(date.getTime())) continue;
        rows.push({
          account: accountName,
          mailbox: String(inbox.name()),
          sender: String(senders[i] || ""),
          subject: String(subjects[i] || ""),
          date: date.toISOString(),
          unread: !Boolean(readStates[i]),
          message_id: messageIds[i] ? String(messageIds[i]) : ""
        });
      }
      rows.sort((left, right) => right.date.localeCompare(left.date));
      const kept = rows.slice(0, perAccount);
      const windowStart = rows.length > kept.length && kept.length
        ? kept[kept.length - 1].date
        : cutoff.toISOString();
      output.push(JSON.stringify({
        kind: "account",
        account: accountName,
        window_start: windowStart,
        window_end: now.toISOString(),
        matched: rows.length,
        kept: kept.length
      }));
      for (const row of kept) output.push(JSON.stringify(row));
    } catch (e) { continue; }
  }
  return output.join("\n");
}
"#;

fn scan(args: &[String]) -> Result<()> {
    let source = flag_value(args, "--source")
        .unwrap_or_else(|| "all".to_string())
        .replace('-', "_")
        .to_ascii_lowercase();
    if source == "gmail" {
        return Err(anyhow!(
            "Gmail reads are disabled; use the local Apple Mail metadata bridge"
        ));
    }
    if source != "all" && source != "apple" && source != "apple_mail" {
        return Err(anyhow!(
            "unknown mail scan source: {source} (expected all or apple)"
        ));
    }
    let days = flag_value(args, "--days")
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SCAN_DAYS)
        .clamp(1, 60);
    let per_account = flag_value(args, "--per-account")
        .or_else(|| flag_value(args, "--limit"))
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_PER_ACCOUNT)
        .clamp(1, 500);
    let dry_run = has_flag(args, "--dry-run");
    let json_output = has_flag(args, "--json");
    let if_running = has_flag(args, "--if-running");
    let stale_seconds = flag_value(args, "--if-stale").and_then(|raw| raw.parse::<u64>().ok());
    let background = if_running;

    if dry_run {
        let ready = apple_mail_accounts_present() || std::env::var_os("HEIWA_OSASCRIPT").is_some();
        let payload = json!({
            "command": "mail scan",
            "dry_run": true,
            "policy": POLICY,
            "days": days,
            "per_account": per_account,
            "snapshot": headers_snapshot_path().display().to_string(),
            "sources": {"apple": {
                "selected": source == "all" || source == "apple",
                "ready": ready,
                "blocker": if ready { Value::Null } else {
                    Value::String("no Apple Mail accounts configured".into())
                }
            }}
        });
        println!("{payload}");
        return Ok(());
    }

    let now = chrono::Utc::now();
    let mut saved = load_mail_sync_state();
    if stale_seconds.is_some() || background {
        match mail_sync_decision(&saved, now, stale_seconds, background) {
            MailSyncDecision::NoConsent => {
                return print_scan_payload(
                    json!({"status": "no_consent", "sources": [{"source":"apple","status":"no_consent"}], "fetched":0, "appended":0, "updated":0, "removed":0}),
                    json_output,
                );
            }
            MailSyncDecision::Fresh => {
                return print_scan_payload(
                    json!({"status": "fresh", "freshness":"fresh", "sources": [{"source":"apple","status":"fresh"}], "fetched":saved.fetched.unwrap_or(0), "appended":0, "updated":0, "removed":0, "last_scan_at":saved.last_scan_at, "last_attempt_at":saved.last_attempt_at}),
                    json_output,
                );
            }
            MailSyncDecision::Backoff => {
                return print_scan_payload(
                    json!({"status": "error", "freshness":"backoff", "sources": [{"source":"apple","status":"error","error":saved.error.clone().unwrap_or_else(||"previous scan failed".into())}], "fetched":0, "appended":0, "updated":0, "removed":0}),
                    json_output,
                );
            }
            MailSyncDecision::Scan => {}
        }
    }

    let apple_ready =
        apple_mail_accounts_present() || std::env::var_os("HEIWA_OSASCRIPT").is_some();
    if source != "all" && source != "apple" && source != "apple_mail" {
        return Err(anyhow!(
            "only Apple Mail metadata scanning is currently available"
        ));
    }
    if !apple_ready {
        let reason = format!(
            "no Apple Mail accounts found under {}; open Mail.app and add an account, or grant Heiwa automation access",
            MailProbe::detect().data_dir.display()
        );
        let payload = json!({"status":"skipped","sources":[{"source":"apple","status":"skipped","reason":reason}],"fetched":0,"appended":0,"updated":0,"removed":0});
        return print_scan_payload(payload, json_output);
    }

    saved.schema_version = 1;
    saved.last_attempt_at = Some(now);
    let output = match scan_apple_mail(days, per_account, if_running) {
        Ok(output) => output,
        Err(error) => {
            saved.error = Some(error.to_string());
            let _ = save_mail_sync_state(&saved);
            let payload = json!({"status":"error","sources":[{"source":"apple","status":"error","error":error.to_string()}],"fetched":0,"appended":0,"updated":0,"removed":0});
            return print_scan_payload(payload, json_output);
        }
    };
    if output.status == "mail_not_running" {
        saved.error = None;
        let _ = save_mail_sync_state(&saved);
        return print_scan_payload(
            json!({"status":"mail_not_running","freshness":"closed","sources":[{"source":"apple","status":"mail_not_running"}],"fetched":0,"appended":0,"updated":0,"removed":0}),
            json_output,
        );
    }

    let counts = reconcile_snapshot(&output.accounts)?;
    saved.consented_at = if background {
        saved.consented_at
    } else {
        saved.consented_at.or(Some(now))
    };
    saved.last_scan_at = Some(now);
    saved.complete = Some(true);
    saved.fetched = Some(counts.total);
    saved.error = None;
    save_mail_sync_state(&saved)?;
    let source_report = json!({
        "source": "apple", "status": "scanned", "accounts": output.accounts.len(),
        "matched": output.accounts.iter().map(|account| account.matched).sum::<usize>(),
        "fetched": counts.total, "updated": counts.updated, "removed": counts.removed
    });
    let receipt = write_scan_receipt(
        std::slice::from_ref(&source_report),
        counts.total,
        counts.appended,
    )?;
    let payload = json!({
        "status": "scanned", "freshness": "scanned", "sources": [source_report],
        "fetched": counts.total, "appended": counts.appended, "updated": counts.updated,
        "removed": counts.removed, "deduplicated": counts.updated, "snapshot": headers_snapshot_path(),
        "receipt": receipt, "last_scan_at": saved.last_scan_at, "last_attempt_at": saved.last_attempt_at
    });
    print_scan_payload(payload, json_output)
}

fn print_scan_payload(payload: Value, json_output: bool) -> Result<()> {
    if json_output {
        println!("{payload}");
    } else {
        println!("mail scan");
        println!(
            "  status: {}",
            payload["status"].as_str().unwrap_or("unknown")
        );
        println!("  fetched: {}", payload["fetched"].as_u64().unwrap_or(0));
        println!("  added: {}", payload["appended"].as_u64().unwrap_or(0));
        println!("  updated: {}", payload["updated"].as_u64().unwrap_or(0));
        println!("  removed: {}", payload["removed"].as_u64().unwrap_or(0));
    }
    Ok(())
}

/// Pull inbox metadata from Mail.app via JXA (osascript -l JavaScript).
fn scan_apple_mail(days: u64, per_account: usize, if_running: bool) -> Result<MailScanOutput> {
    let output = crate::cmd::osascript::run_jxa(
        APPLE_MAIL_JXA,
        &[
            days.to_string(),
            per_account.to_string(),
            if if_running {
                "1".to_string()
            } else {
                "0".to_string()
            },
        ],
    )?;
    let text = String::from_utf8_lossy(&output);
    let mut result = MailScanOutput::default();
    let mut current: Option<AccountScan> = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|error| anyhow!("invalid Apple Mail scan output: {error}"))?;
        if value.get("kind").and_then(Value::as_str) == Some("status") {
            result.status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_string();
            continue;
        }
        if value.get("kind").and_then(Value::as_str) == Some("account") {
            if let Some(account) = current.take() {
                result.accounts.push(account);
            }
            current = Some(AccountScan {
                account: value["account"].as_str().unwrap_or("").to_string(),
                window_start: value["window_start"].as_str().unwrap_or("").to_string(),
                window_end: value["window_end"].as_str().unwrap_or("").to_string(),
                matched: value["matched"].as_u64().unwrap_or(0) as usize,
                rows: Vec::new(),
            });
        } else if let Some(account) = current.as_mut() {
            let mut row = value;
            let account_name = row["account"].as_str().unwrap_or("").to_string();
            let message_id = row.get("message_id").and_then(Value::as_str).unwrap_or("");
            let id = mail_row_id(&account_name, message_id, &row);
            if let Some(object) = row.as_object_mut() {
                object.remove("message_id");
                object.insert("id".into(), json!(id));
                object.insert("scanned_at".into(), json!(chrono::Utc::now().to_rfc3339()));
            }
            account.rows.push(row);
        }
    }
    if let Some(account) = current {
        result.accounts.push(account);
    }
    if result.status.is_empty() {
        result.status = "scanned".into();
    }
    Ok(result)
}

fn mail_row_id(account: &str, message_id: &str, row: &Value) -> String {
    use sha2::{Digest, Sha256};
    let fallback = format!(
        "{}|{}|{}|{}",
        account,
        row.get("sender").and_then(Value::as_str).unwrap_or(""),
        row.get("subject").and_then(Value::as_str).unwrap_or(""),
        row.get("date").and_then(Value::as_str).unwrap_or(""),
    );
    let source = if message_id.is_empty() {
        fallback
    } else {
        format!("{account}\0{message_id}")
    };
    let digest = Sha256::digest(source.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("mail_{}", &hex[..16])
}

/// Stable identity for a snapshot row so repeated scans never duplicate.
fn snapshot_dedupe_key(row: &Value) -> String {
    row.get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            mail_row_id(
                row.get("account").and_then(Value::as_str).unwrap_or(""),
                row.get("message_id").and_then(Value::as_str).unwrap_or(""),
                row,
            )
        })
}

fn reconcile_snapshot(accounts: &[AccountScan]) -> Result<ReconcileCounts> {
    reconcile_snapshot_at(&headers_snapshot_path(), accounts)
}

fn reconcile_snapshot_at(
    path: &std::path::Path,
    accounts: &[AccountScan],
) -> Result<ReconcileCounts> {
    use anyhow::Context;
    use std::fs::OpenOptions;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_path = path.with_extension("jsonl.lock");
    let mut lock_options = OpenOptions::new();
    lock_options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options
        .open(&lock_path)
        .with_context(|| format!("failed to open mail snapshot lock {}", lock_path.display()))?;
    lock.lock().context("failed to lock mail snapshot")?;
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("failed to read mail snapshot"),
    };
    let mut existing = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let parsed: Value = serde_json::from_str(line).context("invalid mail snapshot JSON")?;
        if !parsed.is_object() {
            return Err(anyhow!("invalid mail snapshot row: expected object"));
        }
        existing.push(parsed);
    }
    let mut counts = ReconcileCounts::default();
    let now = chrono::Utc::now();
    let retention_cutoff = now - chrono::Duration::days(RETENTION_DAYS);
    existing.retain(|row| {
        row.get("date")
            .and_then(Value::as_str)
            .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
            .map(|date| date.with_timezone(&chrono::Utc) >= retention_cutoff)
            .unwrap_or(true)
    });
    for account in accounts {
        let start = chrono::DateTime::parse_from_rfc3339(&account.window_start)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc));
        let end = chrono::DateTime::parse_from_rfc3339(&account.window_end)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc));
        let returned: std::collections::HashMap<String, Value> = account
            .rows
            .iter()
            .map(|row| (snapshot_dedupe_key(row), row.clone()))
            .collect();
        let mut next = Vec::with_capacity(existing.len() + returned.len());
        for row in existing.drain(..) {
            let same_account =
                row.get("account").and_then(Value::as_str) == Some(account.account.as_str());
            let inside = same_account
                && start.zip(end).is_some_and(|(start, end)| {
                    row.get("date")
                        .and_then(Value::as_str)
                        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
                        .map(|date| {
                            let date = date.with_timezone(&chrono::Utc);
                            date >= start && date <= end
                        })
                        .unwrap_or(false)
                });
            if inside {
                if let Some(replacement) = returned.get(&snapshot_dedupe_key(&row)) {
                    next.push(replacement.clone());
                } else {
                    counts.removed += 1;
                }
            } else {
                next.push(row);
            }
        }
        existing = next;
        for (key, row) in returned {
            if !existing
                .iter()
                .any(|candidate| snapshot_dedupe_key(candidate) == key)
            {
                existing.push(row);
                counts.appended += 1;
            } else {
                counts.updated += 1;
            }
        }
    }
    existing.sort_by(|a, b| {
        let left = a.get("date").and_then(Value::as_str).unwrap_or("");
        let right = b.get("date").and_then(Value::as_str).unwrap_or("");
        right.cmp(left)
    });
    if existing.len() > MAX_SNAPSHOT_LINES {
        existing.truncate(MAX_SNAPSHOT_LINES);
    }
    let serialized = existing
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + if existing.is_empty() { "" } else { "\n" };
    atomic_private_write(path, serialized.as_bytes())?;
    counts.total = accounts.iter().map(|account| account.rows.len()).sum();
    drop(lock);
    Ok(counts)
}

fn atomic_private_write(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    use anyhow::Context;
    use std::fs::OpenOptions;
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("mail snapshot has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("headers.jsonl");
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut temp_options = OpenOptions::new();
    temp_options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        temp_options.mode(0o600);
    }
    let write_result = (|| -> Result<()> {
        let mut temp = temp_options.open(&temp_path).with_context(|| {
            format!(
                "failed to create private mail snapshot {}",
                temp_path.display()
            )
        })?;
        temp.write_all(contents)?;
        temp.sync_all()?;
        drop(temp);
        std::fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to atomically replace mail snapshot {}",
                path.display()
            )
        })?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .context("failed to sync mail snapshot directory")?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
fn append_snapshot_rows_at(path: &std::path::Path, rows: &[Value]) -> Result<usize> {
    use anyhow::Context;
    use std::fs::OpenOptions;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_path = path.with_extension("jsonl.lock");
    let mut lock_options = OpenOptions::new();
    lock_options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options
        .open(&lock_path)
        .with_context(|| format!("failed to open mail snapshot lock {}", lock_path.display()))?;
    lock.lock().context("failed to lock mail snapshot")?;
    let existing_raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("failed to read mail snapshot"),
    };
    let mut existing_rows = Vec::new();
    for line in existing_raw.lines().filter(|line| !line.trim().is_empty()) {
        let parsed: Value = serde_json::from_str(line).context("invalid mail snapshot JSON")?;
        if !parsed.is_object() {
            return Err(anyhow!("invalid mail snapshot row: expected object"));
        }
        existing_rows.push(parsed);
    }
    let mut positions: std::collections::HashMap<String, usize> = existing_rows
        .iter()
        .enumerate()
        .map(|(index, row)| (snapshot_dedupe_key(row), index))
        .collect();
    let scanned_at = chrono::Utc::now().to_rfc3339();
    let mut appended = 0;
    for row in rows {
        let key = snapshot_dedupe_key(row);
        let mut stamped = row.clone();
        if let Some(obj) = stamped.as_object_mut() {
            obj.insert("scanned_at".into(), json!(&scanned_at));
        }
        if let Some(index) = positions.get(&key).copied() {
            existing_rows[index] = stamped;
        } else {
            let index = existing_rows.len();
            existing_rows.push(stamped);
            positions.insert(key, index);
            appended += 1;
        }
    }
    if existing_rows.len() > MAX_SNAPSHOT_LINES {
        let drop = existing_rows.len() - MAX_SNAPSHOT_LINES;
        existing_rows.drain(..drop);
    }
    let serialized = existing_rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    atomic_private_write(path, serialized.as_bytes())?;
    drop(lock);
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
        "receipt_id": receipt_id, "kind": "mail_metadata_scan", "policy": POLICY,
        "sources": sources, "fetched": fetched, "appended": appended,
        "snapshot": headers_snapshot_path().display().to_string(),
        "scanned_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(
        receipts_dir.join(format!("{receipt_id}.json")),
        receipt.to_string(),
    )?;
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
async fn triage(args: &[String]) -> Result<()> {
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
        let action = row
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("digest");
        let suggestion = suggested_action(row);
        let mut item = json!({
            "message": row,
            "summary": message_summary(row),
            "suggestion": suggestion,
        });

        if action == "draft" {
            let request_id = triage_request_id(row);
            let already = crate::cmd::approvals::requests_dir()
                .join(format!("{request_id}.json"))
                .exists()
                || crate::cmd::approvals::decisions_dir()
                    .join(format!("{request_id}.json"))
                    .exists();
            if already {
                skipped_existing += 1;
                item["staged"] = json!({"request_id": request_id, "status": "already_staged"});
            } else {
                let (draft, draft_source) = if no_draft {
                    (template_draft(row), "template".to_string())
                } else {
                    generate_draft(row).await
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
    let action = row
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("digest");
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

/// Suggested reply text. Tries the local-only model-call route first (metadata only — the
/// prompt carries sender + subject, never a body, and never leaves the
/// machine); falls back to a deterministic template when Ollama is down.
/// Model choice and durable route evidence come from `ModelCallExecutor`.
async fn generate_draft(row: &Value) -> (String, String) {
    let prompt = draft_prompt(row);
    let candidates = crate::discovered_model_call_candidates().await;
    if let Ok(result) = crate::execute_mail_draft_model_call(candidates, &prompt).await {
        let text = result.text.trim();
        if !text.is_empty() {
            return (
                text.to_string(),
                format!(
                    "{}/{} (drex-routed)",
                    result.provider, result.provider_model_id
                ),
            );
        }
    }
    (template_draft(row), "template".to_string())
}

/// The signature drafts are written under.
///
/// Drafts are outgoing text written on the user's behalf, so this must never
/// be a name baked into the binary. Until L2 establishes a per-user identity
/// with a display name, drafts go unsigned rather than signed by whoever
/// happened to build Heiwa.
fn draft_signature() -> Option<String> {
    heiwa_provider::load_identity()
        .and_then(|identity| identity.display_name)
        .filter(|name| !name.trim().is_empty())
}

fn draft_prompt(row: &Value) -> String {
    draft_prompt_with(row, draft_signature().as_deref())
}

/// Pure form: the signature is an input, so the invariant "no name is baked
/// into the binary" is testable without depending on whose machine runs it.
fn draft_prompt_with(row: &Value, signer: Option<&str>) -> String {
    let sender = row.get("sender").and_then(Value::as_str).unwrap_or("");
    let subject = row.get("subject").and_then(Value::as_str).unwrap_or("");
    let signature = match signer {
        Some(name) => format!(" Sign as {name}."),
        None => " Do not add a signature or sign-off name.".to_string(),
    };
    format!(
        "Draft a short email reply (2-3 sentences) on the account owner's \
         behalf. You only know the metadata — sender: {sender}, subject: \
         \"{subject}\" — the body was not read for privacy reasons. \
         Acknowledge the email and promise a fuller response where \
         appropriate. Plain text only, no subject line, no placeholders.\
         {signature}"
    )
}

pub(crate) fn template_draft(row: &Value) -> String {
    template_draft_with(row, draft_signature().as_deref())
}

fn template_draft_with(row: &Value, signer: Option<&str>) -> String {
    let subject = row
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("your email");
    let signature = match signer {
        Some(name) => format!(" — {name}"),
        None => String::new(),
    };
    format!(
        "Hi — thanks for your note about \"{subject}\". I've seen it and will \
         follow up with a proper reply shortly.{signature}"
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
    crate::home::heiwa_state_dir()
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
    scored.sort_by_key(|row| std::cmp::Reverse(row.0));
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

    let lanes = json!([
        {
            "id": "gmail",
            "name": "Gmail",
            "status": "planned",
            "read": "Disabled by policy; Mail.app owns local metadata reads without restricted Gmail scopes.",
            "reply": "Draft replies in-app; gmail.send remains ungranted until an approval-backed sender exists.",
            "guardrail": "No Gmail read scope; every future outbound send must show the full body and record a receipt.",
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
    println!("  heiwa mail scan [--source apple|all] [--days N] [--per-account N] [--if-running] [--if-stale SECONDS] [--dry-run] [--json]");
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
    fn apple_mail_script_propagates_account_query_failure() {
        assert!(APPLE_MAIL_JXA.contains("Apple Mail account query failed"));
        assert!(!APPLE_MAIL_JXA.contains("catch (e) { return \"\"; }"));
    }

    #[test]
    fn sync_decision_covers_consent_freshness_backoff_and_scan() {
        let now = chrono::Utc::now();
        let empty = MailSyncState::default();
        assert_eq!(
            mail_sync_decision(&empty, now, Some(180), true),
            MailSyncDecision::NoConsent
        );
        let fresh = MailSyncState {
            schema_version: 1,
            consented_at: Some(now),
            last_scan_at: Some(now),
            ..Default::default()
        };
        assert_eq!(
            mail_sync_decision(&fresh, now, Some(180), true),
            MailSyncDecision::Fresh
        );
        let failed = MailSyncState {
            schema_version: 1,
            consented_at: Some(now),
            last_attempt_at: Some(now),
            error: Some("denied".into()),
            ..Default::default()
        };
        assert_eq!(
            mail_sync_decision(&failed, now, Some(180), true),
            MailSyncDecision::Backoff
        );
        assert_eq!(
            mail_sync_decision(&empty, now, None, false),
            MailSyncDecision::Scan
        );
    }

    #[test]
    fn reconcile_updates_unread_removes_inside_window_and_preserves_unscanned_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let now = chrono::Utc::now();
        let target_date = (now - chrono::Duration::minutes(10)).to_rfc3339();
        let removed_date = (now - chrono::Duration::minutes(20)).to_rfc3339();
        let outside_date = (now - chrono::Duration::hours(3)).to_rfc3339();
        let target_id = mail_row_id("a", "<target>", &json!({}));
        let gone_id = mail_row_id("a", "<gone>", &json!({}));
        let target = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"x","date":target_date,"unread":true,"id":target_id});
        let removed = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"gone","date":removed_date,"unread":true,"id":gone_id});
        let outside = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"outside","date":outside_date,"unread":true,"id":"mail_outside"});
        let unscanned = json!({"account":"b","mailbox":"INBOX","sender":"s","subject":"untouched","date":target_date,"unread":true,"id":"mail_b"});
        let initial = [target.clone(), removed, outside.clone(), unscanned.clone()]
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&path, initial).unwrap();
        let refreshed = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"x","date":target_date,"unread":false,"id":target["id"]});
        let account = AccountScan {
            account: "a".into(),
            window_start: (now - chrono::Duration::hours(1)).to_rfc3339(),
            window_end: (now + chrono::Duration::minutes(1)).to_rfc3339(),
            matched: 1,
            rows: vec![refreshed],
        };
        let counts = reconcile_snapshot_at(&path, &[account]).unwrap();
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(counts.removed, 1);
        assert!(rows.iter().any(|row| row["id"] == "mail_outside"));
        assert!(rows.iter().any(|row| row["id"] == "mail_b"));
        assert!(rows
            .iter()
            .any(|row| row["id"] == target["id"] && row["unread"] == false));
        assert!(!rows.iter().any(|row| row["id"] == gone_id));
    }

    #[test]
    fn capped_account_only_deletes_inside_shortened_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let now = chrono::Utc::now();
        let older = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"older","date":(now - chrono::Duration::hours(2)).to_rfc3339(),"unread":true,"id":"mail_older"});
        let recent = json!({"account":"a","mailbox":"INBOX","sender":"s","subject":"recent","date":(now - chrono::Duration::minutes(10)).to_rfc3339(),"unread":true,"id":"mail_recent"});
        std::fs::write(
            &path,
            [older.clone(), recent.clone()]
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        let account = AccountScan {
            account: "a".into(),
            window_start: (now - chrono::Duration::hours(1)).to_rfc3339(),
            window_end: (now + chrono::Duration::minutes(1)).to_rfc3339(),
            matched: 1,
            rows: vec![],
        };
        reconcile_snapshot_at(&path, &[account]).unwrap();
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(rows.iter().any(|row| row["id"] == "mail_older"));
        assert!(!rows.iter().any(|row| row["id"] == "mail_recent"));
    }

    #[test]
    fn message_ids_are_hashed_and_never_stored() {
        let row = json!({"account":"a","sender":"s","subject":"x","date":"2026-09-14T00:00:00Z"});
        let id = mail_row_id("a", "<secret-message-id>", &row);
        assert!(id.starts_with("mail_"));
        assert_eq!(id.len(), 21);
        assert!(!id.contains("secret-message-id"));
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
    fn snapshot_refresh_updates_existing_row_and_preserves_unqueried_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let target = json!({
            "account": "a", "mailbox": "INBOX", "sender": "s",
            "subject": "x", "date": "2026-06-12", "unread": true,
        });
        let unqueried = json!({
            "account": "b", "mailbox": "INBOX", "sender": "other",
            "subject": "keep", "date": "2026-06-11", "unread": true,
        });
        assert_eq!(
            append_snapshot_rows_at(&path, &[target.clone(), unqueried.clone()]).unwrap(),
            2
        );

        let mut refreshed = target;
        refreshed["unread"] = json!(false);
        let appended = append_snapshot_rows_at(&path, &[refreshed]).unwrap();

        assert_eq!(appended, 0, "refreshing known mail is not an append");
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2, "refresh must not duplicate known mail");
        assert_eq!(rows[0]["unread"], false, "latest unread state wins");
        assert_eq!(rows[1]["sender"], "other");
        assert_eq!(rows[1]["unread"], true, "unqueried mail is preserved");
    }

    #[test]
    fn snapshot_corrupt_json_fails_without_replacing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let original = b"{not-json}\n";
        std::fs::write(&path, original).unwrap();

        let result = append_snapshot_rows_at(
            &path,
            &[json!({
                "account": "a", "sender": "s", "subject": "x", "date": "2026-06-12"
            })],
        );

        assert!(result.is_err(), "corrupt source must fail closed");
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn snapshot_non_object_row_fails_without_replacing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let original = b"[]\n";
        std::fs::write(&path, original).unwrap();

        let result = append_snapshot_rows_at(
            &path,
            &[json!({
                "account": "a", "sender": "s", "subject": "x", "date": "2026-06-12"
            })],
        );

        assert!(result.is_err(), "non-object snapshot rows must fail closed");
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn snapshot_invalid_utf8_fails_without_replacing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let original = [0xff, 0xfe, b'\n'];
        std::fs::write(&path, original).unwrap();

        let result = append_snapshot_rows_at(
            &path,
            &[json!({
                "account": "a", "sender": "s", "subject": "x", "date": "2026-06-12"
            })],
        );

        assert!(result.is_err(), "unreadable UTF-8 source must fail closed");
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn concurrent_snapshot_updates_are_serialized() {
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|sender| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    append_snapshot_rows_at(
                        &path,
                        &[json!({
                            "account": "a", "sender": sender, "subject": "x",
                            "date": "2026-06-12", "unread": true
                        })],
                    )
                    .unwrap();
                })
            })
            .collect();
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 2);
        assert!(raw.contains("\"sender\":\"one\""));
        assert!(raw.contains("\"sender\":\"two\""));
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_and_lock_files_are_owner_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.jsonl");
        append_snapshot_rows_at(
            &path,
            &[json!({
                "account": "a", "sender": "s", "subject": "x", "date": "2026-06-12"
            })],
        )
        .unwrap();

        let snapshot_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let lock_mode = std::fs::metadata(path.with_extension("jsonl.lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(snapshot_mode, 0o600);
        assert_eq!(lock_mode, 0o600);
    }

    #[test]
    fn failed_atomic_replace_removes_private_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("headers.jsonl");
        std::fs::create_dir(&target).unwrap();

        assert!(atomic_private_write(&target, b"replacement\n").is_err());
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["headers.jsonl"]);
        assert!(target.is_dir(), "failed replace must preserve the target");
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
    }

    #[test]
    fn drafts_carry_no_signature_when_the_install_has_no_identity() {
        // A draft is outgoing text written on the user's behalf. With no
        // per-install identity there is no name to sign, and the binary must
        // not supply one of its own.
        let row = json!({"sender": "a@example.com", "subject": "Invoice"});
        let template = template_draft_with(&row, None);
        let prompt = draft_prompt_with(&row, None);
        // The greeting itself contains an em dash, so assert on the tail:
        // an unsigned draft ends at the sentence, with no sign-off appended.
        assert!(
            template.trim_end().ends_with("shortly."),
            "unsigned template gained a sign-off: {template}"
        );
        assert!(prompt.contains("Do not add a signature"));
    }

    #[test]
    fn drafts_sign_with_the_installs_own_identity() {
        let row = json!({"sender": "a@example.com", "subject": "Invoice"});
        assert!(template_draft_with(&row, Some("Alex")).ends_with("— Alex"));
        assert!(draft_prompt_with(&row, Some("Alex")).contains("Sign as Alex."));
    }

    #[test]
    fn routed_draft_prompt_contains_metadata_but_never_message_body() {
        let row = json!({
            "sender": "vendor@example.com",
            "subject": "Invoice follow-up",
            "body": "BODY_MUST_NEVER_ENTER_MODEL_PROMPT",
        });
        let prompt = draft_prompt(&row);
        assert!(prompt.contains("vendor@example.com"));
        assert!(prompt.contains("Invoice follow-up"));
        assert!(!prompt.contains("BODY_MUST_NEVER_ENTER_MODEL_PROMPT"));
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
        let row =
            json!({"sender": "noreply@x.com", "subject": "w", "score": -2, "action": "digest"});
        let suggestion = suggested_action(&row);
        assert_eq!(suggestion["action"], "delete");
        assert!(suggestion["reason"]
            .as_str()
            .unwrap()
            .contains("not staged"));
    }
}
