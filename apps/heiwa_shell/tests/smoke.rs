use std::process::Command;

#[test]
fn test_heiwa_help() {
    let output = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--", "--help"])
        .output()
        .expect("failed to execute process");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Heiwa AI runtime and shell"));
}
