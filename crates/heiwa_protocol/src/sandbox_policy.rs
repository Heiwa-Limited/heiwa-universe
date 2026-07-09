//! Sandbox Policy IR — model-agnostic permission tiers for agent execution.
//!
//! Tiers map to the platform vision (T0 observe → T4 forbidden). Enforcement
//! backends (OS primitives, containers) plug in later; this module is the
//! single source of truth for *what is allowed*, not *how* it is isolated.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{NetworkPolicy, RiskClass};

/// Variable-permission tiers for agent tools on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTier {
    /// Read graph + allowlisted paths only. No shell. No network (or local-only).
    T0Observe = 0,
    /// Project/workspace read-write + local browser profile. Limited network.
    T1Workspace = 1,
    /// Broader host read, limited network, no elevated installers.
    T2HostSafe = 2,
    /// Host shell outside tight workspace — always requires approval + timeout.
    T3Elevated = 3,
    /// Hard deny (credential stores, other users, raw disk).
    T4Forbidden = 4,
}

impl SandboxTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::T0Observe => "t0_observe",
            Self::T1Workspace => "t1_workspace",
            Self::T2HostSafe => "t2_host_safe",
            Self::T3Elevated => "t3_elevated",
            Self::T4Forbidden => "t4_forbidden",
        }
    }

    pub fn max_risk_class(self) -> RiskClass {
        match self {
            Self::T0Observe => RiskClass::HostSafeReadonly,
            Self::T1Workspace | Self::T2HostSafe => RiskClass::HostMutating,
            Self::T3Elevated => RiskClass::SandboxRequired,
            Self::T4Forbidden => RiskClass::SandboxRequired,
        }
    }
}

/// Verdict from the pure policy evaluator (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PolicyVerdict {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

impl PolicyVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    pub fn require_approval(reason: impl Into<String>) -> Self {
        Self::RequireApproval {
            reason: reason.into(),
        }
    }
}

/// Declarative policy bound to a session / worker lease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub tier: SandboxTier,
    pub working_dir: PathBuf,
    pub allowed_read: Vec<PathBuf>,
    pub allowed_write: Vec<PathBuf>,
    pub network: NetworkPolicy,
    /// When true, shell is never auto-allowed even at T3 without explicit lease name.
    pub shell_requires_approval: bool,
    /// Substrings that force T4 deny if present in a path (normalized lowercase).
    pub forbidden_path_markers: Vec<String>,
}

impl SandboxPolicy {
    /// Default developer workspace policy (T1) rooted at `working_dir`.
    pub fn workspace_default(working_dir: impl Into<PathBuf>) -> Self {
        let working_dir = working_dir.into();
        Self {
            tier: SandboxTier::T1Workspace,
            allowed_read: vec![working_dir.clone()],
            allowed_write: vec![working_dir.clone()],
            working_dir,
            network: NetworkPolicy::LocalOnly,
            shell_requires_approval: false,
            forbidden_path_markers: default_forbidden_markers(),
        }
    }

    /// Observe-only (T0).
    pub fn observe_only(working_dir: impl Into<PathBuf>) -> Self {
        let working_dir = working_dir.into();
        Self {
            tier: SandboxTier::T0Observe,
            allowed_read: vec![working_dir.clone()],
            allowed_write: vec![],
            working_dir,
            network: NetworkPolicy::Deny,
            shell_requires_approval: true,
            forbidden_path_markers: default_forbidden_markers(),
        }
    }

    pub fn check_read(&self, path: &Path) -> PolicyVerdict {
        if self.tier == SandboxTier::T4Forbidden {
            return PolicyVerdict::deny("tier_t4_forbidden");
        }
        if is_forbidden_path(path, &self.forbidden_path_markers) {
            return PolicyVerdict::deny("forbidden_path_marker");
        }
        if path_in_any(path, &self.allowed_read) || path_in_any(path, &self.allowed_write) {
            return PolicyVerdict::Allow;
        }
        if matches!(self.tier, SandboxTier::T2HostSafe | SandboxTier::T3Elevated) {
            // Broader host read still blocks forbidden markers; allow other absolute paths.
            if path.is_absolute() {
                return PolicyVerdict::Allow;
            }
        }
        PolicyVerdict::deny("path_not_in_read_scope")
    }

    pub fn check_write(&self, path: &Path) -> PolicyVerdict {
        if self.tier == SandboxTier::T0Observe {
            return PolicyVerdict::deny("t0_observe_no_writes");
        }
        if self.tier == SandboxTier::T4Forbidden {
            return PolicyVerdict::deny("tier_t4_forbidden");
        }
        if is_forbidden_path(path, &self.forbidden_path_markers) {
            return PolicyVerdict::deny("forbidden_path_marker");
        }
        if path_in_any(path, &self.allowed_write) {
            return PolicyVerdict::Allow;
        }
        if self.tier == SandboxTier::T3Elevated {
            return PolicyVerdict::require_approval("t3_elevated_write_outside_workspace");
        }
        PolicyVerdict::deny("path_not_in_write_scope")
    }

    pub fn check_network(&self, host: &str) -> PolicyVerdict {
        match self.network {
            NetworkPolicy::Allow => PolicyVerdict::Allow,
            NetworkPolicy::Deny => PolicyVerdict::deny("network_denied"),
            NetworkPolicy::LocalOnly => {
                let h = host.to_ascii_lowercase();
                if h == "localhost"
                    || h == "127.0.0.1"
                    || h == "::1"
                    || h.ends_with(".local")
                    || h == "0.0.0.0"
                {
                    PolicyVerdict::Allow
                } else {
                    PolicyVerdict::deny("network_local_only")
                }
            }
        }
    }

    pub fn check_shell(&self, command_line: &str) -> PolicyVerdict {
        if self.tier == SandboxTier::T0Observe {
            return PolicyVerdict::deny("t0_observe_no_shell");
        }
        if self.tier == SandboxTier::T4Forbidden {
            return PolicyVerdict::deny("tier_t4_forbidden");
        }
        let lower = command_line.to_ascii_lowercase();
        for marker in DANGEROUS_SHELL_MARKERS {
            if lower.contains(marker) {
                return PolicyVerdict::deny(format!("dangerous_shell_pattern:{marker}"));
            }
        }
        if self.tier == SandboxTier::T3Elevated || self.shell_requires_approval {
            return PolicyVerdict::require_approval("shell_requires_approval");
        }
        if matches!(self.tier, SandboxTier::T1Workspace | SandboxTier::T2HostSafe) {
            return PolicyVerdict::Allow;
        }
        PolicyVerdict::deny("shell_not_permitted")
    }

    pub fn check_tool_risk(&self, risk: RiskClass) -> PolicyVerdict {
        match (self.tier, risk) {
            (SandboxTier::T0Observe, RiskClass::HostSafeReadonly) => PolicyVerdict::Allow,
            (SandboxTier::T0Observe, _) => PolicyVerdict::deny("t0_observe_risk_exceeded"),
            (SandboxTier::T4Forbidden, _) => PolicyVerdict::deny("tier_t4_forbidden"),
            (SandboxTier::T3Elevated, RiskClass::SandboxRequired) => {
                PolicyVerdict::require_approval("sandbox_required_tool")
            }
            (_, RiskClass::SandboxRequired) if self.tier as u8 <= SandboxTier::T2HostSafe as u8 => {
                PolicyVerdict::require_approval("sandbox_required_tool")
            }
            _ => PolicyVerdict::Allow,
        }
    }
}

fn default_forbidden_markers() -> Vec<String> {
    [
        "/.ssh",
        "\\.ssh",
        "/.gnupg",
        "\\.gnupg",
        "id_rsa",
        "id_ed25519",
        "credentials.json",
        "login.keychain",
        "/etc/shadow",
        "ntuser.dat",
        "sam",
        "system32\\config\\sam",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

const DANGEROUS_SHELL_MARKERS: &[&str] = &[
    "rm -rf /",
    "mkfs.",
    "dd if=",
    ":(){",
    "format c:",
    "remove-item -recurse -force c:\\",
    "shutdown /s",
    "reg delete",
];

fn is_forbidden_path(path: &Path, markers: &[String]) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    markers.iter().any(|m| s.contains(&m.to_ascii_lowercase()))
}

fn path_in_any(path: &Path, roots: &[PathBuf]) -> bool {
    let Ok(canon_candidate) = normalize_for_compare(path) else {
        return false;
    };
    for root in roots {
        if let Ok(canon_root) = normalize_for_compare(root) {
            if canon_candidate.starts_with(&canon_root) {
                return true;
            }
        } else if path.starts_with(root) {
            return true;
        }
    }
    false
}

/// Best-effort path normalization without requiring the path to exist.
fn normalize_for_compare(path: &Path) -> Result<PathBuf, ()> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(());
                }
            }
            Component::Normal(c) => out.push(c),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t0_blocks_write_and_shell() {
        let p = SandboxPolicy::observe_only("/home/u/proj");
        assert!(matches!(
            p.check_write(Path::new("/home/u/proj/a.txt")),
            PolicyVerdict::Deny { .. }
        ));
        assert!(matches!(
            p.check_shell("ls"),
            PolicyVerdict::Deny { .. }
        ));
        assert!(p.check_read(Path::new("/home/u/proj/a.txt")).is_allowed());
    }

    #[test]
    fn t1_allows_workspace_write_blocks_ssh() {
        let p = SandboxPolicy::workspace_default("/home/u/proj");
        assert!(p
            .check_write(Path::new("/home/u/proj/src/main.rs"))
            .is_allowed());
        assert!(matches!(
            p.check_read(Path::new("/home/u/.ssh/id_ed25519")),
            PolicyVerdict::Deny { .. }
        ));
    }

    #[test]
    fn local_only_network() {
        let p = SandboxPolicy::workspace_default("/tmp/w");
        assert!(p.check_network("localhost").is_allowed());
        assert!(matches!(
            p.check_network("api.openai.com"),
            PolicyVerdict::Deny { .. }
        ));
    }

    #[test]
    fn dangerous_shell_denied() {
        let p = SandboxPolicy::workspace_default("/tmp/w");
        assert!(matches!(
            p.check_shell("rm -rf /"),
            PolicyVerdict::Deny { .. }
        ));
    }
}
