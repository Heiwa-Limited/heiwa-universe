//! Integration tests for `heiwa calendar sync`.
//! Dry-run stays hermetic: it must not touch ~/.heiwa state.

use std::process::Command;

#[test]
fn calendar_sync_dry_run_reports_apple_google_read_model_plan_without_writing_state() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .args(["calendar", "sync", "--source", "all", "--dry-run", "--json"])
        .output()
        .expect("binary runs");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("calendar sync dry-run returns JSON");

    assert_eq!(payload["command"], "calendar sync");
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["policy"], "read-model-before-external-writes");
    assert_eq!(payload["sources"]["apple"]["selected"], true);
    assert_eq!(payload["sources"]["google"]["selected"], true);
    assert_eq!(
        payload["sources"]["google"]["sync_semantics"],
        "full sync stores nextSyncToken; incremental sync handles 410 Gone with scoped wipe + full resync"
    );
    // Normalize separators: Windows joins with backslashes.
    let snapshot = payload["snapshot"]
        .as_str()
        .expect("snapshot path")
        .replace('\\', "/");
    assert!(snapshot.ends_with(".heiwa/state/calendar/events.jsonl"));
    assert!(
        !home.path().join(".heiwa").exists(),
        "dry-run must not write Heiwa state"
    );
}
