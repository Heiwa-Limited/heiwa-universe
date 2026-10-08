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
