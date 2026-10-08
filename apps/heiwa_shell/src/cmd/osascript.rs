use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

/// Run an AppleScript program through the local osascript executable. Tests
/// and connector fixtures can replace the executable with `HEIWA_OSASCRIPT`;
/// production still resolves to `/usr/bin/osascript`.
pub(crate) fn run_applescript(
    script: &str,
    args: &[String],
    timeout: std::time::Duration,
) -> Result<Vec<u8>> {
    let executable = std::env::var_os("HEIWA_OSASCRIPT")
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "/usr/bin/osascript".into());
    let mut command = Command::new(&executable);
    command.args(["-e", script]).args(args);
    heiwa_core::subprocess::bounded_output(&mut command, timeout, 4 * 1024 * 1024).map_err(
        |error| {
            anyhow!(
                "osascript at {} failed: {error}",
                Path::new(&executable).display()
            )
        },
    )
}
