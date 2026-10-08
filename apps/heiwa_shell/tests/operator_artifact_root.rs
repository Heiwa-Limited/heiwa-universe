//! Operator artifacts belong to the journal whose `artifact_created` links
//! make them durable. A runner built over one session journal must commit and
//! reconcile only there — never in another journal, and in particular never
//! in the process's ambient Heiwa root, where reconciling against the wrong
//! links deletes artifacts that journal still owns.
//!
//! This is its own test binary on purpose, holding exactly one test: it
//! points the process's ambient Heiwa roots at disposable directories before
//! anything resolves them, which would race with any other test in the same
//! process. Nothing here touches the real Heiwa home.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use heiwa_core::drex::{
    CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallRequest, ModelCallStage,
    PrivacyClass, SafetyClass,
};
use heiwa_evidence::OperatorJournal;
use heiwa_protocol::{ExecutionScope, ModelTier, RiskClass, ToolLease};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use heiwa_shell::model_calls::ModelCallExecutor;
use heiwa_shell::operator::{OperatorModelTurn, OperatorTurnRunner, OperatorTurnWork};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

/// Asks to read the large fixture file, then finishes.
struct LargeReadAdapter {
    calls: AtomicUsize,
}

#[async_trait]
impl ProviderAdapter for LargeReadAdapter {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let text = if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            json!({ "tool_calls": [{
                "id": "read-1",
                "name": "fs.read",
                "arguments": { "path": "large.txt", "max_bytes": 32768 }
            }] })
            .to_string()
        } else {
            "read complete".to_string()
        };
        stream_tx.send(StreamEvent::Token(text)).await?;
        stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await?;
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }
}

fn large_read_turn(workspace: &Path) -> OperatorModelTurn {
    let prompt = "read the large file";
    let candidate = ModelCallCandidate {
        tier: ModelTier {
            id: 1,
            model_id: "fixture-model".into(),
            provider_model_id: "fixture-model".into(),
            provider: "fixture".into(),
            capability_class: 1,
            max_context_tokens: 128_000,
            enabled: true,
            last_success_rate: 1.0,
            ..ModelTier::default()
        },
        locality: ExecutionLocality::OnDevice,
        connected: true,
        adapter_capable: true,
        quota_available: true,
        marginal_cost_usd: Some(0.0),
        cost_truth: CostTruth::LocalZeroCost,
    };
    let mut scope = ExecutionScope::local_default(workspace.to_path_buf());
    scope.tool_leases.push(ToolLease {
        name: "fs.read".into(),
        risk_class: RiskClass::HostSafeReadonly,
        allowed: true,
    });
    OperatorModelTurn {
        request: ModelCallRequest {
            thread_id: "thread-artifacts".into(),
            turn_id: "turn-1".into(),
            work_id: None,
            call_id: "call-1".into(),
            intent: "chat".into(),
            stage: ModelCallStage::Execution,
            raw_text: prompt.into(),
            privacy: PrivacyClass::Standard,
            risk: CallRisk::Low,
            safety: SafetyClass::low_risk_auto_approval(&CallRisk::Low),
            required_capabilities: vec![],
            required_context_tokens: 1,
            minimum_quality_class: 1,
            minimum_success_rate: 0.0,
            maximum_marginal_cost_usd: None,
            preferred_provider: None,
            preferred_model: None,
            allowed_models: vec![],
            excluded_models: vec![],
        },
        candidates: vec![candidate],
        messages: vec![Message {
            role: Role::User,
            content: prompt.into(),
        }],
        remaining_budget_usd: None,
        max_attempts: 1,
        tool_scope: Some(scope),
        done_payload: Arc::new(|result| json!({ "text": result.text })),
    }
}

/// Run one large-output tool turn through a runner built over `sessions`
/// with the runner's own default artifact store.
async fn run_large_read(sessions: &Arc<OperatorSessionService>, workspace: &Path) {
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(LargeReadAdapter {
        calls: AtomicUsize::new(0),
    });
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |_: &str, _: &str| Some(adapter.clone())),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor);
    sessions.ensure_thread("thread-artifacts").expect("thread");
    let mut handle = runner
        .submit(
            "thread-artifacts",
            StartTurnRequest::auto(format!("large-read-{}", uuid::Uuid::new_v4()), "read"),
            OperatorTurnWork::Model(Box::new(large_read_turn(workspace))),
        )
        .expect("submit");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), handle.recv())
            .await
            .expect("turn timed out")
            .expect("stream closed");
        if frame.is_terminal() {
            return;
        }
    }
}

/// File name -> content digest for every file in `dir` (empty if absent).
fn snapshot(dir: &Path) -> BTreeMap<String, String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).expect("read");
            (
                path.file_name().unwrap().to_string_lossy().to_string(),
                format!("{:x}", Sha256::digest(bytes)),
            )
        })
        .collect()
}

fn artifact_ids(sessions: &OperatorSessionService) -> Vec<String> {
    let mut ids: Vec<String> = sessions
        .artifact_links()
        .expect("links")
        .into_iter()
        .map(|(_, artifact_id)| artifact_id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_runner_commits_and_reconciles_artifacts_only_under_its_own_journal() {
    let scratch = tempfile::tempdir().unwrap();
    let ambient = scratch.path().join("ambient");
    std::env::set_var("HEIWA_HOME", ambient.join("home"));
    std::env::set_var("HEIWA_STATE_DIR", ambient.join("state"));
    std::env::set_var("HEIWA_EVIDENCE_DIR", ambient.join("evidence"));
    let journal_b: PathBuf = heiwa_evidence::journal_root().expect("ambient root");
    assert_eq!(
        journal_b,
        ambient.join("evidence"),
        "the ambient root is disposable"
    );

    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("large.txt"), "x".repeat(20 * 1024)).unwrap();

    // Journal B is the ambient root, with a linked artifact of its own.
    let sessions_b = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(journal_b.clone()).unwrap(),
    ));
    run_large_read(&sessions_b, workspace.path()).await;
    let b_ids = artifact_ids(&sessions_b);
    assert_eq!(b_ids.len(), 1, "B links one artifact");
    let b_dir = journal_b.join("operator_artifacts");
    let b_before = snapshot(&b_dir);
    assert_eq!(
        b_before.keys().cloned().collect::<Vec<_>>(),
        vec![format!("{}.json", b_ids[0])],
        "B holds exactly its linked artifact"
    );

    // Journal A is a different disposable root. An unlinked leftover in its
    // own artifact directory is what A's reconciliation should remove.
    let journal_a = scratch.path().join("journal-a");
    let a_dir = journal_a.join("operator_artifacts");
    std::fs::create_dir_all(&a_dir).unwrap();
    std::fs::write(a_dir.join("artifact-leftover.json"), b"{}").unwrap();
    let sessions_a = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(journal_a.clone()).unwrap(),
    ));
    run_large_read(&sessions_a, workspace.path()).await;

    assert_eq!(
        snapshot(&b_dir),
        b_before,
        "running over A never touches B's artifacts"
    );
    let a_ids = artifact_ids(&sessions_a);
    assert_eq!(a_ids.len(), 1, "A links one artifact");
    assert_eq!(
        snapshot(&a_dir).keys().cloned().collect::<Vec<_>>(),
        vec![format!("{}.json", a_ids[0])],
        "A's artifact is under A and A's reconciliation removed its unlinked leftover"
    );
}
