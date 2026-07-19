use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use heiwa_core::drex::{ExecutionLocality, ModelCallCandidate, ModelCallRequest, PrivacyClass};
use heiwa_evidence::{
    now_iso, CursorEvent, EvidenceTransport, JsonlTransport, OperatorActor, OperatorEvent,
    OperatorEventType, OperatorRisk, OperatorSensitivity, PersistedArtifact,
    OPERATOR_EVENT_SCHEMA_VERSION,
};
use heiwa_protocol::ExecutionScope;
use heiwa_provider::adapter::{Message, StreamEvent};
use heiwa_session::operator::{
    OperatorSessionService, RouteMode, StartTurnRequest, TurnRoutePolicy,
};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::model_calls::{ModelCallError, ModelCallExecution, ModelCallExecutor, ModelCallResult};

const OPERATOR_STREAM_CAPACITY: usize = 128;

#[derive(Default, Clone)]
pub struct ActiveTurnRegistry {
    turns: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

impl ActiveTurnRegistry {
    pub fn register(&self, turn_id: String) -> watch::Receiver<bool> {
        let (sender, receiver) = watch::channel(false);
        self.turns.lock().unwrap().insert(turn_id, sender);
        receiver
    }

    pub fn signal_cancel(&self, turn_id: &str) -> bool {
        self.turns
            .lock()
            .unwrap()
            .get(turn_id)
            .is_some_and(|sender| sender.send(true).is_ok())
    }

    pub fn remove(&self, turn_id: &str) {
        self.turns.lock().unwrap().remove(turn_id);
    }
}

#[derive(Debug, Clone)]
pub enum OperatorStreamFrame {
    Durable(Box<CursorEvent>),
    AssistantDelta {
        thread_id: String,
        turn_id: String,
        text: String,
    },
    Error {
        thread_id: String,
        turn_id: String,
        message: String,
    },
}

impl OperatorStreamFrame {
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Error { .. } => true,
            Self::Durable(row) => matches!(
                row.event.event_type,
                OperatorEventType::TurnCompleted
                    | OperatorEventType::TurnInterrupted
                    | OperatorEventType::Blocker
            ),
            Self::AssistantDelta { .. } => false,
        }
    }
}

#[async_trait]
pub trait OperatorModelExecutor: Send + Sync {
    async fn execute(
        &self,
        execution: ModelCallExecution,
    ) -> std::result::Result<ModelCallResult, ModelCallError>;
}

#[async_trait]
impl OperatorModelExecutor for ModelCallExecutor {
    async fn execute(
        &self,
        execution: ModelCallExecution,
    ) -> std::result::Result<ModelCallResult, ModelCallError> {
        ModelCallExecutor::execute(self, execution).await
    }
}

pub type DonePayload = Arc<dyn Fn(&ModelCallResult) -> serde_json::Value + Send + Sync>;

pub trait OperatorArtifactStore: Send + Sync {
    fn store(&self, artifact: PersistedArtifact) -> Result<()>;
}

#[derive(Default)]
struct LocalArtifactStore;

impl OperatorArtifactStore for LocalArtifactStore {
    fn store(&self, artifact: PersistedArtifact) -> Result<()> {
        JsonlTransport::default_local()?.register_artifact(artifact)
    }
}

pub struct OperatorModelTurn {
    pub request: ModelCallRequest,
    pub candidates: Vec<ModelCallCandidate>,
    pub messages: Vec<Message>,
    pub remaining_budget_usd: Option<f64>,
    pub max_attempts: usize,
    pub tool_scope: Option<ExecutionScope>,
    pub done_payload: DonePayload,
}

pub enum OperatorTurnWork {
    Deterministic {
        response: String,
        route: serde_json::Value,
        done: serde_json::Value,
    },
    Model(Box<OperatorModelTurn>),
}

pub struct OperatorTurnHandle {
    pub thread_id: String,
    pub turn_id: String,
    pub cursor: String,
    pub duplicate: bool,
    pub frames: broadcast::Receiver<OperatorStreamFrame>,
}

#[derive(Clone)]
pub struct OperatorTurnRunner {
    sessions: Arc<OperatorSessionService>,
    executor: Arc<dyn OperatorModelExecutor>,
    active: ActiveTurnRegistry,
    active_threads: Arc<Mutex<HashMap<String, String>>>,
    frames: broadcast::Sender<OperatorStreamFrame>,
    artifacts: Arc<dyn OperatorArtifactStore>,
}

impl OperatorTurnRunner {
    pub fn new(
        sessions: Arc<OperatorSessionService>,
        executor: Arc<dyn OperatorModelExecutor>,
    ) -> Self {
        let (frames, _) = broadcast::channel(OPERATOR_STREAM_CAPACITY);
        Self {
            sessions,
            executor,
            active: ActiveTurnRegistry::default(),
            active_threads: Arc::new(Mutex::new(HashMap::new())),
            frames,
            artifacts: Arc::new(LocalArtifactStore),
        }
    }

    pub fn with_artifact_store(mut self, artifacts: Arc<dyn OperatorArtifactStore>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn active_turns(&self) -> &ActiveTurnRegistry {
        &self.active
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OperatorStreamFrame> {
        self.frames.subscribe()
    }

    pub fn submit(
        &self,
        thread_id: &str,
        request: StartTurnRequest,
        work: OperatorTurnWork,
    ) -> Result<OperatorTurnHandle> {
        let route_policy = request.route_policy.clone();
        let frames = self.subscribe();
        let submission = self.sessions.start_turn(thread_id, request)?;
        if !submission.duplicate {
            let cancel = self.active.register(submission.turn_id.clone());
            self.active_threads
                .lock()
                .unwrap()
                .insert(submission.turn_id.clone(), thread_id.to_string());
            let runner = self.clone();
            let thread_id = thread_id.to_string();
            let turn_id = submission.turn_id.clone();
            tokio::spawn(async move {
                runner
                    .run_and_close(thread_id, turn_id, route_policy, work, cancel)
                    .await;
            });
        }
        Ok(OperatorTurnHandle {
            thread_id: submission.thread_id,
            turn_id: submission.turn_id,
            cursor: submission.cursor,
            duplicate: submission.duplicate,
            frames,
        })
    }

    pub fn request_cancel(&self, turn_id: &str) -> Result<bool> {
        let Some(thread_id) = self.active_threads.lock().unwrap().get(turn_id).cloned() else {
            return Ok(false);
        };
        let row = self.sessions.append_event(runtime_event(
            &thread_id,
            turn_id,
            None,
            OperatorEventType::TurnCancelRequested,
            json!({"reason": "OPERATOR_REQUEST"}),
        ))?;
        let _ = self
            .frames
            .send(OperatorStreamFrame::Durable(Box::new(row)));
        Ok(self.active.signal_cancel(turn_id))
    }

    async fn run_and_close(
        &self,
        thread_id: String,
        turn_id: String,
        route_policy: TurnRoutePolicy,
        work: OperatorTurnWork,
        cancel: watch::Receiver<bool>,
    ) {
        let result = self
            .run_turn(&thread_id, &turn_id, &route_policy, work, cancel.clone())
            .await;
        if let Err(error) = result {
            let cancelled = *cancel.borrow();
            let payload = if cancelled {
                json!({"reason": "OPERATOR_CANCELLED"})
            } else {
                json!({"reason": "EXECUTION_FAILED", "message": bounded_text(&error.to_string(), 1024)})
            };
            if let Ok(row) = self.sessions.append_event(runtime_event(
                &thread_id,
                &turn_id,
                None,
                OperatorEventType::TurnInterrupted,
                payload,
            )) {
                let _ = self
                    .frames
                    .send(OperatorStreamFrame::Durable(Box::new(row)));
            } else {
                let _ = self.frames.send(OperatorStreamFrame::Error {
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message: error.to_string(),
                });
            }
        }
        self.active.remove(&turn_id);
        self.active_threads.lock().unwrap().remove(&turn_id);
    }

    async fn run_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        route_policy: &TurnRoutePolicy,
        work: OperatorTurnWork,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut cursor = self
            .append_and_publish(runtime_event(
                thread_id,
                turn_id,
                None,
                OperatorEventType::AssistantStarted,
                json!({}),
            ))?
            .cursor;

        match work {
            OperatorTurnWork::Deterministic {
                response,
                route,
                done,
            } => {
                let call_id = format!("call-{}", Uuid::new_v4());
                self.append_and_publish(runtime_event(
                    thread_id,
                    turn_id,
                    Some(&call_id),
                    OperatorEventType::RoutePlanned,
                    route.clone(),
                ))?;
                self.append_and_publish(runtime_event(
                    thread_id,
                    turn_id,
                    Some(&call_id),
                    OperatorEventType::RouteCompleted,
                    route,
                ))?;
                let _ = self.frames.send(OperatorStreamFrame::AssistantDelta {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    text: response.clone(),
                });
                self.finish_turn(thread_id, turn_id, response, done, None, Some(call_id))?;
            }
            OperatorTurnWork::Model(mut model) => {
                apply_route_policy(&mut model.request, route_policy)?;
                apply_candidate_policy(&mut model.candidates, route_policy);
                model.request.thread_id = thread_id.to_string();
                model.request.turn_id = turn_id.to_string();
                let result = self
                    .execute_model(thread_id, turn_id, &mut cursor, *model, cancel)
                    .await?;
                let done = (result.done_payload)(&result.result);
                let response = result.result.text.clone();
                self.finish_turn(
                    thread_id,
                    turn_id,
                    response,
                    done,
                    Some(result.result),
                    Some(result.receipt_ref),
                )?;
            }
        }
        Ok(())
    }

    async fn execute_model(
        &self,
        thread_id: &str,
        turn_id: &str,
        cursor: &mut String,
        model: OperatorModelTurn,
        cancel: watch::Receiver<bool>,
    ) -> Result<CompletedModelTurn> {
        let request_template = model.request.clone();
        let mut receipt_ref = request_template.call_id.clone();
        let candidates = model.candidates.clone();
        let mut messages = model.messages.clone();
        let remaining_budget_usd = model.remaining_budget_usd;
        let max_attempts = model.max_attempts;
        let done_payload = model.done_payload;
        let first = self
            .execute_model_stage(
                thread_id,
                turn_id,
                cursor,
                model.request,
                model.candidates,
                model.messages,
                remaining_budget_usd,
                max_attempts,
                cancel.clone(),
            )
            .await?;

        let tool_calls = crate::agentic::parse_tool_calls(&first.text);
        let Some(scope) = model.tool_scope else {
            return Ok(CompletedModelTurn {
                result: first,
                done_payload,
                receipt_ref,
            });
        };
        if tool_calls.is_empty() {
            return Ok(CompletedModelTurn {
                result: first,
                done_payload,
                receipt_ref,
            });
        }

        let mut tool_entries = Vec::new();
        for call in tool_calls {
            if *cancel.borrow() {
                return Err(anyhow!("operator turn cancelled"));
            }
            self.append_and_publish(runtime_event(
                thread_id,
                turn_id,
                Some(&call.id),
                OperatorEventType::ToolCallStarted,
                json!({"name": call.name, "arguments": call.arguments}),
            ))?;
            let mut tool_cancel = cancel.clone();
            let tool_execution = crate::agentic::execute_tool_call(
                scope.clone(),
                call.clone(),
                &first.provider,
                &first.model_id,
            );
            tokio::pin!(tool_execution);
            let (receipt, entry) = tokio::select! {
                result = &mut tool_execution => result?,
                changed = tool_cancel.changed() => {
                    match changed {
                        Ok(()) if *tool_cancel.borrow() => return Err(anyhow!("operator turn cancelled")),
                        _ => return Err(anyhow!("operator cancellation channel closed")),
                    }
                }
            };
            if *cancel.borrow() {
                return Err(anyhow!("operator turn cancelled"));
            }
            let (preview, artifact_ref) = self.persist_large_tool_output(
                thread_id,
                turn_id,
                &call.id,
                &call.name,
                &entry.output,
            )?;
            self.append_and_publish(runtime_event(
                thread_id,
                turn_id,
                Some(&call.id),
                OperatorEventType::ToolCallCompleted,
                json!({
                    "name": call.name,
                    "status": receipt.status.as_str(),
                    "output": preview.clone(),
                    "output_preview": preview,
                    "artifact_ref": artifact_ref,
                    "receipt_id": receipt.id,
                    "error": receipt.error,
                }),
            ))?;
            tool_entries.push(entry);
        }

        if *cancel.borrow() {
            return Err(anyhow!("operator turn cancelled"));
        }
        messages.push(Message {
            role: heiwa_provider::adapter::Role::Assistant,
            content: first.text,
        });
        messages.push(Message {
            role: heiwa_provider::adapter::Role::User,
            content: crate::agentic::tool_result_prompt(&tool_entries),
        });
        let mut follow_up = request_template;
        follow_up.call_id = format!("call-{}", Uuid::new_v4());
        receipt_ref = follow_up.call_id.clone();
        follow_up.raw_text = crate::agentic::tool_result_prompt(&tool_entries);
        let result = self
            .execute_model_stage(
                thread_id,
                turn_id,
                cursor,
                follow_up,
                candidates,
                messages,
                remaining_budget_usd,
                max_attempts,
                cancel,
            )
            .await?;
        Ok(CompletedModelTurn {
            result,
            done_payload,
            receipt_ref,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_model_stage(
        &self,
        thread_id: &str,
        turn_id: &str,
        cursor: &mut String,
        request: ModelCallRequest,
        candidates: Vec<ModelCallCandidate>,
        messages: Vec<Message>,
        remaining_budget_usd: Option<f64>,
        max_attempts: usize,
        cancel: watch::Receiver<bool>,
    ) -> Result<ModelCallResult> {
        let (delta_tx, mut delta_rx) = mpsc::channel(32);
        let execution = ModelCallExecution {
            request,
            candidates,
            messages,
            remaining_budget_usd,
            max_attempts,
            cancel,
            delta_tx: Some(delta_tx),
        };
        let future = self.executor.execute(execution);
        tokio::pin!(future);
        let result = loop {
            tokio::select! {
                result = &mut future => break result.map_err(anyhow::Error::from)?,
                delta = delta_rx.recv() => {
                    let Some(delta) = delta else { continue; };
                    self.publish_durable_after(thread_id, cursor)?;
                    if let StreamEvent::Token(text) = delta {
                        let _ = self.frames.send(OperatorStreamFrame::AssistantDelta {
                            thread_id: thread_id.to_string(),
                            turn_id: turn_id.to_string(),
                            text,
                        });
                    }
                }
            }
        };
        self.publish_durable_after(thread_id, cursor)?;
        while let Ok(delta) = delta_rx.try_recv() {
            if let StreamEvent::Token(text) = delta {
                let _ = self.frames.send(OperatorStreamFrame::AssistantDelta {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    text,
                });
            }
        }
        Ok(result)
    }

    fn persist_large_tool_output(
        &self,
        thread_id: &str,
        turn_id: &str,
        call_id: &str,
        tool_name: &str,
        output: &str,
    ) -> Result<(String, Option<String>)> {
        const MAX_OPERATOR_TOOL_OUTPUT_BYTES: usize = 16 * 1024;
        if output.len() <= MAX_OPERATOR_TOOL_OUTPUT_BYTES {
            return Ok((output.to_string(), None));
        }
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        self.artifacts.store(PersistedArtifact {
            artifact_id: artifact_id.clone(),
            run_id: None,
            lease_id: None,
            session_id: Some(thread_id.to_string()),
            user_id: "local-operator".to_string(),
            mission_id: turn_id.to_string(),
            cell_run_id: None,
            artifact_type: "tool_output".to_string(),
            title: format!("{tool_name} output"),
            uri: None,
            path: None,
            content_json: serde_json::to_string(output)?,
            created_at: now_iso(),
            owner_id: Some("local-operator".to_string()),
            principal_id: Some("operator-turn-runner".to_string()),
        })?;
        self.append_and_publish(runtime_event(
            thread_id,
            turn_id,
            Some(call_id),
            OperatorEventType::ArtifactCreated,
            json!({
                "artifact_id": artifact_id,
                "kind": "tool_output",
                "tool_name": tool_name,
                "byte_len": output.len(),
            }),
        ))?;
        Ok((
            bounded_text(output, MAX_OPERATOR_TOOL_OUTPUT_BYTES),
            Some(artifact_id),
        ))
    }

    fn finish_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        response: String,
        done: serde_json::Value,
        model: Option<ModelCallResult>,
        receipt_ref: Option<String>,
    ) -> Result<()> {
        self.append_and_publish(runtime_event(
            thread_id,
            turn_id,
            None,
            OperatorEventType::AssistantCompleted,
            json!({"text": response}),
        ))?;
        self.append_and_publish(runtime_event(
            thread_id,
            turn_id,
            None,
            OperatorEventType::ReceiptLinked,
            json!({
                "kind": "operator_turn",
                "receipt_ref": receipt_ref.clone(),
                "text": receipt_ref.as_ref().map(|reference| format!("operator turn receipt {reference}")),
                "provider": model.as_ref().map(|result| result.provider.as_str()),
                "model": model.as_ref().map(|result| result.model_id.as_str()),
                "cost_usd": model.as_ref().map(|result| result.cost_usd),
                "cost_truth": model.as_ref().map(|result| &result.cost_truth),
            }),
        ))?;
        self.append_and_publish(runtime_event(
            thread_id,
            turn_id,
            None,
            OperatorEventType::TurnCompleted,
            json!({"trace": done}),
        ))?;
        Ok(())
    }

    fn append_and_publish(&self, event: OperatorEvent) -> Result<CursorEvent> {
        let row = self.sessions.append_event(event)?;
        let _ = self
            .frames
            .send(OperatorStreamFrame::Durable(Box::new(row.clone())));
        Ok(row)
    }

    fn publish_durable_after(&self, thread_id: &str, cursor: &mut String) -> Result<()> {
        let page = self.sessions.events_after(thread_id, Some(cursor), 256)?;
        for row in page.events {
            *cursor = row.cursor.clone();
            let _ = self
                .frames
                .send(OperatorStreamFrame::Durable(Box::new(row)));
        }
        if let Some(next) = page.next_cursor {
            *cursor = next;
        }
        Ok(())
    }
}

struct CompletedModelTurn {
    result: ModelCallResult,
    done_payload: DonePayload,
    receipt_ref: String,
}

fn apply_route_policy(request: &mut ModelCallRequest, policy: &TurnRoutePolicy) -> Result<()> {
    request.preferred_provider = policy.preferred_provider.clone();
    request.preferred_model = policy.preferred_model.clone();
    request.allowed_models = policy.allowed_models.clone();
    request.excluded_models = policy.excluded_models.clone();
    request.minimum_quality_class = policy.minimum_quality_class;
    request.maximum_marginal_cost_usd = policy.maximum_marginal_cost_usd;
    request.privacy = PrivacyClass::parse(&policy.privacy).map_err(|error| anyhow!(error))?;
    match policy.mode {
        RouteMode::Auto => {}
        RouteMode::LocalOnly => request.privacy = PrivacyClass::LocalOnly,
        RouteMode::RemoteOnly => {}
        RouteMode::Explicit => {
            if request.preferred_provider.is_none() && request.preferred_model.is_none() {
                return Err(anyhow!(
                    "explicit route policy requires preferred_provider or preferred_model"
                ));
            }
        }
    }
    Ok(())
}

fn apply_candidate_policy(candidates: &mut Vec<ModelCallCandidate>, policy: &TurnRoutePolicy) {
    if policy.mode == RouteMode::RemoteOnly {
        candidates.retain(|candidate| candidate.locality != ExecutionLocality::OnDevice);
    }
}

fn runtime_event(
    thread_id: &str,
    turn_id: &str,
    call_id: Option<&str>,
    event_type: OperatorEventType,
    payload: serde_json::Value,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: format!("evt-{}", Uuid::new_v4()),
        thread_id: thread_id.to_string(),
        turn_id: Some(turn_id.to_string()),
        run_id: None,
        call_id: call_id.map(str::to_string),
        event_type,
        occurred_at: now_iso(),
        actor: OperatorActor {
            kind: "runtime".to_string(),
            id: "operator-turn-runner".to_string(),
        },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: call_id.map(str::to_string),
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use heiwa_core::drex::{
        CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallRequest,
        ModelCallStage, PrivacyClass, SafetyClass,
    };
    use heiwa_evidence::{OperatorEventType, OperatorJournal, PersistedArtifact};
    use heiwa_protocol::{ExecutionScope, ModelTier, RiskClass, ToolLease};
    use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
    use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
    use serde_json::json;
    use tokio::sync::Notify;

    use super::{
        ActiveTurnRegistry, OperatorArtifactStore, OperatorModelExecutor, OperatorModelTurn,
        OperatorStreamFrame, OperatorTurnRunner, OperatorTurnWork,
    };
    use crate::model_calls::{
        ModelCallError, ModelCallExecution, ModelCallExecutor, ModelCallResult,
    };

    #[derive(Default)]
    struct RecordingExecutor {
        calls: AtomicUsize,
        entered_after: Mutex<Vec<OperatorEventType>>,
        sessions: Mutex<Option<Arc<OperatorSessionService>>>,
        started: Notify,
        block: AtomicBool,
    }

    struct SequencedExecutor {
        calls: AtomicUsize,
        responses: Vec<String>,
    }

    struct CompletingAdapter;

    #[async_trait]
    impl ProviderAdapter for CompletingAdapter {
        async fn send(
            &self,
            _model: &str,
            _messages: &[Message],
            stream_tx: tokio::sync::mpsc::Sender<StreamEvent>,
        ) -> anyhow::Result<()> {
            stream_tx
                .send(StreamEvent::Token("model answer".into()))
                .await?;
            stream_tx
                .send(StreamEvent::Done(TokenUsage::default()))
                .await?;
            Ok(())
        }

        async fn interrupt(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn supported_models(&self) -> Vec<String> {
            vec!["fake-model".into()]
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for SequencedExecutor {
        async fn execute(
            &self,
            _execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelCallResult {
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: self.responses[index].clone(),
                usage: TokenUsage::default(),
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.0,
                cost_truth: CostTruth::LocalZeroCost,
                attempt_records: vec![],
            })
        }
    }

    #[derive(Default)]
    struct RecordingArtifactStore {
        artifacts: Mutex<Vec<PersistedArtifact>>,
    }

    impl OperatorArtifactStore for RecordingArtifactStore {
        fn store(&self, artifact: PersistedArtifact) -> anyhow::Result<()> {
            self.artifacts.lock().unwrap().push(artifact);
            Ok(())
        }
    }

    impl RecordingExecutor {
        fn with_sessions(sessions: Arc<OperatorSessionService>) -> Self {
            Self {
                sessions: Mutex::new(Some(sessions)),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for RecordingExecutor {
        async fn execute(
            &self,
            mut execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(sessions) = self.sessions.lock().unwrap().clone() {
                let events = sessions
                    .events_after(&execution.request.thread_id, None, 64)
                    .unwrap();
                *self.entered_after.lock().unwrap() = events
                    .events
                    .iter()
                    .map(|row| row.event.event_type.clone())
                    .collect();
            }
            self.started.notify_waiters();
            while self.block.load(Ordering::SeqCst) {
                if *execution.cancel.borrow() {
                    return Err(ModelCallError::Cancelled);
                }
                if execution.cancel.changed().await.is_err() {
                    return Err(ModelCallError::Cancelled);
                }
            }
            Ok(ModelCallResult {
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: "model answer".into(),
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 2,
                    cost_usd: 0.0,
                    ..TokenUsage::default()
                },
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.0,
                cost_truth: CostTruth::LocalZeroCost,
                attempt_records: vec![],
            })
        }
    }

    fn service(path: &std::path::Path) -> Arc<OperatorSessionService> {
        Arc::new(OperatorSessionService::new(
            OperatorJournal::new(path.to_path_buf()).unwrap(),
        ))
    }

    fn model_turn() -> OperatorModelTurn {
        OperatorModelTurn {
            request: ModelCallRequest {
                thread_id: String::new(),
                turn_id: String::new(),
                call_id: "call-1".into(),
                intent: "chat".into(),
                stage: ModelCallStage::Execution,
                raw_text: "hello".into(),
                privacy: PrivacyClass::Standard,
                risk: CallRisk::Low,
                safety: SafetyClass::Approved,
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
            candidates: vec![],
            messages: vec![Message {
                role: Role::User,
                content: "hello".into(),
            }],
            remaining_budget_usd: None,
            max_attempts: 1,
            tool_scope: None,
            done_payload: Arc::new(|result| json!({"text": result.text})),
        }
    }

    async fn wait_for_terminal(
        receiver: &mut tokio::sync::broadcast::Receiver<OperatorStreamFrame>,
    ) {
        loop {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
                .await
                .expect("runner timed out")
                .expect("runner stream closed");
            if frame.is_terminal() {
                return;
            }
        }
    }

    #[test]
    fn operator_active_turn_registry_registers_signals_and_removes() {
        let registry = ActiveTurnRegistry::default();
        let receiver = registry.register("turn-1".into());
        assert!(!*receiver.borrow());
        assert!(registry.signal_cancel("turn-1"));
        assert!(*receiver.borrow());
        registry.remove("turn-1");
        assert!(!registry.signal_cancel("turn-1"));
    }

    #[tokio::test]
    async fn operator_deterministic_turn_is_durable_and_streams_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions.clone(), executor);

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Deterministic {
                    response: "deterministic answer".into(),
                    route: json!({"mode": "deterministic"}),
                    done: json!({"mode": "deterministic"}),
                },
            )
            .unwrap();
        wait_for_terminal(&mut handle.frames).await;

        let events = sessions.events_after("default", None, 64).unwrap();
        let types = events
            .events
            .iter()
            .map(|row| row.event.event_type.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            vec![
                OperatorEventType::ThreadCreated,
                OperatorEventType::TurnStarted,
                OperatorEventType::UserMessage,
                OperatorEventType::AssistantStarted,
                OperatorEventType::RoutePlanned,
                OperatorEventType::RouteCompleted,
                OperatorEventType::AssistantCompleted,
                OperatorEventType::ReceiptLinked,
                OperatorEventType::TurnCompleted,
            ]
        );
    }

    #[tokio::test]
    async fn operator_model_executor_enters_only_after_durable_intent_and_assistant_start() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        wait_for_terminal(&mut handle.frames).await;

        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *executor.entered_after.lock().unwrap(),
            vec![
                OperatorEventType::ThreadCreated,
                OperatorEventType::TurnStarted,
                OperatorEventType::UserMessage,
                OperatorEventType::AssistantStarted,
            ]
        );
    }

    #[tokio::test]
    async fn operator_model_backed_turn_persists_route_receipt_and_terminal_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let adapter: Arc<dyn ProviderAdapter> = Arc::new(CompletingAdapter);
        let resolver = Arc::new(move |_provider: &str, _model: &str| Some(adapter.clone()));
        let executor = Arc::new(ModelCallExecutor::new(resolver, sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions.clone(), executor);
        let mut turn = model_turn();
        turn.candidates = vec![ModelCallCandidate {
            tier: ModelTier {
                id: 1,
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                provider: "fake".into(),
                rate_group: "test".into(),
                capability_class: 3,
                effort_knob: "default".into(),
                effort_level: 1,
                cost_per_turn: 0.0,
                max_context_tokens: 8192,
                strengths_json: "[]".into(),
                vram_requirement_mb: 0,
                quantization_type: "none".into(),
                kv_cache_strategy: "none".into(),
                enabled: true,
                last_success_rate: 1.0,
                avg_latency_ms: 1,
                latency_p_95_ms: 1,
                updated_at: String::new(),
            },
            locality: ExecutionLocality::OnDevice,
            connected: true,
            adapter_capable: true,
            quota_available: true,
            marginal_cost_usd: Some(0.0),
            cost_truth: CostTruth::LocalZeroCost,
        }];
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        wait_for_terminal(&mut handle.frames).await;

        let types = sessions
            .events_after("default", None, 64)
            .unwrap()
            .events
            .into_iter()
            .map(|row| row.event.event_type)
            .collect::<Vec<_>>();
        assert!(types.windows(3).any(|events| {
            events
                == [
                    OperatorEventType::RoutePlanned,
                    OperatorEventType::RouteAttempted,
                    OperatorEventType::RouteCompleted,
                ]
        }));
        assert_eq!(
            types[types.len() - 3],
            OperatorEventType::AssistantCompleted
        );
        assert_eq!(types[types.len() - 2], OperatorEventType::ReceiptLinked);
        assert_eq!(types[types.len() - 1], OperatorEventType::TurnCompleted);
    }

    #[tokio::test]
    async fn operator_journal_append_failure_never_enters_executor() {
        let dir = tempfile::tempdir().unwrap();
        let evidence_path = dir.path().join("evidence");
        let sessions = service(&evidence_path);
        std::fs::remove_dir(&evidence_path).unwrap();
        std::fs::write(&evidence_path, "not a directory").unwrap();
        let executor = Arc::new(RecordingExecutor::default());
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        assert!(runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .is_err());
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn operator_duplicate_submission_does_not_spawn_twice() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        let first = runner
            .submit(
                "default",
                StartTurnRequest::auto("same-request", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        let second = runner
            .submit(
                "default",
                StartTurnRequest::auto("same-request", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        assert_eq!(first.turn_id, second.turn_id);
        assert!(!first.duplicate);
        assert!(second.duplicate);
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        runner.request_cancel(&first.turn_id).unwrap();
    }

    #[tokio::test]
    async fn operator_cancel_intent_is_durable_before_signal_and_runner_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        wait_for_terminal(&mut handle.frames).await;

        let events = sessions.events_after("default", None, 64).unwrap();
        let tail = &events.events[events.events.len() - 2..];
        assert_eq!(
            tail[0].event.event_type,
            OperatorEventType::TurnCancelRequested
        );
        assert_eq!(tail[1].event.event_type, OperatorEventType::TurnInterrupted);
        assert_eq!(tail[1].event.payload["reason"], "OPERATOR_CANCELLED");
        assert!(!runner.active_turns().signal_cancel(&handle.turn_id));
    }

    #[tokio::test]
    async fn operator_cancel_append_failure_leaves_execution_running() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions, executor.clone());
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();

        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        std::fs::rename(&stream, &backup).unwrap();
        std::fs::create_dir(&stream).unwrap();
        assert!(runner.request_cancel(&handle.turn_id).is_err());
        assert!(runner.active_turns().signal_cancel("missing") == false);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        wait_for_terminal(&mut handle.frames).await;
    }

    #[tokio::test]
    async fn operator_tools_are_intent_first_and_large_output_is_artifact_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let data = "x".repeat(20 * 1024);
        std::fs::write(dir.path().join("large.txt"), data).unwrap();
        let sessions = service(&dir.path().join("evidence"));
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                json!({"tool_calls": [{"id": "tool-1", "name": "fs.read", "arguments": {"path": "large.txt", "max_bytes": 32768}}]}).to_string(),
                "final answer".into(),
            ],
        });
        let artifacts = Arc::new(RecordingArtifactStore::default());
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone())
            .with_artifact_store(artifacts.clone());
        let mut turn = model_turn();
        let mut scope = ExecutionScope::local_default(dir.path().to_path_buf());
        scope.tool_leases.push(ToolLease {
            name: "fs.read".into(),
            risk_class: RiskClass::HostSafeReadonly,
            allowed: true,
        });
        turn.tool_scope = Some(scope);

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "read it"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        wait_for_terminal(&mut handle.frames).await;

        assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
        let events = sessions.events_after("default", None, 64).unwrap();
        let started = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ToolCallStarted)
            .unwrap();
        let completed = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ToolCallCompleted)
            .unwrap();
        assert!(started < completed);
        let completed_payload = &events.events[completed].event.payload;
        assert!(completed_payload["output_preview"].as_str().unwrap().len() <= 16 * 1024);
        assert!(completed_payload["artifact_ref"].as_str().is_some());
        let stored = artifacts.artifacts.lock().unwrap();
        assert_eq!(stored.len(), 1);
        assert!(stored[0].content_json.len() > 16 * 1024);
    }
}
