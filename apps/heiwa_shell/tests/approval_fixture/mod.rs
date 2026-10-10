//! Shared synthetic credential fixture for approval integration tests.
#![allow(dead_code)]
pub fn provision_credential(home: &std::path::Path) {
    provision_credential_with_token(home, "integration-test-credential-0123456789");
}

pub fn provision_credential_with_token(home: &std::path::Path, token: &str) {
    let directory = home.join(".heiwa/secrets");
    std::fs::create_dir_all(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = directory.join("machine_auth_token");
    std::fs::write(&path, token).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
