use std::process::Command;

#[test]
fn shell_boots_without_stdb_env_vars() {
    let output = Command::new("cargo")
        .args(["run", "-p", "heiwa-shell", "--bin", "heiwa", "--", "doctor"])
        .env_remove("STDB_URL")
        .env_remove("STDB_TOKEN")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Heiwa Doctor Report"));
}
