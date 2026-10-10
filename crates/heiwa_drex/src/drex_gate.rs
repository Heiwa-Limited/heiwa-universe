use anyhow::{anyhow, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Target risk levels for action routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Verdict for approval gating
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalVerdict {
    AutoApproved,
    AwaitingApproval {
        request_id: String,
        request_path: PathBuf,
    },
}

/// Check the approval policy for a planned action and risk tier.
/// Enforces environment-level HEIWA_AUTO_APPROVE override.
fn get_state_dir() -> PathBuf {
    heiwa_config::HeiwaPaths::resolve().state_dir
}

pub fn evaluate_approval_policy(
    _action: &str,
    _target: &str,
    risk: RiskLevel,
    surface: &str,
) -> ApprovalVerdict {
    // Read override env variable
    let auto_approve_env =
        std::env::var("HEIWA_AUTO_APPROVE").unwrap_or_else(|_| "cli".to_string());
    if auto_approve_env == "all" {
        return ApprovalVerdict::AutoApproved;
    }
    if auto_approve_env == "cli" && surface == "cli" && risk < RiskLevel::Critical {
        return ApprovalVerdict::AutoApproved;
    }

    // Default threshold matrix
    let requires_hold = match (risk, surface) {
        (RiskLevel::Low, _) => false,
        (RiskLevel::Medium, "discord") => true,
        (RiskLevel::Medium, _) => false,
        (RiskLevel::High, "cli") => false,
        (RiskLevel::High, _) => true,
        (RiskLevel::Critical, _) => true,
    };

    if !requires_hold {
        return ApprovalVerdict::AutoApproved;
    }

    let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
    let request_path = get_state_dir()
        .join("dispatch")
        .join("requests")
        .join(format!("{}.json", request_id));

    ApprovalVerdict::AwaitingApproval {
        request_id,
        request_path,
    }
}

/// Stage the approval request JSON payload to local state
pub fn stage_approval_request(
    request_id: &str,
    request_path: &Path,
    action: &str,
    target: &str,
    risk: RiskLevel,
    surface: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    if let Some(parent) = request_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let request_json = json!({
        "id": request_id,
        "action": action,
        "target": target,
        "risk": risk.as_str(),
        "surface": surface,
        "created_at_utc": chrono::Utc::now().to_rfc3339(),
        "payload": payload,
    });
    fs::write(request_path, serde_json::to_string_pretty(&request_json)?)?;
    Ok(())
}

/// Block and watch for an operator decision on a request, with a timeout
pub fn wait_for_decision(request_id: &str, timeout: Duration) -> Result<String> {
    wait_for_decision_cancellable(request_id, timeout, &AtomicBool::new(false))
}

/// Blocking approval poll with a cooperative cancellation boundary. The
/// caller owns the flag and waits for this function to return before
/// declaring its turn cancelled, so no detached waiter survives a turn.
pub fn wait_for_decision_cancellable(
    request_id: &str,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<String> {
    if !heiwa_core::integrity::valid_decision_id(request_id) {
        return Err(anyhow!("invalid approval request id"));
    }
    let paths = heiwa_config::HeiwaPaths::try_resolve()
        .ok_or_else(|| anyhow!("runtime root unavailable for approval verification"))?;
    let decision_path = paths
        .state_dir
        .join("dispatch")
        .join("approvals")
        .join("decisions")
        .join(format!("{}.json", request_id));

    println!("wait_for_decision checking path: {:?}", decision_path);

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    while start.elapsed() < timeout {
        if cancelled.load(Ordering::Acquire) {
            return Err(anyhow!("approval wait cancelled for {}", request_id));
        }
        match fs::symlink_metadata(&decision_path) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(anyhow!("decision integrity verification failed: nonregular record; reconciliation needed"));
                }
                let parsed = heiwa_core::integrity::read_decision_record(&decision_path)
                    .map_err(|_| anyhow!("decision integrity verification failed: unreadable or malformed record; reconciliation needed"))?;
                let key = heiwa_core::config::read_runtime_secret_file(
                    &paths.runtime_root,
                    "machine_auth_token",
                );
                let integrity =
                    heiwa_core::integrity::classify_decision(key.as_deref(), request_id, &parsed);
                if integrity != heiwa_core::integrity::DecisionIntegrity::Verified {
                    return Err(anyhow!(
                        "decision integrity verification failed: {}; reconciliation needed",
                        integrity.as_str()
                    ));
                }
                return Ok(parsed["outcome"]
                    .as_str()
                    .expect("verified outcome")
                    .to_string());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        std::thread::sleep(poll_interval);
    }

    Err(anyhow!(
        "Timeout waiting for approval decision for {}",
        request_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let original = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, original }
        }

        fn remove(key: &'static str) -> Self {
            let original = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::High.as_str(), "high");
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn test_evaluate_approval_policy_auto_approve_all() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _auto_approve = EnvGuard::set("HEIWA_AUTO_APPROVE", "all");
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Critical, "discord");
        assert_eq!(verdict, ApprovalVerdict::AutoApproved);
    }

    #[test]
    fn test_evaluate_approval_policy_cli_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _auto_approve = EnvGuard::set("HEIWA_AUTO_APPROVE", "cli");
        // Low is always auto-approved
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Low, "discord");
        assert_eq!(verdict, ApprovalVerdict::AutoApproved);

        // Medium is auto-approved on CLI, but held on discord
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Medium, "cli");
        assert_eq!(verdict, ApprovalVerdict::AutoApproved);

        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Medium, "discord");
        assert!(matches!(verdict, ApprovalVerdict::AwaitingApproval { .. }));

        // High is auto-approved on CLI (requires_hold returns false for High & cli), but held on discord
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::High, "cli");
        assert_eq!(verdict, ApprovalVerdict::AutoApproved);

        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::High, "discord");
        assert!(matches!(verdict, ApprovalVerdict::AwaitingApproval { .. }));

        // Critical is always held
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Critical, "cli");
        assert!(matches!(verdict, ApprovalVerdict::AwaitingApproval { .. }));
    }

    #[test]
    fn test_stage_and_wait_for_decision() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let _home = EnvGuard::set("HOME", temp.path());
        let _heiwa_home = EnvGuard::remove("HEIWA_HOME");
        let _state_dir = EnvGuard::remove("HEIWA_STATE_DIR");

        let _auto_approve = EnvGuard::set("HEIWA_AUTO_APPROVE", "none");
        let verdict = evaluate_approval_policy("deploy", "production", RiskLevel::Critical, "cli");

        if let ApprovalVerdict::AwaitingApproval {
            request_id,
            request_path,
        } = verdict
        {
            let payload = json!({"cmd": "git push production"});
            stage_approval_request(
                &request_id,
                &request_path,
                "deploy",
                "production",
                RiskLevel::Critical,
                "cli",
                &payload,
            )
            .unwrap();

            assert!(request_path.exists());
            let file_content = fs::read_to_string(&request_path).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&file_content).unwrap();
            assert_eq!(parsed["action"], "deploy");
            assert_eq!(parsed["target"], "production");

            let decision_dir = temp
                .path()
                .join(".heiwa")
                .join("state")
                .join("dispatch")
                .join("approvals")
                .join("decisions");
            fs::create_dir_all(&decision_dir).unwrap();
            let decision_path = decision_dir.join(format!("{}.json", request_id));

            let secrets = temp.path().join(".heiwa/secrets");
            fs::create_dir_all(&secrets).unwrap();
            let key_file = secrets.join("machine_auth_token");
            fs::write(&key_file, "test-decision-key").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
                fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let mut decision_json = json!({
                "id": request_id,
                "outcome": "approved",
                "decided_at_utc": chrono::Utc::now().to_rfc3339()
            });
            heiwa_core::integrity::seal_decision("test-decision-key", &mut decision_json).unwrap();
            fs::write(
                &decision_path,
                serde_json::to_string_pretty(&decision_json).unwrap(),
            )
            .unwrap();

            let outcome = wait_for_decision(&request_id, Duration::from_secs(2)).unwrap();
            assert_eq!(outcome, "approved");
        } else {
            panic!("Expected AwaitingApproval verdict");
        }
    }

    #[test]
    fn an_untagged_or_wrong_id_record_cannot_authorize_a_wait() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let _home = EnvGuard::set("HOME", temp.path());
        let _heiwa_home = EnvGuard::remove("HEIWA_HOME");
        let _state_dir = EnvGuard::remove("HEIWA_STATE_DIR");
        let directory = temp
            .path()
            .join(".heiwa/state/dispatch/approvals/decisions");
        fs::create_dir_all(&directory).unwrap();
        for record in [
            json!({"id":"req_forged","outcome":"approved"}),
            json!({"id":"req_other","outcome":"approved"}),
        ] {
            fs::write(directory.join("req_forged.json"), record.to_string()).unwrap();
            let error = wait_for_decision("req_forged", Duration::from_millis(150))
                .expect_err("forged record cannot authorize effects");
            assert!(error.to_string().contains("integrity"));
        }
    }

    #[test]
    fn waits_require_the_file_key_and_an_untampered_allowlisted_outcome() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let _home = EnvGuard::set("HOME", temp.path());
        let _heiwa_home = EnvGuard::remove("HEIWA_HOME");
        let _state_dir = EnvGuard::remove("HEIWA_STATE_DIR");
        let _env_key = EnvGuard::set("HEIWA_MACHINE_AUTH_TOKEN", "key-a");
        let directory = temp
            .path()
            .join(".heiwa/state/dispatch/approvals/decisions");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("req_a.json");
        let mut record = json!({"id":"req_a", "outcome":"approved"});
        heiwa_core::integrity::seal_decision("key-a", &mut record).unwrap();
        fs::write(&path, record.to_string()).unwrap();
        assert!(
            wait_for_decision("req_a", Duration::from_millis(150))
                .unwrap_err()
                .to_string()
                .contains("unverifiable"),
            "environment presence cannot be authority"
        );
        let secrets = temp.path().join(".heiwa/secrets");
        fs::create_dir(&secrets).unwrap();
        let key_file = secrets.join("machine_auth_token");
        fs::write(&key_file, "key-a").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700)).unwrap();
            fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(
            wait_for_decision("req_a", Duration::from_millis(150)).unwrap(),
            "approved"
        );
        let mut denied = json!({"id":"req_a", "outcome":"denied"});
        heiwa_core::integrity::seal_decision("key-a", &mut denied).unwrap();
        fs::write(&path, denied.to_string()).unwrap();
        assert_eq!(
            wait_for_decision("req_a", Duration::from_millis(150)).unwrap(),
            "denied"
        );
        record["outcome"] = json!("denied");
        fs::write(&path, record.to_string()).unwrap();
        assert!(wait_for_decision("req_a", Duration::from_millis(150))
            .unwrap_err()
            .to_string()
            .contains("tag_mismatch"));
        let mut invalid_outcome = json!({"id":"req_a", "outcome":"run-anything"});
        let tag = heiwa_core::integrity::decision_tag("key-a", &invalid_outcome).unwrap();
        invalid_outcome["integrity"] = json!({"version":"1","alg":"hmac-sha256","tag":tag});
        fs::write(&path, invalid_outcome.to_string()).unwrap();
        assert!(wait_for_decision("req_a", Duration::from_millis(150))
            .unwrap_err()
            .to_string()
            .contains("invalid_outcome"));
        assert!(wait_for_decision("../req_a", Duration::from_millis(150))
            .unwrap_err()
            .to_string()
            .contains("invalid approval"));
        assert!(wait_for_decision("req_absent", Duration::from_millis(1))
            .unwrap_err()
            .to_string()
            .contains("Timeout"));
    }

    #[test]
    fn cancellable_wait_stops_before_polling_for_a_decision() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let _home = EnvGuard::set("HOME", temp.path());
        let _heiwa_home = EnvGuard::remove("HEIWA_HOME");
        let _state_dir = EnvGuard::remove("HEIWA_STATE_DIR");
        let cancelled = AtomicBool::new(true);
        let error =
            wait_for_decision_cancellable("req_cancelled", Duration::from_secs(1), &cancelled)
                .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    }
}
