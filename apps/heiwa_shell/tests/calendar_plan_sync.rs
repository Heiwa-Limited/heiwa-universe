//! Calendar plan sync against a hermetic EventKit helper fixture.
//!
//! The real binary owns validation, diffing, staging, approval, receipts and
//! journal replay. Only the native helper is replaced, so CI never touches a
//! user's calendar. The fixture records every request so the tests can prove
//! that one approval produces one batch write containing only the delta.

#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PLAN_ID: &str = "devon-2026-09";

fn marker(key: &str) -> String {
    format!("heiwa://calendar/plan/{PLAN_ID}/{key}")
}

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    evidence: PathBuf,
    helper: PathBuf,
    log: PathBuf,
    scan: PathBuf,
    apply: PathBuf,
    plan: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp fixture root");
        let home = root.path().join("home");
        fs::create_dir_all(home.join(".heiwa")).expect("create Heiwa root");
        fs::write(
            home.join(".heiwa/local-identity.json"),
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "installation_id": "install-calendar-plan-test",
                "display_name": "Ada",
                "created_at": "2026-09-13T00:00:00Z"
            }))
            .unwrap(),
        )
        .expect("write local identity");
        let helper = root.path().join("fixture-apple-resources");
        fs::write(
            &helper,
            r#"#!/bin/sh
set -eu
request="$1"
printf '%s\n' "$request" >> "$HEIWA_PLAN_FIXTURE_LOG"
case "$request" in
  *'"operation":"list"'*)
    printf '%s' '{"schema_version":1,"calendars":[{"id":"cal-life","name":"Life","source":"iCloud","writable":true}]}'
    ;;
  *'"operation":"plan_scan"'*)
    cat "$HEIWA_PLAN_FIXTURE_SCAN"
    ;;
  *'"operation":"plan_apply"'*)
    cat "$HEIWA_PLAN_FIXTURE_APPLY"
    ;;
  *)
    printf '%s' '{"schema_version":1,"error":"invalid_request"}'
    exit 1
    ;;
esac
"#,
        )
        .expect("write helper fixture");
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).unwrap();
        Self {
            evidence: root.path().join("evidence"),
            log: root.path().join("helper.log"),
            scan: root.path().join("scan.json"),
            apply: root.path().join("apply.json"),
            plan: root.path().join("plan.json"),
            helper,
            home,
            _root: root,
        }
    }

    fn heiwa(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_heiwa"));
        command
            .env("HOME", &self.home)
            .env("HEIWA_EVIDENCE_DIR", &self.evidence)
            .env("HEIWA_APPLE_RESOURCES_HELPER", &self.helper)
            .env("HEIWA_PLAN_FIXTURE_LOG", &self.log)
            .env("HEIWA_PLAN_FIXTURE_SCAN", &self.scan)
            .env("HEIWA_PLAN_FIXTURE_APPLY", &self.apply)
            .env_remove("HEIWA_HOME")
            .env_remove("HEIWA_STATE_DIR");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.heiwa().args(args).output().expect("run heiwa")
    }

    fn ok_json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "heiwa {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output")
    }

    fn connect(&self) {
        let output = self.run(&["connect", "apple-calendar", "--authorize"]);
        assert!(
            output.status.success(),
            "connect stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_plan(&self, events: Value) {
        fs::write(
            &self.plan,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "plan_id": PLAN_ID,
                "window": {"start": "2026-09-01T00:00:00-07:00", "end": "2026-10-01T00:00:00-07:00"},
                "calendars": ["Life"],
                "events": events,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_scan(&self, marked: Value) {
        fs::write(
            &self.scan,
            json!({"schema_version": 1, "marked": marked, "unmarked": []}).to_string(),
        )
        .unwrap();
    }

    fn helper_calls(&self, operation: &str) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains(&format!("\"operation\":\"{operation}\"")))
            .map(str::to_string)
            .collect()
    }

    fn plan_path(&self) -> &str {
        self.plan.to_str().unwrap()
    }
}

fn event(key: &str, title: &str, start: &str, end: &str) -> Value {
    json!({"key": key, "calendar": "Life", "title": title, "start": start, "end": end})
}

fn owned(key: &str, title: &str, start: &str, end: &str, id: &str) -> Value {
    json!({"marker": marker(key), "external_id": id, "calendar": "Life", "title": title,
           "start": start, "end": end, "notes": "", "location": "", "tentative": false})
}

fn contains_file_with(dir: &Path, needle: &str) -> bool {
    walk(dir)
        .iter()
        .any(|path| fs::read_to_string(path).is_ok_and(|text| text.contains(needle)))
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn one_approval_applies_only_the_delta_in_one_batch_with_a_replayable_receipt() {
    let f = Fixture::new();
    f.connect();
    f.write_plan(json!([
        event(
            "lunch",
            "Lunch",
            "2026-09-14T11:15:00-07:00",
            "2026-09-14T12:00:00-07:00"
        ),
        event(
            "dinner",
            "Dinner",
            "2026-09-14T18:00:00-07:00",
            "2026-09-14T19:00:00-07:00"
        ),
        event(
            "open",
            "Open",
            "2026-09-15T13:00:00-07:00",
            "2026-09-15T15:00:00-07:00"
        ),
    ]));
    // What EventKit reports today: lunch unchanged (UTC spelling), dinner renamed, one stale block.
    f.write_scan(json!([
        owned(
            "lunch",
            "Lunch",
            "2026-09-14T18:15:00Z",
            "2026-09-14T19:00:00Z",
            "ek-lunch"
        ),
        owned(
            "dinner",
            "Supper",
            "2026-09-15T01:00:00Z",
            "2026-09-15T02:00:00Z",
            "ek-dinner"
        ),
        owned(
            "stale",
            "Old block",
            "2026-09-16T17:00:00Z",
            "2026-09-16T18:00:00Z",
            "ek-stale"
        ),
    ]));

    let diff = f.ok_json(&["calendar", "plan", "diff", f.plan_path(), "--json"]);
    assert_eq!(
        diff["counts"],
        json!({"create": 1, "update": 1, "delete": 1, "adopt": 0, "unchanged": 1})
    );
    assert!(f.helper_calls("plan_apply").is_empty(), "diff never writes");

    let staged = f.ok_json(&["calendar", "plan", "stage", f.plan_path(), "--json"]);
    assert_eq!(staged["in_sync"], false);
    let request_id = staged["approval_request"]["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(staged["approval_request"]["risk_tier"], "T2");
    assert_eq!(
        staged["approval_request"]["intent"]["changes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        f.helper_calls("plan_apply").is_empty(),
        "staging never writes"
    );

    fs::write(
        &f.apply,
        json!({"schema_version": 1, "applied": [
            {"op": "delete", "marker": marker("stale"), "external_id": "ek-stale"},
            {"op": "update", "marker": marker("dinner"), "external_id": "ek-dinner"},
            {"op": "create", "marker": marker("open"), "external_id": "ek-open"},
        ]})
        .to_string(),
    )
    .unwrap();
    let decided = f.ok_json(&["approvals", "decide", &request_id, "--approve", "--json"]);
    let applied = &decided["decision"]["applied_effects"][0];
    assert_eq!(applied["kind"], "apple_calendar_plan_apply");
    let receipt_id = applied["receipt_id"].as_str().unwrap().to_string();

    let writes = f.helper_calls("plan_apply");
    assert_eq!(writes.len(), 1, "one approval, one batch write");
    assert!(
        writes[0].contains("ek-stale")
            && writes[0].contains("ek-dinner")
            && writes[0].contains(&marker("open"))
    );
    assert!(
        !writes[0].contains("ek-lunch"),
        "unchanged events are not rewritten"
    );

    let receipt_path = f
        .home
        .join(format!(".heiwa/state/calendar/receipts/{receipt_id}.json"));
    let receipt: Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("file receipt")).unwrap();
    assert_eq!(receipt["kind"], "calendar_plan_applied");
    assert_eq!(receipt["approval_id"], request_id);
    assert_eq!(receipt["changes"].as_array().unwrap().len(), 3);
    assert!(
        contains_file_with(&f.evidence, &receipt_id),
        "receipt is journaled"
    );

    // Replaying the decision returns the recorded outcome without writing again.
    f.ok_json(&["approvals", "decide", &request_id, "--approve", "--json"]);
    assert_eq!(f.helper_calls("plan_apply").len(), 1);

    // Once Calendar matches the plan, staging has nothing to approve.
    f.write_scan(json!([
        owned(
            "lunch",
            "Lunch",
            "2026-09-14T18:15:00Z",
            "2026-09-14T19:00:00Z",
            "ek-lunch"
        ),
        owned(
            "dinner",
            "Dinner",
            "2026-09-15T01:00:00Z",
            "2026-09-15T02:00:00Z",
            "ek-dinner"
        ),
        owned(
            "open",
            "Open",
            "2026-09-15T20:00:00Z",
            "2026-09-15T22:00:00Z",
            "ek-open"
        ),
    ]));
    let again = f.ok_json(&["calendar", "plan", "stage", f.plan_path(), "--json"]);
    assert_eq!(again["in_sync"], true);
}

#[test]
fn approval_refuses_to_write_when_the_calendar_drifted_after_staging() {
    let f = Fixture::new();
    f.connect();
    f.write_plan(json!([event(
        "lunch",
        "Lunch",
        "2026-09-14T11:15:00-07:00",
        "2026-09-14T12:00:00-07:00"
    )]));
    f.write_scan(json!([]));
    let staged = f.ok_json(&["calendar", "plan", "stage", f.plan_path(), "--json"]);
    let request_id = staged["approval_request"]["request_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Someone edits the calendar before the approval lands.
    f.write_scan(json!([owned(
        "lunch",
        "Lunch (moved)",
        "2026-09-14T19:15:00Z",
        "2026-09-14T20:00:00Z",
        "ek-lunch"
    )]));
    let output = f.run(&["approvals", "decide", &request_id, "--approve", "--json"]);
    assert!(!output.status.success(), "drifted plan must not apply");
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed since this plan was staged"));
    assert!(
        f.helper_calls("plan_apply").is_empty(),
        "nothing was written"
    );
}

#[test]
fn invalid_plans_are_rejected_before_calendar_is_read() {
    let f = Fixture::new();
    f.connect();
    f.write_plan(json!([
        event(
            "dup",
            "Lunch",
            "2026-09-14T11:15:00-07:00",
            "2026-09-14T12:00:00-07:00"
        ),
        event(
            "dup",
            "Dinner",
            "2026-09-14T18:00:00-07:00",
            "2026-09-14T19:00:00-07:00"
        ),
    ]));
    let output = f.run(&["calendar", "plan", "diff", f.plan_path()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate event key"));
    assert!(f.helper_calls("plan_scan").is_empty());
}
