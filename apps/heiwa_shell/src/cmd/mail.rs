use anyhow::{anyhow, Result};
use serde_json::json;
use std::path::PathBuf;

const POLICY: &str = "metadata-only-no-body";

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => status(args),
        Some("accounts") => accounts(args),
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

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn print_help() {
    println!("heiwa mail");
    println!();
    println!("Usage:");
    println!("  heiwa mail status [--json]");
    println!("  heiwa mail accounts [--json]");
    println!();
    println!("Policy: {POLICY}.");
    println!("Reads Mail.app data dir layout only; does not parse message bodies.");
}
