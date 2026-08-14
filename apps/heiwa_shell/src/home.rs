//! Shell-side accessors for the per-user state layout.
//!
//! Resolution itself belongs to `heiwa_config::HeiwaPaths` (ConfigRoot), which
//! owns the env precedence and is the only code that knows where user state
//! lives. These are thin named accessors so shell call sites read well; they
//! deliberately hold no policy of their own.

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
