use anyhow::{anyhow, Result};
use std::process::Command;

/// Run a JavaScript-for-Automation program through the local osascript
/// executable. Tests and connector fixtures can replace the executable with
/// `HEIWA_OSASCRIPT`; production still resolves to `/usr/bin/osascript`.
pub(crate) fn run_jxa(script: &str, args: &[String]) -> Result<Vec<u8>> {
    let executable = std::env::var_os("HEIWA_OSASCRIPT")
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "/usr/bin/osascript".into());
    let mut command = Command::new(&executable);
    command
        .args(["-l", "JavaScript", "-e"])
        .arg(script)
        .args(args);
    heiwa_core::subprocess::bounded_output(
        &mut command,
        std::time::Duration::from_secs(45),
        4 * 1024 * 1024,
    )
    .map_err(|error| {
        anyhow!(
            "osascript at {} failed: {error}",
            Path::new(&executable).display()
        )
    })
}

use std::path::Path;
