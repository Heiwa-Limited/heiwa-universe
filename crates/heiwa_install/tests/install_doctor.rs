use heiwa_install::check_installation;

#[test]
fn test_doctor_discovery() {
    let report = check_installation().expect("failed to run doctor");
    
    // In this environment, we expect at least Rust and Python to be present
    assert!(report.rust_version.is_some(), "Rust should be detected");
    assert!(report.python_version.is_some(), "Python should be detected");
}
