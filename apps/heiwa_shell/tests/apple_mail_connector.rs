//! Hermetic Apple Mail connector acceptance through the osascript seam.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(root: &Path) -> PathBuf {
    let path = root.join("fixture-osascript");
    fs::write(
        &path,
        r#"#!/bin/sh
set -eu
printf '%s\n' "${FIXTURE_PHASE:-live}" >> "$FIXTURE_LOG"
if [ "${FIXTURE_PHASE:-live}" = "closed" ]; then
  printf '%s\n' '{"kind":"status","status":"mail_not_running"}'
  exit 0
fi
printf '%s\n' '{"kind":"account","account":"alpha","window_start":"2026-09-01T00:00:00Z","window_end":"2026-09-14T23:59:59Z","matched":2,"kept":1}'
if [ "${FIXTURE_PHASE:-live}" != "changed" ]; then
  printf '%s\n' '{"account":"alpha","mailbox":"INBOX","sender":"ada@example.com","subject":"keep","date":"2026-09-14T12:00:00Z","unread":true,"message_id":"<keep@example.com>"}'
  printf '%s\n' '{"account":"alpha","mailbox":"INBOX","sender":"ada@example.com","subject":"gone","date":"2026-09-14T11:00:00Z","unread":true,"message_id":"<gone@example.com>"}'
else
  printf '%s\n' '{"account":"alpha","mailbox":"INBOX","sender":"ada@example.com","subject":"keep","date":"2026-09-14T12:00:00Z","unread":false,"message_id":"<keep@example.com>"}'
fi
printf '%s\n' '{"kind":"account","account":"beta","window_start":"2026-09-01T00:00:00Z","window_end":"2026-09-14T23:59:59Z","matched":1,"kept":1}'
printf '%s\n' '{"account":"beta","mailbox":"INBOX","sender":"bob@example.com","subject":"beta","date":"2026-09-14T10:00:00Z","unread":true,"message_id":"<beta@example.com>"}'
"#,
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn run(state: &Path, bridge: &Path, log: &Path, phase: &str, args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HEIWA_STATE_DIR", state)
        .env("HEIWA_EVIDENCE_DIR", state.join("evidence"))
        .env("HEIWA_OSASCRIPT", bridge)
        .env("FIXTURE_LOG", log)
        .env("FIXTURE_PHASE", phase)
        .args(["mail", "scan", "--source", "apple", "--json"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn explicit_scan_reconciles_unread_removal_and_preserves_both_accounts() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let log = root.path().join("fixture.log");
    fs::create_dir_all(&state).unwrap();
    let bridge = fixture(root.path());

    let first = run(&state, &bridge, &log, "live", &[]);
    assert_eq!(first["status"], "scanned");
    assert_eq!(first["fetched"], 3);
    assert!(first["snapshot"].as_str().is_some());
    let snapshot = state.join("mail/headers.jsonl");
    let raw = fs::read_to_string(&snapshot).unwrap();
    assert!(raw.contains("mail_"));
    assert!(!raw.contains("keep@example.com"));

    let second = run(&state, &bridge, &log, "changed", &[]);
    assert_eq!(second["status"], "scanned");
    assert_eq!(second["removed"], 1);
    let rows = fs::read_to_string(snapshot).unwrap();
    assert!(rows.contains("\"unread\":false"));
    assert!(rows.contains("\"account\":\"beta\""));
    assert!(!rows.contains("\"subject\":\"gone\""));
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 2);
}

#[test]
fn background_requires_consent_and_does_not_launch_closed_mail() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let log = root.path().join("fixture.log");
    fs::create_dir_all(&state).unwrap();
    let bridge = fixture(root.path());

    let no_consent = run(
        &state,
        &bridge,
        &log,
        "live",
        &["--if-running", "--if-stale", "180"],
    );
    assert_eq!(no_consent["status"], "no_consent");
    assert!(
        !log.exists(),
        "no-consent background scan never invokes osascript"
    );

    let _ = run(&state, &bridge, &log, "live", &[]);
    let closed = run(
        &state,
        &bridge,
        &log,
        "closed",
        &["--if-running", "--if-stale", "0"],
    );
    assert_eq!(closed["status"], "mail_not_running");
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 2);
}
