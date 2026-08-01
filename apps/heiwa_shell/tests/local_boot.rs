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
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
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
