//! Canonical home-directory resolution for the `heiwa` shell.
//!
//! `HOME` wins over the platform lookup so hermetic tests (and any sandboxed
//! run) can redirect all `~/.heiwa` state with one env var. On Unix this is
//! what `dirs::home_dir()` reads anyway; on Windows `dirs` ignores `HOME`
//! (it uses `USERPROFILE`), which previously let sandboxed state leak into
//! the real user profile.

use std::path::PathBuf;

pub fn heiwa_home() -> Option<PathBuf> {
    Some(heiwa_config::HeiwaPaths::resolve().home_dir)
}

/// Resolve the canonical Heiwa runtime root.
pub fn heiwa_runtime_dir() -> PathBuf {
    heiwa_config::HeiwaPaths::resolve().runtime_root
}

/// Resolve the canonical hot-state root used by every shell command.
pub fn heiwa_state_dir() -> PathBuf {
    heiwa_config::HeiwaPaths::resolve().state_dir
}
