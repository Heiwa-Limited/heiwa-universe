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
us=$(printf '\037')
rs=$(printf '\036')
if [ "${FIXTURE_PHASE:-live}" = "closed" ]; then
  printf 'S%smail_not_running' "$us"
  exit 0
fi
if [ "${FIXTURE_PHASE:-live}" = "fail" ]; then
  exit 1
fi
printf 'A%salpha%s1788220800%s1789430399%s2%s2%s' "$us" "$us" "$us" "$us" "$us" "$rs"
if [ "${FIXTURE_PHASE:-live}" != "changed" ]; then
  printf 'R%salpha%sINBOX%sada@example.com%skeep%s1789387200%s1%s<keep@example.com>%s' "$us" "$us" "$us" "$us" "$us" "$us" "$us" "$rs"
  printf 'R%salpha%sINBOX%sada@example.com%sgone%s1789383600%s1%s<gone@example.com>%s' "$us" "$us" "$us" "$us" "$us" "$us" "$us" "$rs"
else
  printf 'R%salpha%sINBOX%sada@example.com%skeep%s1789387200%s0%s<keep@example.com>%s' "$us" "$us" "$us" "$us" "$us" "$us" "$us" "$rs"
fi
printf 'A%sbeta%s1788220800%s1789430399%s1%s1%s' "$us" "$us" "$us" "$us" "$us" "$rs"
printf 'R%sbeta%sINBOX%sbob@example.com%sbeta%s1789380000%s1%s<beta@example.com>%s' "$us" "$us" "$us" "$us" "$us" "$us" "$us" "$rs"
printf 'T%s2%s2%s2' "$us" "$us" "$us"
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
    assert_eq!(first["sources"][0]["complete"], true);
    assert_eq!(first["sources"][0]["accounts_total"], 2);
    assert_eq!(first["sources"][0]["accounts_scanned"], 2);
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
fn failed_attempt_is_recorded_and_background_backs_off_without_relaunch() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let log = root.path().join("fixture.log");
    fs::create_dir_all(&state).unwrap();
    let bridge = fixture(root.path());

    let _ = run(&state, &bridge, &log, "live", &[]);
    let failed = run(&state, &bridge, &log, "fail", &[]);
    assert_eq!(failed["status"], "error");
    let state_file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state.join("mail/apple_sync.json")).unwrap())
            .unwrap();
    assert!(state_file["error"]
        .as_str()
        .unwrap()
        .contains("could not be read"));

    let backoff = run(
        &state,
        &bridge,
        &log,
        "live",
        &["--if-running", "--if-stale", "180"],
    );
    assert_eq!(backoff["status"], "backoff");
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
