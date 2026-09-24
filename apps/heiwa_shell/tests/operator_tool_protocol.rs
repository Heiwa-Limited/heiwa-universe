//! The tool-call protocol a model must follow is taught at the same boundary
//! that parses it: the operator runner. A turn whose scope grants a read tool
//! the protocol names hears it exactly once, in its system preamble, at every
//! model stage; a turn without such a grant never hears about tools. These
//! tests capture the messages a provider actually receives through the real
//! runner and model-call executor.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
use heiwa_shell::agentic::tool_instruction_prompt;
use heiwa_shell::model_calls::ModelCallExecutor;
use heiwa_shell::operator::{OperatorModelTurn, OperatorTurnRunner, OperatorTurnWork};
use serde_json::json;
use tokio::sync::mpsc;

/// Records every message list it is sent. The first stage answers with
/// `first`; any later stage finishes.
struct RecordingAdapter {
    first: String,
    seen: Mutex<Vec<Vec<Message>>>,
    calls: AtomicUsize,
}

impl RecordingAdapter {
    fn answering(first: &str) -> Arc<Self> {
        Arc::new(Self {
            first: first.to_string(),
            seen: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        })
    }

    fn stages(&self) -> Vec<Vec<Message>> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProviderAdapter for RecordingAdapter {
    async fn send(
        &self,
        _model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        self.seen.lock().unwrap().push(messages.to_vec());
        let text = if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.clone()
        } else {
            "final answer".to_string()
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

const LIST_CALL: &str =
    r#"{"tool_calls":[{"id":"list-1","name":"fs.list","arguments":{"path":"."}}]}"#;

fn system(content: &str) -> Message {
    Message {
        role: Role::System,
        content: content.into(),
    }
}

fn user(content: &str) -> Message {
    Message {
        role: Role::User,
        content: content.into(),
    }
}

fn scope_granting(
    workspace: &std::path::Path,
    leases: &[(&str, RiskClass, bool)],
) -> ExecutionScope {
    let mut scope = ExecutionScope::local_default(workspace.to_path_buf());
    for (name, risk_class, allowed) in leases {
        scope.tool_leases.push(ToolLease {
            name: (*name).into(),
            risk_class: *risk_class,
            allowed: *allowed,
        });
    }
    scope
}

fn turn(messages: Vec<Message>, tool_scope: Option<ExecutionScope>) -> OperatorModelTurn {
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
    OperatorModelTurn {
        request: ModelCallRequest {
            thread_id: String::new(),
            turn_id: String::new(),
            work_id: None,
            call_id: format!("call-{}", uuid::Uuid::new_v4()),
            intent: "research".into(),
            stage: ModelCallStage::Execution,
            raw_text: "what is here".into(),
            privacy: PrivacyClass::LocalOnly,
            risk: CallRisk::Low,
            safety: SafetyClass::low_risk_auto_approval(&CallRisk::Low),
            required_capabilities: vec![],
            required_context_tokens: 1,
            minimum_quality_class: 1,
            minimum_success_rate: 0.0,
            maximum_marginal_cost_usd: Some(0.0),
            preferred_provider: None,
            preferred_model: None,
            allowed_models: vec![],
            excluded_models: vec![],
        },
        candidates: vec![candidate],
        messages,
        remaining_budget_usd: None,
        max_attempts: 1,
        tool_scope,
        done_payload: Arc::new(|result| json!({ "text": result.text })),
    }
}

/// Run one turn through the real runner and executor; return every message
/// list the provider received, one per model stage.
async fn delivered(first: &str, turn: OperatorModelTurn) -> Vec<Vec<Message>> {
    let journal = tempfile::tempdir().unwrap();
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(journal.path().to_path_buf()).unwrap(),
    ));
    let adapter = RecordingAdapter::answering(first);
    let provider: Arc<dyn ProviderAdapter> = adapter.clone();
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |_: &str, _: &str| Some(provider.clone())),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor);
    sessions.ensure_thread("thread-protocol").expect("thread");
    let mut handle = runner
        .submit(
            "thread-protocol",
            StartTurnRequest::auto(format!("protocol-{}", uuid::Uuid::new_v4()), "what is here"),
            OperatorTurnWork::Model(Box::new(turn)),
        )
        .expect("submit");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), handle.recv())
            .await
            .expect("turn timed out")
            .expect("stream closed");
        if frame.is_terminal() {
            break;
        }
    }
    adapter.stages()
}

fn protocol_positions(messages: &[Message]) -> Vec<usize> {
    let protocol = tool_instruction_prompt();
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.content == protocol)
        .map(|(index, _)| index)
        .collect()
}

fn mentions_tools(messages: &[Message]) -> bool {
    messages
        .iter()
        .any(|message| message.content.contains("tool_calls"))
}

#[tokio::test]
async fn a_turn_granted_read_tools_is_taught_the_protocol_once_at_every_stage() {
    let workspace = tempfile::tempdir().unwrap();
    let scope = scope_granting(
        workspace.path(),
        &[
            ("fs.list", RiskClass::HostSafeReadonly, true),
            ("fs.read", RiskClass::HostSafeReadonly, true),
            ("repo.grep", RiskClass::HostSafeReadonly, true),
        ],
    );
    let stages = delivered(
        LIST_CALL,
        turn(
            vec![system("caller preamble"), user("what is here")],
            Some(scope),
        ),
    )
    .await;

    assert_eq!(
        stages.len(),
        2,
        "one tool round: a first and a follow-up stage"
    );
    for (stage, messages) in stages.iter().enumerate() {
        assert_eq!(
            protocol_positions(messages),
            vec![1],
            "stage {stage}: once, right after the caller's system preamble: {messages:#?}"
        );
        assert!(matches!(messages[1].role, Role::System), "stage {stage}");
        assert_eq!(messages[0].content, "caller preamble", "stage {stage}");
        assert!(matches!(messages[2].role, Role::User), "stage {stage}");
    }
}

#[tokio::test]
async fn a_caller_that_already_sent_the_protocol_is_not_sent_it_twice() {
    let workspace = tempfile::tempdir().unwrap();
    let scope = scope_granting(
        workspace.path(),
        &[("fs.list", RiskClass::HostSafeReadonly, true)],
    );
    let stages = delivered(
        LIST_CALL,
        turn(
            vec![system(&tool_instruction_prompt()), user("what is here")],
            Some(scope),
        ),
    )
    .await;
    for messages in &stages {
        assert_eq!(protocol_positions(messages), vec![0], "{messages:#?}");
    }
}

#[tokio::test]
async fn a_turn_without_a_granted_read_tool_never_hears_about_tools() {
    let workspace = tempfile::tempdir().unwrap();
    let cases = [
        ("no scope", None),
        (
            "a scope with no leases",
            Some(scope_granting(workspace.path(), &[])),
        ),
        (
            "a denied read lease",
            Some(scope_granting(
                workspace.path(),
                &[("fs.read", RiskClass::HostSafeReadonly, false)],
            )),
        ),
        (
            "only a tool the protocol does not name",
            Some(scope_granting(
                workspace.path(),
                &[("shell", RiskClass::HostMutating, true)],
            )),
        ),
    ];
    for (case, scope) in cases {
        let stages = delivered(
            "a plain answer",
            turn(vec![system("caller preamble"), user("what is here")], scope),
        )
        .await;
        assert_eq!(stages.len(), 1, "{case}: no tool round");
        assert!(!mentions_tools(&stages[0]), "{case}: {:#?}", stages[0]);
        assert_eq!(stages[0].len(), 2, "{case}: messages unchanged");
    }
}

#[test]
fn the_protocol_names_every_tool_that_unlocks_it() {
    let protocol = tool_instruction_prompt();
    for tool in heiwa_shell::agentic::TOOL_PROTOCOL_TOOLS {
        assert!(protocol.contains(tool), "{tool} missing from the protocol");
    }
}
