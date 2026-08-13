use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn heiwa_with_home(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .args(args)
        .env("HOME", home)
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        .output()
        .expect("run heiwa")
}

#[test]
fn auto_status_json_reports_empty_local_store() {
    let home = tempdir().unwrap();
    let output = heiwa_with_home(home.path(), &["auto", "status", "--json"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["command"], "auto status");
    assert_eq!(payload["automation_count"], 0);
    assert!(payload["db_path"]
        .as_str()
        .unwrap()
        .contains("automations.sqlite3"));
}

#[test]
fn auto_create_cron_persists_active_automation() {
    let home = tempdir().unwrap();
    let output = heiwa_with_home(
        home.path(),
        &[
            "auto",
            "create",
            "--name",
            "Daily brief",
            "--prompt",
            "Summarize today",
            "--cron",
            "0 9 * * *",
            "--timezone",
            "UTC",
            "--active",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["command"], "auto create");
    assert_eq!(payload["automation"]["name"], "Daily brief");
    assert_eq!(payload["automation"]["status"], "active");
    assert!(home
        .path()
        .join(".heiwa/state/automations/automations.sqlite3")
        .exists());

    let list = heiwa_with_home(home.path(), &["auto", "list", "--json"]);
    assert!(
        list.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_payload: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list_payload["automations"].as_array().unwrap().len(), 1);
}

#[test]
fn auto_trigger_queues_execution_and_writes_receipt() {
    let home = tempdir().unwrap();
    let create = heiwa_with_home(
        home.path(),
        &[
            "auto",
            "create",
            "--name",
            "Manual check",
            "--prompt",
            "Check local runtime",
            "--active",
            "--json",
        ],
    );
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let create_payload: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = create_payload["automation"]["id"].as_str().unwrap();

    let trigger = heiwa_with_home(home.path(), &["auto", "trigger", id, "--json"]);
    assert!(
        trigger.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&trigger.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&trigger.stdout).unwrap();
    assert_eq!(payload["command"], "auto trigger");
    assert_eq!(payload["result"]["queued"], true);
    let execution_id = payload["result"]["execution_id"].as_str().unwrap();
    let receipt = home.path().join(format!(
        ".heiwa/state/automations/receipts/rcpt-{execution_id}-queued.json"
    ));
    assert!(receipt.exists(), "missing receipt: {}", receipt.display());
    let raw = fs::read_to_string(receipt).unwrap();
    assert!(raw.contains("automation_execution_event"));
}

#[test]
fn auto_run_executes_deterministic_prompt_through_operator_runtime() {
    let home = tempdir().unwrap();
    let evidence = home.path().join(".heiwa/evidence");
    let create = heiwa_with_home(
        home.path(),
        &[
            "auto", "create", "--name", "Greeting", "--prompt", "hi", "--active", "--json",
        ],
    );
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let create_payload: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let id = create_payload["automation"]["id"].as_str().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .args(["auto", "run", id, "--json"])
        .env("HOME", home.path())
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        .env("HEIWA_EVIDENCE_DIR", &evidence)
        .output()
        .expect("run deterministic automation");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["command"], "auto run");
    assert_eq!(payload["execution"]["status"], "completed");
    let execution_id = payload["execution"]["id"].as_str().unwrap();
    assert!(home
        .path()
        .join(format!(
            ".heiwa/state/automations/receipts/rcpt-{execution_id}-completed.json"
        ))
        .exists());
}
