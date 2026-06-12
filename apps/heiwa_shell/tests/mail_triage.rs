//! Integration tests for `heiwa mail triage` -> approvals decide -> outbox.
//! Hermetic: temp HOME, seeded headers.jsonl, HEIWA_OLLAMA_BASE pointed at a
//! dead port so draft generation falls back to the deterministic template.

use std::process::Command;

fn seed_snapshot(home: &std::path::Path) {
    let mail_dir = home.join(".heiwa/state/mail");
    std::fs::create_dir_all(&mail_dir).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d");
    let rows = [
        serde_json::json!({
            "account": "test", "mailbox": "INBOX", "sender": "vendor@example.com",
            "subject": "Invoice overdue — action required", "date": today.to_string(), "unread": true,
        }),
        serde_json::json!({
            "account": "test", "mailbox": "INBOX", "sender": "newsletter@example.com",
            "subject": "Weekly roundup", "date": "2026-05-01", "unread": false,
        }),
    ];
    let lines: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    std::fs::write(mail_dir.join("headers.jsonl"), lines.join("\n") + "\n").unwrap();
}

fn heiwa(home: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home)
        .env("HEIWA_OLLAMA_BASE", "http://127.0.0.1:1") // force template fallback
        .args(args)
        .output()
        .expect("binary runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn triage_stages_reply_draft_and_is_idempotent() {
    let home = tempfile::tempdir().unwrap();
    seed_snapshot(home.path());

    let (ok, stdout, stderr) = heiwa(home.path(), &["mail", "triage", "--json"]);
    assert!(ok, "stderr: {stderr}");
    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(payload["counts"]["staged"], 1, "one draft-tier message staged");

    // The urgent message produced a pending request with a template draft.
    let item = payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["staged"]["status"] == "pending")
        .expect("staged item");
    let request_id = item["staged"]["request_id"].as_str().unwrap().to_string();
    assert!(request_id.starts_with("req_mail_"));
    let request_path = home
        .path()
        .join(".heiwa/state/dispatch/requests")
        .join(format!("{request_id}.json"));
    let request: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&request_path).unwrap()).unwrap();
    assert_eq!(request["action"], "mail-reply-draft");
    assert_eq!(request["intent"]["draft_source"], "template");
    assert!(request["intent"]["draft"].as_str().unwrap().contains("Devon"));

    // The bulk sender is suggested for delete but never staged.
    let bulk = payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["message"]["sender"] == "newsletter@example.com")
        .expect("bulk item");
    assert_eq!(bulk["suggestion"]["action"], "delete");
    assert!(bulk.get("staged").is_none() || bulk["staged"].is_null());

    // Second run: nothing new staged.
    let (ok, stdout, _) = heiwa(home.path(), &["mail", "triage", "--json"]);
    assert!(ok);
    let second: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(second["counts"]["staged"], 0);
    assert_eq!(second["counts"]["already_staged"], 1);
}

#[test]
fn approve_moves_draft_to_outbox_with_receipt() {
    let home = tempfile::tempdir().unwrap();
    seed_snapshot(home.path());
    let (ok, stdout, _) = heiwa(home.path(), &["mail", "triage", "--json"]);
    assert!(ok);
    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let request_id = payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|i| i["staged"]["request_id"].as_str())
        .unwrap()
        .to_string();

    let (ok, stdout, stderr) = heiwa(
        home.path(),
        &["approvals", "decide", &request_id, "--approve", "--json"],
    );
    assert!(ok, "stderr: {stderr}");
    let decision: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let applied = decision["decision"]["applied_effects"].as_array().unwrap();
    assert_eq!(applied[0]["kind"], "mail_outbox_stage");

    let outbox = home
        .path()
        .join(".heiwa/state/mail/outbox")
        .join(format!("{request_id}.json"));
    let entry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&outbox).unwrap()).unwrap();
    assert_eq!(entry["to"], "vendor@example.com");
    assert_eq!(entry["status"], "ready_for_manual_send");
    assert!(entry["subject"].as_str().unwrap().starts_with("Re: "));
    assert_eq!(entry["external_writes"].as_array().unwrap().len(), 0);

    assert!(home
        .path()
        .join(".heiwa/state/mail/receipts")
        .join(format!("rcpt-{request_id}-outbox.json"))
        .exists());
}

#[test]
fn deny_writes_dismissal_receipt_and_blocks_restaging() {
    let home = tempfile::tempdir().unwrap();
    seed_snapshot(home.path());
    let (ok, stdout, _) = heiwa(home.path(), &["mail", "triage", "--json"]);
    assert!(ok);
    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let request_id = payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|i| i["staged"]["request_id"].as_str())
        .unwrap()
        .to_string();

    let (ok, _, stderr) = heiwa(
        home.path(),
        &["approvals", "decide", &request_id, "--deny", "--json"],
    );
    assert!(ok, "stderr: {stderr}");
    assert!(home
        .path()
        .join(".heiwa/state/mail/receipts")
        .join(format!("rcpt-{request_id}-dismissed.json"))
        .exists());
    assert!(!home
        .path()
        .join(".heiwa/state/mail/outbox")
        .join(format!("{request_id}.json"))
        .exists());

    // Triage again: the denied suggestion must not come back.
    let (ok, stdout, _) = heiwa(home.path(), &["mail", "triage", "--json"]);
    assert!(ok);
    let again: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(again["counts"]["staged"], 0);
}
