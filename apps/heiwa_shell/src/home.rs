//! Canonical home-directory resolution for the `heiwa` shell.
//!
//! `HOME` wins over the platform lookup so hermetic tests (and any sandboxed
//! run) can redirect all `~/.heiwa` state with one env var. On Unix this is
//! what `dirs::home_dir()` reads anyway; on Windows `dirs` ignores `HOME`
//! (it uses `USERPROFILE`), which previously let sandboxed state leak into
//! the real user profile.

use std::path::PathBuf;

pub fn heiwa_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

/// Resolve the canonical hot-state root used by every shell command.
pub fn heiwa_state_dir() -> PathBuf {
    std::env::var_os("HEIWA_STATE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            heiwa_home()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".heiwa")
                .join("state")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_env_wins_over_platform_lookup() {
        // Serialize env mutation within this test only; no other test in this
        // binary touches HOME.
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", "/tmp/heiwa-home-test");
        assert_eq!(heiwa_home(), Some(PathBuf::from("/tmp/heiwa-home-test")));
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}
