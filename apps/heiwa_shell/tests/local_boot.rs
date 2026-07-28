use std::process::Command;

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
