//! Integration tests for `heiwa approvals decide` — verify the on-approval
//! wire that flips a draft hold to confirmed (approve) or drops it (deny),
//! and that the calendar-receipt lane records the state change.
//!
//! Hermetic: every test runs the real binary against a temp `HOME` so
//! `~/.heiwa/state/` is never touched. `heiwa schedule --at` keeps the
//! staging path free of the Python parser subprocess.

use std::fs;
use std::path::Path;
use std::process::Command;

fn heiwa() -> Command {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
}

fn stage(home: &Path) -> (String, String) {
    let out = heiwa()
        .env("HOME", home)
        .args([
            "schedule",
            "call mom",
            "--at",
            "2026-06-19T15:00",
            "--json",
        ])
        .output()
        .expect("schedule runs");
    assert!(out.status.success(), "schedule failed: {:?}", out.stderr);
    let payload: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("schedule json");
    let request_id = payload["approval_request"]["request_id"]
        .as_str()
        .expect("request id")
        .to_string();
    let hold_id = payload["hold"]["id"].as_str().expect("hold id").to_string();
    (request_id, hold_id)
}

#[test]
fn dry_run_previews_hold_confirm_effect() {
    let home = tempfile::tempdir().unwrap();
    let (request_id, hold_id) = stage(home.path());

    let out = heiwa()
        .env("HOME", home.path())
        .args(["approvals", "decide", &request_id, "--approve", "--dry-run"])
        .output()
        .expect("decide dry-run runs");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);

    // Effects are visible in the dry-run stdout.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("hold.status: draft -> confirmed"),
        "stdout should preview the hold_confirm effect; got: {stdout}"
    );
    assert!(
        stdout.contains(&hold_id),
        "stdout should reference the target hold; got: {stdout}"
    );

    // No decision file written, hold still draft.
    // (The spine stores decisions at
    // `~/.heiwa/state/dispatch/approvals/decisions/` — see approvals.rs
    // `decisions_dir()`. Approval requests live one level up under
    // `dispatch/requests/`.)
    let decisions = home.path().join(".heiwa/state/dispatch/approvals/decisions");
    assert!(!decisions.exists(), "dry-run must not write a decision");
    let hold: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            home.path()
                .join(".heiwa/state/calendar/holds")
                .join(format!("{hold_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(hold["status"], "draft");
}

#[test]
fn approve_flips_hold_status_and_writes_status_receipt() {
    let home = tempfile::tempdir().unwrap();
    let (request_id, hold_id) = stage(home.path());

    let out = heiwa()
        .env("HOME", home.path())
        .args(["approvals", "decide", &request_id, "--approve"])
        .output()
        .expect("decide approve runs");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("{hold_id} -> status=confirmed")),
        "stdout should report the applied effect; got: {stdout}"
    );

    // Decision file exists with effects + applied_effects.
    // The spine stores decisions at
    // `~/.heiwa/state/dispatch/approvals/decisions/` (see
    // `approvals.rs::decisions_dir()`).
    let decision: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            home.path()
                .join(".heiwa/state/dispatch/approvals/decisions")
                .join(format!("{request_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(decision["outcome"], "approved");
    assert_eq!(decision["effects"].as_array().unwrap().len(), 1);
    assert_eq!(decision["effects"][0]["kind"], "hold_confirm");
    assert_eq!(decision["applied_effects"][0]["kind"], "hold_confirm");

    // Hold JSON now says confirmed, with provenance fields.
    let hold: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            home.path()
                .join(".heiwa/state/calendar/holds")
                .join(format!("{hold_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(hold["status"], "confirmed");
    assert_eq!(hold["confirmed_by_decision"], request_id.as_str());
    assert!(
        hold["confirmed_at"].is_string(),
        "confirmed_at should be set; hold: {hold}"
    );

    // Calendar receipt lane captured the state change.
    let status_receipt_path = home
        .path()
        .join(".heiwa/state/calendar/receipts")
        .join(format!("rcpt-{hold_id}-status-confirmed.json"));
    assert!(
        status_receipt_path.exists(),
        "status-change receipt must be written: {status_receipt_path:?}"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&status_receipt_path).unwrap()).unwrap();
    assert_eq!(receipt["kind"], "calendar_hold_status_changed");
    assert_eq!(receipt["new_status"], "confirmed");
    assert_eq!(receipt["by_decision"], request_id.as_str());
}

#[test]
fn deny_drops_draft_hold_and_writes_drop_receipt() {
    let home = tempfile::tempdir().unwrap();
    let (request_id, hold_id) = stage(home.path());

    let out = heiwa()
        .env("HOME", home.path())
        .args(["approvals", "decide", &request_id, "--deny"])
        .output()
        .expect("decide deny runs");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("{hold_id} dropped (was draft)")),
        "stdout should report the drop; got: {stdout}"
    );

    // Hold file is gone.
    let hold_path = home
        .path()
        .join(".heiwa/state/calendar/holds")
        .join(format!("{hold_id}.json"));
    assert!(!hold_path.exists(), "draft hold must be removed on deny");

    // Drop receipt captured.
    let drop_receipt_path = home
        .path()
        .join(".heiwa/state/calendar/receipts")
        .join(format!("rcpt-{hold_id}-dropped.json"));
    assert!(
        drop_receipt_path.exists(),
        "drop receipt must be written: {drop_receipt_path:?}"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&drop_receipt_path).unwrap()).unwrap();
    assert_eq!(receipt["kind"], "calendar_hold_dropped");
    assert_eq!(receipt["by_decision"], request_id.as_str());

    // Decision file has the drop effect.
    let decision: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            home.path()
                .join(".heiwa/state/dispatch/approvals/decisions")
                .join(format!("{request_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(decision["outcome"], "denied");
    assert_eq!(decision["effects"][0]["kind"], "hold_drop");
    assert_eq!(decision["applied_effects"][0]["kind"], "hold_drop");
}
