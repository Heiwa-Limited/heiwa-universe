use std::process::Command;

#[test]
fn shell_boots_local_first() {
    let output = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .arg("doctor")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Heiwa Doctor Report"));
}
