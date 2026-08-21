//! Integration tests for `heiwa schedule` — run the real binary against a
//! temp HOME so ~/.heiwa/state is never touched. `--at` keeps the test
//! hermetic (no Python subprocess).

use std::process::Command;

fn run_schedule(home: &std::path::Path, extra: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_heiwa"));
    cmd.env("HOME", home)
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        .arg("schedule")
        .args(["call", "mom", "--at", "2026-06-19T15:00", "--json"])
        .args(extra);
    let out = cmd.output().expect("binary runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn dry_run_writes_nothing_and_emits_payload_shape() {
    let home = tempfile::tempdir().unwrap();
    let (ok, stdout, stderr) = run_schedule(home.path(), &["--dry-run"]);
    assert!(ok, "stderr: {stderr}");

    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json output");
    assert_eq!(payload["command"], "schedule");
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["parsed"]["confidence"], 1.0);
    assert_eq!(payload["hold_request"]["date"], "2026-06-19");
    assert_eq!(
        payload["approval_request"]["schema_version"],
        "operator_dispatch_request_v1"
    );
    assert!(
        !home.path().join(".heiwa").exists(),
        "dry-run must not write state"
    );
}

#[test]
fn real_run_stages_approval_and_hold_under_home() {
    let home = tempfile::tempdir().unwrap();
    let (ok, stdout, stderr) = run_schedule(home.path(), &[]);
    assert!(ok, "stderr: {stderr}");

    let payload: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json output");
    let request_id = payload["approval_request"]["request_id"]
        .as_str()
        .expect("request id");
    let hold_id = payload["hold"]["id"].as_str().expect("hold id");
    assert_eq!(payload["hold"]["external_promotion"], "local_only");
    assert!(payload["hold"]["promotion"].is_null());

    let state = home.path().join(".heiwa").join("state");
    assert!(state
        .join("dispatch")
        .join("requests")
        .join(format!("{request_id}.json"))
        .exists());
    assert!(state
        .join("calendar")
        .join("holds")
        .join(format!("{hold_id}.json"))
        .exists());
    assert!(state
        .join("calendar")
        .join("receipts")
        .join(format!("rcpt-{hold_id}.json"))
        .exists());

    // The staged request must be visible to the existing approvals surface.
    let out = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        .args(["approvals", "list", "--json"])
        .output()
        .expect("approvals list runs");
    let listing: serde_json::Value = serde_json::from_slice(&out.stdout).expect("approvals json");
    let pending = listing["pending_summary"]
        .as_array()
        .expect("pending array");
    assert!(
        pending.iter().any(|p| p["id"] == request_id),
        "schedule request not visible in approvals list"
    );
}

#[test]
fn unparseable_without_at_fails_cleanly() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env("HOME", home.path())
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        // HEIWA_PARSE_TIME points nowhere: simulates an install without the SDK.
        .env("HEIWA_PARSE_TIME", "/nonexistent/parse_time.py")
        .env("HEIWA_PYTHON", "/nonexistent/python")
        .args(["schedule", "no", "time", "in", "this"])
        .output()
        .expect("binary runs");
    // Either the parser resolves (dev machine) and reports low confidence, or
    // it doesn't resolve (clean install) — both must fail with the --at hint.
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--at"), "stderr should hint --at: {stderr}");
    assert!(!home.path().join(".heiwa").exists());
}
