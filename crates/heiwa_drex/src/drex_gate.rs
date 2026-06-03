use anyhow::{anyhow, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
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
fn get_home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
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
    let home = get_home_dir();
    let request_path = home
        .join(".heiwa")
        .join("state")
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
    let home = get_home_dir();
    let decision_path = home
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join("approvals")
        .join("decisions")
        .join(format!("{}.json", request_id));

    println!("wait_for_decision checking path: {:?}", decision_path);

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    while start.elapsed() < timeout {
        if decision_path.exists() {
            let raw = fs::read_to_string(&decision_path)?;
            let parsed: serde_json::Value = serde_json::from_str(&raw)?;
            if let Some(outcome) = parsed.get("outcome").and_then(serde_json::Value::as_str) {
                return Ok(outcome.to_string());
            }
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
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
        std::env::set_var("HEIWA_AUTO_APPROVE", "all");
        let verdict = evaluate_approval_policy("fs_write", "/path", RiskLevel::Critical, "discord");
        assert_eq!(verdict, ApprovalVerdict::AutoApproved);
    }

    #[test]
    fn test_evaluate_approval_policy_cli_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::set_var("HEIWA_AUTO_APPROVE", "cli");
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
        std::env::set_var("HOME", temp.path());

        std::env::set_var("HEIWA_AUTO_APPROVE", "none");
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

            let decision_json = json!({
                "request_id": request_id,
                "outcome": "approved",
                "decided_at_utc": chrono::Utc::now().to_rfc3339()
            });
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
}
