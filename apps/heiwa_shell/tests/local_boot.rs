use std::process::Command;

fn write_fake_executable(path: &std::path::Path) {
    std::fs::write(path, "#!/bin/sh\nexit 0\n").expect("write fake provider CLI");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("make fake provider CLI executable");
    }
}

#[test]
fn shell_boots_local_first() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let evidence = root.path().join("evidence");
    let state = root.path().join("state");
    for path in [&home, &evidence, &state] {
        std::fs::create_dir_all(path).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_EVIDENCE_DIR", evidence)
        .env("HEIWA_STATE_DIR", state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", "/usr/bin:/bin")
        .arg("doctor")
        .output()
        .expect("failed to execute");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Heiwa Doctor Report"));
}

#[test]
fn doctor_separates_provider_account_availability_from_cli_auth() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let evidence = root.path().join("evidence");
    let state = root.path().join("state");
    let bin = root.path().join("bin");
    let heiwa_home = home.join(".heiwa");
    for path in [&home, &evidence, &state, &bin, &heiwa_home] {
        std::fs::create_dir_all(path).unwrap();
    }
    write_fake_executable(&bin.join("gemini"));
    std::fs::write(
        heiwa_home.join("provider_connections.json"),
        r#"["gemini"]"#,
    )
    .unwrap();
    std::fs::write(
        heiwa_home.join("accounts.json"),
        r#"{
          "accounts": [{
            "account_id": "google-cli",
            "provider": "google-gemini-cli",
            "credential": {"kind": "oauth_cli", "binary": "gemini"},
            "rate_group": "google_gemini_cli",
            "status": {"error": "IneligibleTierError"},
            "models": []
          }]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_EVIDENCE_DIR", evidence)
        .env("HEIWA_STATE_DIR", state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env(
            "PATH",
            std::env::join_paths([&bin]).expect("build hermetic provider PATH"),
        )
        .arg("doctor")
        .output()
        .expect("failed to execute doctor");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Provider Accounts:"), "{stdout}");
    assert!(
        stdout.contains("google-cli") && stdout.contains("IneligibleTierError"),
        "{stdout}"
    );
    assert!(
        stdout.contains("CLI Discovery (auth presence only):"),
        "{stdout}"
    );
    assert!(
        stdout.contains("gemini:") && stdout.contains("connected"),
        "{stdout}"
    );
}

#[test]
fn doctor_points_antigravity_auth_to_the_provider_owned_surface() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let evidence = root.path().join("evidence");
    let state = root.path().join("state");
    for path in [&home, &evidence, &state] {
        std::fs::create_dir_all(path).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_EVIDENCE_DIR", evidence)
        .env("HEIWA_STATE_DIR", state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", "/usr/bin:/bin")
        .arg("doctor")
        .output()
        .expect("failed to execute doctor");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("connect Antigravity in its provider-owned surface"),
        "{stdout}"
    );
    assert!(!stdout.contains("install antigravity CLI"), "{stdout}");
}

#[test]
fn doctor_reports_heiwa_launch_agents_without_changing_them() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let evidence = root.path().join("evidence");
    let state = root.path().join("state");
    let agents = home.join("Library/LaunchAgents");
    for path in [&home, &evidence, &state, &agents] {
        std::fs::create_dir_all(path).unwrap();
    }
    // A unique label is never loaded or disabled in the real user domain.
    let label = format!("com.heiwa.doctor-test-{}", std::process::id());
    // Ownership comes from Label, not the filename.
    let plist_path = agents.join("custom-agent.plist");
    std::fs::write(
        agents.join("org.example.other.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>org.example.other</string>
<key>Program</key><string>/usr/bin/true</string></dict></plist>"#,
    )
    .unwrap();
    let missing = root.path().join("missing/daemon.js");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key><array><string>/bin/sh</string><string>{}</string></array>
  <key>EnvironmentVariables</key><dict><key>TOKEN</key><string>hermetic-secret</string></dict>
  <key>RunAtLoad</key><true/>
</dict></plist>"#,
        missing.display()
    );
    std::fs::write(&plist_path, &plist).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", &home)
        .env("HEIWA_EVIDENCE_DIR", evidence)
        .env("HEIWA_STATE_DIR", state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", "/usr/bin:/bin")
        .args(["doctor", "--json"])
        .output()
        .expect("failed to execute doctor");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("hermetic-secret"), "{stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("doctor json");
    let launchd = &report["launchd"];
    if cfg!(target_os = "macos") {
        assert_eq!(launchd["status"], "attention", "{launchd}");
        let agent = launchd["agents"]
            .as_array()
            .and_then(|agents| agents.iter().find(|agent| agent["label"] == label.as_str()))
            .unwrap_or_else(|| panic!("agent reported: {launchd}"));
        assert_eq!(agent["classification"], "reload_risk", "{agent}");
        assert_eq!(agent["loaded"], false);
        assert_eq!(agent["missing_paths"][0], missing.display().to_string());
        assert_eq!(launchd["other_plists"], 1, "{launchd}");
        assert!(!stdout.contains("org.example.other"), "{stdout}");
    } else {
        assert_eq!(launchd["status"], "unsupported", "{launchd}");
    }
    // Report-only: the plist is untouched.
    assert_eq!(std::fs::read_to_string(&plist_path).unwrap(), plist);
}

fn route_event(
    id: &str,
    event_type: heiwa_evidence::OperatorEventType,
    payload: serde_json::Value,
) -> heiwa_evidence::OperatorEvent {
    heiwa_evidence::OperatorEvent {
        schema_version: heiwa_evidence::OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: id.to_string(),
        thread_id: "thread-doctor".to_string(),
        turn_id: Some("turn-doctor".to_string()),
        run_id: None,
        call_id: Some(format!("call-{id}")),
        work_id: None,
        event_type,
        occurred_at: "2026-10-01T09:00:00Z".to_string(),
        actor: heiwa_evidence::OperatorActor {
            kind: "runtime".to_string(),
            id: "model-call-executor".to_string(),
        },
        risk_class: heiwa_evidence::OperatorRisk::Low,
        sensitivity: heiwa_evidence::OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

#[test]
fn doctor_reports_dated_provider_executions_per_channel() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let evidence = root.path().join("evidence");
    let state = root.path().join("state");
    let bin = root.path().join("bin");
    for path in [&home, &evidence, &state, &bin] {
        std::fs::create_dir_all(path).unwrap();
    }
    write_fake_executable(&bin.join("gemini"));
    write_fake_executable(&bin.join("codex"));
    let journal = heiwa_evidence::OperatorJournal::new(evidence.clone()).unwrap();
    use heiwa_evidence::OperatorEventType::{RouteCompleted, RouteFailed};
    journal
        .append(&route_event(
            "ollama-ok",
            RouteCompleted,
            serde_json::json!({"attempt": 1, "provider": "ollama", "model": "qwen",
                "channel": {"kind": "local_runtime", "binary": "ollama"}}),
        ))
        .unwrap();
    journal
        .append(&route_event(
            "gemini-auth",
            RouteFailed,
            serde_json::json!({"attempt": 1, "provider": "gemini", "model": "flash",
                "failure_class": "authentication", "failure_origin": "provider",
                "provider_invoked": true, "message": "DOCTOR-MESSAGE-SENTINEL",
                "channel": {"kind": "oauth_cli", "binary": "gemini"}}),
        ))
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", &home)
        .env("HEIWA_EVIDENCE_DIR", &evidence)
        .env("HEIWA_STATE_DIR", &state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", std::env::join_paths([&bin]).unwrap())
        .args(["doctor", "--json"])
        .output()
        .expect("failed to execute doctor");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("DOCTOR-MESSAGE-SENTINEL"), "{stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let observations = &report["provider_executions"];
    assert_eq!(
        observations["evidence"]["state"], "complete",
        "{observations}"
    );
    assert_eq!(observations["evidence"]["source"], "journal_tail");
    let rows = observations["observations"].as_array().unwrap();
    let gemini = rows.iter().find(|row| row["provider"] == "gemini").unwrap();
    assert_eq!(gemini["latest"], "failure");
    assert_eq!(gemini["last_failure"]["class"], "authentication");
    assert_eq!(gemini["last_failure"]["at"], "2026-10-01T09:00:00Z");
    assert!(rows.iter().all(|row| row["provider"] != "codex"));

    let text = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", &home)
        .env("HEIWA_EVIDENCE_DIR", &evidence)
        .env("HEIWA_STATE_DIR", &state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", std::env::join_paths([&bin]).unwrap())
        .arg("doctor")
        .output()
        .expect("failed to execute doctor");
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.contains("Execution: last failure authentication"),
        "{text}"
    );
    assert!(
        text.contains("a past success does not prove current readiness"),
        "{text}"
    );
    // A signed-in CLI with no recorded execution is not presented as working.
    let codex_line = text
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("codex:"))
        .nth(1)
        .unwrap_or_default();
    assert!(codex_line.contains("Execution: none observed"), "{text}");
}

#[test]
fn doctor_reads_provider_evidence_without_creating_a_journal() {
    let root = tempfile::tempdir().expect("hermetic doctor root");
    let home = root.path().join("home");
    let state = root.path().join("state");
    let evidence = root.path().join("never-created");
    for path in [&home, &state] {
        std::fs::create_dir_all(path).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", &home)
        .env("HEIWA_EVIDENCE_DIR", &evidence)
        .env("HEIWA_STATE_DIR", &state)
        .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
        .env("PATH", "/usr/bin:/bin")
        .args(["doctor", "--json"])
        .output()
        .expect("failed to execute doctor");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["provider_executions"]["evidence"]["state"], "empty");
    assert!(!evidence.exists(), "doctor created the evidence root");
}
